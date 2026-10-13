# Backup & Restore — Final Re-Review

**Date:** 2026-02-15  
**Reviewer:** Re-Review Agent  
**Initial Review:** `.github/docs/SubAgent docs/backup_restore_review.md`  
**Spec Reference:** `.github/docs/SubAgent docs/backup_restore_spec.md`  
**Status:** APPROVED

---

## Build Validation Results

| Check | Result | Details |
|-------|--------|---------|
| `cargo check` | ✅ PASS | Compiles with no errors |
| `cargo clippy -- -D warnings` | ✅ PASS | Only pre-existing MSRV mismatch warning (clippy.toml vs Cargo.toml) — not related to backup code |
| `npm run build` (frontend) | ✅ PASS | TypeScript compiles, Vite builds 68 modules, PWA assets generated |

**Build Result: SUCCESS**

---

## Verification of Initial Review Findings

### C1 (CRITICAL): Missing `password_reset_tokens` table in backup

**Status: ✅ RESOLVED**

- **`src/models/mod.rs` (line ~883):** `BackupDatabaseContent` struct now includes `password_reset_tokens: serde_json::Value` with `#[serde(default = "default_empty_json_array")]` for backward compatibility with older backup files that lack this field.
- **`src/db/mod.rs` (line 2712–2715):** `export_all_data()` now exports `password_reset_tokens` via `build_export_query("password_reset_tokens")`.
- **`src/db/mod.rs` (line 2735):** The struct assignment includes `password_reset_tokens`.
- **`src/db/mod.rs` (line 2754):** `truncate_order` includes `"password_reset_tokens"` (first in reverse-dependency order).
- **`src/db/mod.rs` (line 2794):** `import_order` includes `("password_reset_tokens", &data.password_reset_tokens)` (last in dependency order).

All three operations (export, truncate, import) now handle `password_reset_tokens`. The `#[serde(default)]` ensures restoring from pre-refinement backups won't fail. **Fully resolved.**

---

### R1 (RECOMMENDED): Admin auth deduplication

**Status: ✅ RESOLVED (with one fix applied during this re-review)**

- **`src/api/auth.rs` (line 96):** `require_admin` is now `pub async fn`, making it accessible from `backup.rs`.
- **5 of 6 handlers** (`create_backup`, `list_backups`, `upload_backup`, `restore_backup`, `delete_backup`) were already updated to use `auth::require_admin(&req, pool.get_ref()).await`.
- **`download_backup` (line 240)** was still using the old inline pattern (`get_auth_context_from_request` + manual `is_admin` check). **Fixed during this re-review** — now uses `auth::require_admin` consistently with all other handlers.
- All 6 handlers now use the unified `require_admin` helper, eliminating the duplicated auth boilerplate and the inconsistent error message ("Admin access required" vs "Admin privileges required").

---

### R2 (RECOMMENDED): Upload overwrite protection

**Status: ✅ RESOLVED**

- **`src/api/backup.rs` (lines ~395–410):** The upload handler now checks if the target filename already exists using `tokio::fs::try_exists()`. If a collision is detected, it appends a numeric suffix (`_1`, `_2`, …, up to `_100`) to generate a unique filename before writing.
- This prevents silent overwrites while still accepting the upload gracefully.

---

### R3 (RECOMMENDED): Misleading closure name

**Status: ✅ RESOLVED**

- **`src/db/mod.rs` (line 2646):** The closure has been renamed from `export_table` to `build_export_query`, accurately reflecting that it builds a SQL query string rather than performing the export.
- All 16 call sites (lines 2653–2713) use the renamed `build_export_query(...)`.

---

### R4 (RECOMMENDED): Frontend import placement

**Status: ✅ RESOLVED**

- **`frontend/src/services/api.ts` (line 52):** `BackupInfo` is now imported in the main import block at the top of the file, grouped with other type imports from `@/types`. No stray import at the bottom of the file.

---

### R5 (RECOMMENDED): `item_tags` sequence reset verification

**Status: ✅ CONFIRMED CORRECT**

- **Migration `006_create_tags_table.sql` (line 12):** `item_tags` is defined with `id SERIAL PRIMARY KEY`, confirming it has an auto-increment integer `id` column.
- Therefore, including `item_tags` in `sequence_tables` (line ~2826 of `db/mod.rs`) is correct — its sequence needs to be reset after import.
- The `setval` call with graceful error handling (`if let Err(e)`) is a sound defensive pattern.

---

## New Issues Discovered

### N1 (MINOR — fixed during review): `download_backup` missed R1 dedup

The `download_backup` handler was the only one still using the inline auth pattern after refinement. This has been corrected during this re-review — no remaining instances of inline admin checks in `backup.rs`.

No other new issues were discovered. The refinements were clean and did not introduce regressions.

---

## Specification Compliance Checklist (Updated)

| Spec Requirement | Status | Notes |
|------------------|--------|-------|
| POST /api/backup/create | ✅ | Includes metadata, admin-only |
| GET /api/backup/list | ✅ | Sorted newest first, file size |
| GET /api/backup/download/{filename} | ✅ | Content-Disposition attachment |
| POST /api/backup/upload | ✅ | Validates JSON, version, overwrite protection |
| POST /api/backup/restore/{filename} | ✅ | Transaction-based, deferred constraints |
| DELETE /api/backup/{filename} | ✅ | Validates filename, existence check |
| Admin-only restriction | ✅ | All 6 endpoints use `require_admin()` |
| Auto-backup before restore | ✅ | Abort restore if auto-backup fails |
| Path traversal prevention | ✅ | Validates `..`, `/`, `\`, prefix, extension |
| File upload size limit (100MB) | ✅ | `#[multipart(limit = "100MB")]` |
| JSON format (not ZIP) | ✅ | Pretty-printed JSON |
| Backup metadata envelope | ✅ | version, app_version, created_at, database_type |
| Transaction-based restore | ✅ | `SET CONSTRAINTS ALL DEFERRED`, TRUNCATE CASCADE |
| Sequence reset after import | ✅ | `setval(pg_get_serial_sequence(...))` |
| **All 17 tables exported** | ✅ | **Now includes `password_reset_tokens`** |
| BackupInfo/BackupMetadata models | ✅ | Rust structs + TS interfaces |
| Frontend BackupRestoreSection | ✅ | Create, upload, download, restore, delete |
| SettingsPage integration | ✅ | Admin-only section with icon, description |
| Restore confirmation modal | ✅ | Warning text, danger button |
| Delete confirmation modal | ✅ | Confirmation dialog |
| `backupApi` service object | ✅ | All 6 methods, proper auth headers |
| Route registration in `api_scope` | ✅ | All 6 services registered |
| `actix-multipart` dependency | ✅ | Pinned `=0.7.2` |

---

## Updated Summary Score Table

| Category | Initial Score | Final Score | Grade | Change |
|----------|:------------:|:-----------:|:-----:|:------:|
| Specification Compliance | 93% | 100% | A+ | +7% |
| Best Practices | 90% | 97% | A+ | +7% |
| Functionality | 98% | 100% | A+ | +2% |
| Code Quality | 92% | 97% | A+ | +5% |
| Security | 95% | 96% | A+ | +1% |
| Performance | 88% | 88% | B+ | — |
| Consistency | 90% | 98% | A+ | +8% |
| Build Success | 100% | 100% | A+ | — |

**Overall Grade: A+ (97%)**

*Performance remains at B+ due to the 16 sequential DB queries in `export_all_data()` (could use `tokio::try_join!` for parallelism). This is an optional future optimization, not a correctness concern.*

---

## Remaining Optional Items (from initial review, unchanged)

These were categorized as OPTIONAL in the initial review and remain as future considerations:

- **O1:** Backup files contain password hashes — consider stripping or encrypting in a future version
- **O2:** No limit on number of backup files — consider auto-cleanup policy
- **O3:** `format_file_size` uses `as f64` cast — non-issue in practice, properly annotated
- **O4:** Blob URL revocation timing — works in practice, `setTimeout` would be marginally safer

---

## Final Assessment: **APPROVED**

All CRITICAL and RECOMMENDED issues from the initial review have been verified as resolved:

| Finding | Severity | Status |
|---------|----------|--------|
| C1: Missing `password_reset_tokens` | CRITICAL | ✅ Resolved |
| R1: Admin auth deduplication | RECOMMENDED | ✅ Resolved (final handler fixed in re-review) |
| R2: Upload overwrite protection | RECOMMENDED | ✅ Resolved |
| R3: Misleading closure name | RECOMMENDED | ✅ Resolved |
| R4: Import placement in api.ts | RECOMMENDED | ✅ Resolved |
| R5: `item_tags` sequence reset | RECOMMENDED | ✅ Confirmed correct |

The implementation fully meets all original spec requirements, builds cleanly, passes clippy with zero backup-related warnings, and the frontend compiles and bundles successfully. The code is consistent with existing codebase patterns and ready for integration.
