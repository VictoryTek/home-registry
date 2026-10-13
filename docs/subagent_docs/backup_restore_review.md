# Backup & Restore — Code Review

**Date:** 2026-02-15  
**Reviewer:** Code Review Agent  
**Spec Reference:** `.github/docs/SubAgent docs/backup_restore_spec.md`  
**Status:** NEEDS_REFINEMENT

---

## Build Validation Results

| Check | Result | Details |
|-------|--------|---------|
| `cargo check` | ✅ PASS | Compiles with no errors |
| `cargo clippy -- -D warnings` | ✅ PASS | Only non-related MSRV mismatch warning (clippy.toml vs Cargo.toml) |
| `cargo test --no-run` | ✅ PASS | All test targets compile |
| `npm run build` (frontend) | ✅ PASS | TypeScript compiles, Vite builds 68 modules successfully |

**Build Result: SUCCESS**

---

## Findings

### CRITICAL Issues

#### C1: Missing `password_reset_tokens` table in backup

**Files:** `src/models/mod.rs` (line ~862), `src/db/mod.rs` (lines 2640–2735, 2736–2840)

The spec (section 2.1) explicitly lists `password_reset_tokens` as a table that "must be included in backups." However, the `BackupDatabaseContent` struct and both `export_all_data()` / `import_all_data()` methods omit this table entirely.

If a user restores from backup, all password reset tokens will be lost. While these are typically short-lived, data completeness is a stated requirement.

**Fix:** Add `password_reset_tokens: serde_json::Value` to `BackupDatabaseContent`, and add the table to export/import/truncate sequences in `db/mod.rs`.

---

### RECOMMENDED Issues

#### R1: Duplicated admin authorization pattern across 6 handlers

**File:** `src/api/backup.rs` (lines 119–128, 161–170, 235–244, 336–345, 458–467, 575–584)

Every backup handler duplicates the same auth + admin check:

```rust
let auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
    Ok(a) => a,
    Err(e) => return Ok(e),
};
if !auth.is_admin {
    return Ok(HttpResponse::Forbidden().json(ErrorResponse { ... }));
}
```

`src/api/auth.rs` already has a private `require_admin()` helper (line 96) that combines both checks. The backup module can't use it because it's not public.

**Fix:** Make `require_admin` in `auth.rs` public (`pub async fn`), then use it in all 6 backup handlers to reduce ~72 lines of boilerplate:

```rust
let auth = match auth::require_admin(&req, pool.get_ref()).await {
    Ok(a) => a,
    Err(e) => return Ok(e),
};
```

Also note the error messages differ slightly: backup.rs uses "Admin access required" while auth.rs uses "Admin privileges required". Unifying through a shared helper would ensure consistency.

---

#### R2: Uploaded backup can silently overwrite existing file

**File:** `src/api/backup.rs` (lines 404–405)

When a user uploads a backup whose validated filename matches an existing backup, `tokio::fs::write` silently overwrites it. There is no check or warning.

**Fix:** Check if the target file already exists before writing. Either:
- Return an error if the filename already exists, or
- Append a suffix (e.g., `_1`, `_2`) to make the name unique, or
- Log a warning and proceed (if overwrite is intentional behavior)

---

#### R3: `export_table` closure is misleading — it only builds a string

**File:** `src/db/mod.rs` (lines 2649–2653)

```rust
let export_table = |table: &str| {
    let query = format!(
        "SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb) FROM {table} t"
    );
    query
};
```

This closure returns a `String` but never executes the query. Each call site then runs `client.query_one(&export_table("table_name"), &[]).await?.get(0)`. While functionally correct, the closure name `export_table` is misleading — it doesn't export anything, it just builds SQL text.

**Fix:** Rename to `build_export_query` or refactor into a helper function, or better yet, extract the repeated `client.query_one(...).await?.get(0)` pattern into an `async` helper:

```rust
async fn export_table(client: &tokio_postgres::Client, table: &str) -> Result<serde_json::Value, ...> {
    let query = format!("SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb) FROM {table} t");
    Ok(client.query_one(&query, &[]).await?.get(0))
}
```

This would reduce ~45 lines of repetitive code to 15 one-liners.

---

#### R4: Frontend API import placed at bottom of file instead of top

**File:** `frontend/src/services/api.ts` (line 866)

```typescript
import type { BackupInfo } from '@/types';
```

This import statement appears mid-file (after line 863), following the existing exports. TypeScript/ESLint best practice is to group all imports at the top of the file. While this works, it could cause confusion.

**Fix:** Move the `BackupInfo` import to the existing import block at the top of `api.ts`.

---

#### R5: Sequence reset in `import_all_data` missing `user_settings` table

**File:** `src/db/mod.rs` (lines 2816–2827)

The `sequence_tables` array for resetting auto-increment sequences after import does not include `user_settings`, `inventory_shares`, `user_access_grants`, or `recovery_codes`. These tables use UUID primary keys, so they don't have integer sequences — this is correct.

However, `item_tags` is listed in `sequence_tables` but `item_tags` might not have an integer `id` primary key (it's a junction table). If it uses a composite key instead, the `setval` call would fail.

The code does handle `setval` failures gracefully with `if let Err(e)`, so this is non-breaking. But it logs misleading notes like "Could not reset sequence... (this may be expected)" for tables that genuinely don't have sequences.

**Fix:** Only include tables that are confirmed to have serial/identity `id` columns. Verify `item_tags` schema.

---

### OPTIONAL Issues

#### O1: Backup files contain password hashes in plaintext

**Files:** `src/db/mod.rs` (export includes `users` table with `password_hash`)

The `export_all_data()` method exports the full `users` table which includes Argon2id password hashes. The spec acknowledges this risk (section 13) but doesn't mitigate it beyond admin-only access.

**Consideration:** In a future version, consider either:
- Stripping `password_hash` from exported user records, or
- Encrypting the entire backup file with a user-provided passphrase

---

#### O2: No limit on number of backup files

The implementation creates new files on every backup/auto-backup but never cleans up old ones. Over time, this could consume significant disk space, especially with frequent auto-pre-restore backups.

**Consideration:** Add optional auto-cleanup (e.g., keep only last N backups, or delete auto-backups older than X days).

---

#### O3: `format_file_size` function could use `f64::from()` instead of `as`

**File:** `src/api/backup.rs` (lines 55–66)

The `#[allow(clippy::cast_precision_loss)]` suppression is correct but the function uses `bytes as f64` for the cast. Since `bytes` is `u64`, precision loss is possible for very large files. The existing `#[allow]` annotation handles this correctly, but for perfectionist style, `f64::from(u32)` would be lossless for smaller values.

This is a non-issue in practice — backup files won't exceed f64 precision limits.

---

#### O4: Download blob URL revocation timing

**File:** `frontend/src/services/api.ts` (lines 904–907)

```typescript
link.click();
document.body.removeChild(link);
URL.revokeObjectURL(link.href);
```

`URL.revokeObjectURL` is called immediately after `link.click()`. In most browsers, the download is initiated synchronously so this works. For maximum safety, a small `setTimeout` (e.g., 100ms) before revoking would be more robust.

---

## Specification Compliance Checklist

| Spec Requirement | Status | Notes |
|------------------|--------|-------|
| POST /api/backup/create | ✅ Implemented | Matches spec, includes metadata |
| GET /api/backup/list | ✅ Implemented | Sorts newest first, includes size |
| GET /api/backup/download/{filename} | ✅ Implemented | Content-Disposition attachment header |
| POST /api/backup/upload | ✅ Implemented | Validates JSON structure, version |
| POST /api/backup/restore/{filename} | ✅ Implemented | Transaction-based, deferred constraints |
| DELETE /api/backup/{filename} | ✅ Implemented | Validates filename, checks exists |
| Admin-only restriction | ✅ Implemented | All 6 endpoints check `is_admin` |
| Auto-backup before restore | ✅ Implemented | Aborts restore if auto-backup fails |
| Path traversal prevention | ✅ Implemented | Checks for `..`, `/`, `\`, prefix, extension |
| File upload size limit (100MB) | ✅ Implemented | Via `#[multipart(limit = "100MB")]` |
| JSON format (not ZIP) | ✅ Implemented | Pretty-printed JSON |
| Backup metadata envelope | ✅ Implemented | version, app_version, created_at, database_type |
| Transaction-based restore | ✅ Implemented | `SET CONSTRAINTS ALL DEFERRED`, TRUNCATE CASCADE, commit |
| Sequence reset after import | ✅ Implemented | `setval(pg_get_serial_sequence(...))` |
| BackupInfo/BackupMetadata models | ✅ Implemented | Both Rust structs and TS interfaces |
| Frontend BackupRestoreSection | ✅ Implemented | Create, upload, download, restore, delete |
| SettingsPage integration | ✅ Implemented | Admin-only section with icon, description |
| Restore confirmation modal | ✅ Implemented | Warning text, danger button |
| Delete confirmation modal | ✅ Implemented | Confirmation dialog |
| Error responses use ErrorResponse | ✅ Implemented | Consistent across all handlers |
| Logging with `log` crate | ✅ Implemented | `info!` and `error!` calls |
| `backupApi` service object | ✅ Implemented | All 6 methods, proper auth headers |
| Component export in index.ts | ✅ Implemented | Line 16 |
| Route registration in api_scope | ✅ Implemented | All 6 services registered |
| `actix-multipart` dependency | ✅ Implemented | Pinned `=0.7.2` |
| `password_reset_tokens` backup | ❌ Missing | Spec section 2.1 requires it (see C1) |
| Existing `require_admin` reuse | ❌ Not used | Private in auth.rs, duplicated in backup.rs (see R1) |

---

## Code Quality Analysis

### Backend (Rust)

**Strengths:**
- Clean module separation — `backup.rs` is self-contained with clear doc comments
- Proper error handling — no `unwrap()` or `expect()` in production code
- Consistent use of `ApiResponse<T>` and `ErrorResponse` patterns
- Security: filename validation, admin checks, upload size limits, version validation
- Auto-backup before restore is a well-implemented safety net
- `#[allow(clippy::cast_precision_loss)]` properly annotated where needed
- Helper functions (`validate_backup_filename`, `format_file_size`, `generate_backup_filename`, `ensure_backups_dir`, `create_backup_file`) provide good abstraction

**Concerns:**
- 6 handlers × ~12 lines of auth boilerplate = ~72 lines that could be deduplicated
- `export_table` closure name is misleading
- `export_all_data` has 15 sequential database queries — could be parallelized with `tokio::try_join!` for performance

### Frontend (TypeScript/React)

**Strengths:**
- Clean component structure with proper state management
- All async operations have loading states and error handling
- File input reset after upload (`e.target.value = ''`)
- Buttons disabled during operations to prevent double-clicks
- Restoring indicator banner warns user not to close the page
- `ConfirmModal` reuse follows existing patterns exactly
- Download uses `URL.createObjectURL` + cleanup pattern correctly
- Upload correctly omits `Content-Type` header (letting browser set multipart boundary)

**Concerns:**
- Import placement at bottom of `api.ts` (style issue only)

### Consistency with Existing Codebase

- ✅ Uses same `DatabaseService::new(pool)` pattern
- ✅ Uses same `auth::get_auth_context_from_request()` pattern
- ✅ Uses same `ApiResponse` / `ErrorResponse` types
- ✅ Uses same logging approach (`info!`, `error!`)
- ✅ Frontend follows same `useAuth()`, `useApp()`, `showToast()` patterns
- ✅ Settings page integration matches existing section layout
- ✅ Actix-Web macro style (`#[get]`, `#[post]`, `#[delete]`) matches existing handlers
- ⚠️ Admin error message text differs from auth.rs ("Admin access required" vs "Admin privileges required")

---

## Summary Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 93% | A |
| Best Practices | 90% | A- |
| Functionality | 98% | A+ |
| Code Quality | 92% | A |
| Security | 95% | A |
| Performance | 88% | B+ |
| Consistency | 90% | A- |
| Build Success | 100% | A+ |

**Overall Grade: A- (93%)**

---

## Overall Assessment: **NEEDS_REFINEMENT**

While the implementation is solid with all builds passing and near-complete spec compliance, the missing `password_reset_tokens` table (C1) is a data completeness gap that contradicts the spec's explicit requirements. Combined with the RECOMMENDED items (particularly R1 for code deduplication and R2 for overwrite safety), a refinement pass is warranted.

### Priority Recommendations (ordered)

1. **C1** — Add `password_reset_tokens` to `BackupDatabaseContent` struct and export/import methods
2. **R1** — Make `require_admin` in `auth.rs` public and use it in all backup handlers
3. **R2** — Add overwrite check for uploaded backup files
4. **R4** — Move `BackupInfo` import to top of `api.ts`
5. **R3** — Rename/refactor `export_table` closure in `db/mod.rs`
6. **R5** — Verify `item_tags` has a serial `id` column; clean up sequence reset list

### Affected File Paths

- `c:\Projects\home-registry\src\models\mod.rs`
- `c:\Projects\home-registry\src\db\mod.rs`
- `c:\Projects\home-registry\src\api\backup.rs`
- `c:\Projects\home-registry\src\api\auth.rs`
- `c:\Projects\home-registry\frontend\src\services\api.ts`
