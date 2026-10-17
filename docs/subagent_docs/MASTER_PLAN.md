# MASTER PLAN — home-registry

Consolidated from [ANALYSIS_ARCH.md](ANALYSIS_ARCH.md), [ANALYSIS_BUGS.md](ANALYSIS_BUGS.md), and [ANALYSIS_FEATURES.md](ANALYSIS_FEATURES.md) (baseline `main` @ `89e1e94`). Duplicates are merged into one task. Source IDs are prefixed with the report they come from, because ARCH and FEATURES both use `A1`, `H1` and so on: **ARCH-A1**, **BUGS-SEC1**, **FEAT-F1**. Each points back to the full write-up (with file and line references) in that report.

## How we work through this list
1. Pick the first unchecked item (top to bottom; order inside a tier already reflects dependencies).
2. I show the proposed change (files, diff summary, tests, risks). **Nothing is applied until you approve.**
3. Apply → run the verification commands from `CLAUDE.md` → tick the box → move to the next.
4. Tier 3 items (security, schema/migration, public interface, many files) also get a `reviewer` pass before the commit message. You run all git commands.

**Progress:** 3 / 19 high · 0 / 46 medium · 0 / 25 low

---

## HIGH priority

### Security and access control (do these first; nothing user-facing should be built on top of unprotected endpoints)

- [x] **H-01 — Require authentication and per-inventory authorization on every data endpoint**
  Sources: ARCH-A1, BUGS-SEC1. Files: `src/api/mod.rs:140-950` (inventory, item, organizer, option, organizer-value handlers), `src/db/mod.rs:126-160, 391-437`.
  Today `GET /api/items`, `PUT/DELETE /api/inventories/{id}`, all item and organizer routes work with **no token**. Add a shared authorize helper that resolves item/organizer/option → inventory, loads `EffectivePermissions`, and returns 401/403/404; scope `GET /items` and `search` to accessible inventories. Frontend already sends the bearer token.
- [x] **H-02 — Ownership checks on share update/delete, image delete, and item-images listing**
  Source: BUGS-SEC2. Files: `src/api/auth.rs:1345-1432`, `src/api/mod.rs:1115-1215`.
  A `view` sharee can raise their own share to `edit_inventory`; any user can delete any share or any uploaded file. Require `can_manage_sharing` (or admin) on the share's inventory; require `can_view` for item-images; for image delete, require that the file is referenced by an item the caller can edit.
- [x] **H-03 — TOTP must fail closed at login**
  Source: BUGS-SEC3. File: `src/api/auth.rs:321-357`. `get_totp_settings(...).ok().flatten()` and the `if let Ok(mode)` parse both fall through to a full token on error. Return 500 on lookup error and treat an unknown mode as "TOTP required".
- [ ] **H-04 — Require the current password to regenerate recovery codes**
  Source: BUGS-SEC4. Files: `src/api/auth.rs:1923-1994`, frontend `RecoveryCodesSection`. A stolen JWT currently regenerates codes, resets the password, and locks the owner out.
- [ ] **H-05 — Rate limiter fix + bound unauthenticated Argon2 work**
  Sources: ARCH-A4, BUGS-SEC5, BUGS-SEC14 (rate-limit part). Files: `src/main.rs:130-170`, `src/api/auth.rs:2174-2185`.
  Build the limiter backend once (not per worker), correct `interval/max_requests` (currently ~5,000 req/s), guard `RATE_LIMIT_RPS=0`, key on real client IP behind a trusted proxy, and add per-account/per-IP attempt limits on login/register/recovery. Replace "verify against 10 hashes" for recovery codes with a single lookup.
- [ ] **H-06 — Token versioning, auth context from the DB user, and "sign out everywhere"**
  Sources: ARCH-A5, BUGS-SEC8, FEAT-G1. Files: `src/auth/mod.rs:143-217, 280-288`, `src/api/auth.rs:63-102`, new migration (`users.token_version`).
  Build `AuthContext.is_admin` from the loaded row (demoted admins lose access immediately), invalidate tokens on password change/reset/recovery/deactivation, add the Settings button.
- [ ] **H-07 — One source of truth for "who can access this inventory"**
  Source: ARCH-A2. Files: `src/db/mod.rs:1765-1869, 1919-1961, 2362-2384, 2397-2500`. Four copies of the owner/share/grant rule; `get_effective_permissions` falls back to `View` on a parse error (fails open). Consolidate in SQL (view/function) and have the Rust permission object derive from it.

### Data integrity

- [ ] **H-08 — Backup/restore correctness (2FA wipe, consistent snapshot, schema drift)**
  Sources: ARCH-A3, BUGS-BUG2, BUGS-BUG14. Files: `src/db/mod.rs:2694-2894`, `src/models/mod.rs:1026-1050`, `src/api/backup.rs`.
  Include `user_totp_settings`; derive the table list from the catalog; take the export in one `REPEATABLE READ` transaction; store the schema version in the backup metadata; fix the sequence-reset error path; make backup filenames collision-safe. (Backing up `uploads/` is tracked in **M-19**.)
- [ ] **H-09 — Drop the six orphaned tables and their dead models**
  Source: ARCH-H1 (plus the model/DEAD1 part of BUGS-DEAD1). Files: `migrations/V005-V007, V013`, `src/models/mod.rs:215-312`, `src/db/mod.rs:2805-2876`. New migration `V024` drops `categories`, `tags`, `item_tags`, `custom_fields`, `item_custom_values`, `password_reset_tokens`; remove from export/import. **Note:** FEAT-F3 (invite links) needs a new token table; do not reuse the dropped one.
- [ ] **H-10 — Items must not default into inventory #1**
  Sources: BUGS-BUG3, ARCH-H4 (the `DEFAULT 1` part). Files: `src/db/mod.rs:225`, `src/models/mod.rs:60`, `migrations/V001-V003`. Make `inventory_id` required in `CreateItemRequest`, and drop the column default in a new migration.

### Core bugs

- [ ] **H-11 — JSON error handler never runs (config overridden at `/api` scope)**
  Source: ARCH-P1. Files: `src/main.rs:192-208`, `src/api/mod.rs:1652-1653`. Configure `JsonConfig`/`PayloadConfig` once; malformed JSON should return the standard envelope, without echoing internals.
- [ ] **H-12 — Dates display one day early west of UTC; warranty countdown off by one**
  Source: BUGS-BUG1. Files: `frontend/src/utils/dateFormat.ts:22-48`, `frontend/src/utils/notifications.ts:30-31, 48-52`. Parse `YYYY-MM-DD` as a local date.

### High-value features

- [ ] **H-13 — Registration mode + invite and reset links**
  Sources: BUGS-SEC12, FEAT-F3. Files: `src/api/auth.rs:385-511`, new `app_settings` + invite-token migration, `UserManagement`, `RegisterPage`. `open/invite/closed`, single-use hashed tokens with expiry, admin reset via link instead of typing a password.
- [ ] **H-14 — Organizer-aware reports (replaces the "always Uncategorized" breakdown)**
  Sources: FEAT-F2, ARCH-H2, BUGS-BUG9. Files: `src/db/mod.rs:2397-2690`, `InventoryReportPage.tsx`. Group-by organizer, organizer filters, organizer columns in CSV, and make statistics/breakdown honour the same filters as the item list.
- [ ] **H-15 — Item search, filter and sort UI**
  Sources: FEAT-F1, BUGS-BUG17 (search path/`/` bug). Needs **H-01** (scoped search). Client-side filter on the detail page, scoped global search via `?q=`, organizer chips, sort control.
- [ ] **H-16 — Phone-friendly item photos (HEIC, compression, camera capture)**
  Source: FEAT-C3. File: `frontend/src/components/ImageOrganizerInput.tsx:20-53, 161`. Reuse `convertHeicToJpeg` + `compressImage`; add `capture="environment"`.
- [ ] **H-17 — Move and duplicate items between inventories**
  Source: FEAT-C1. Needs **H-01**. Server checks `can_remove_items` (source) and `can_add_items` (destination); re-map organizer values by name; add `POST /items/{id}/duplicate`.
- [ ] **H-18 — CSV import mirroring the export**
  Source: FEAT-C2. Needs **H-01, M-10**. Preview + commit in one transaction; template download.
- [ ] **H-19 — Scheduled backups with retention**
  Source: FEAT-A1. Needs **H-08**. `BACKUP_INTERVAL_HOURS` + `BACKUP_RETENTION`, tokio task after migrations, "last/next backup" in the UI.

---

## MEDIUM priority

### Security hardening
- [ ] **M-01 — TOTP replay protection and atomic attempt counting** — BUGS-SEC6 (+ the `totp_setup` counter reset). `src/api/totp.rs`, `src/db/mod.rs:3048-3085`.
  Note: RFC 6238 §5.2 says a verifier MUST NOT accept the same OTP twice. `totp-rs` is pinned at `=5.6.0`, whose `check()` returns a bool (no matched step), so store and compare the last-used step ourselves; do not copy the 6.x `check_current` examples. Also fix `totp_setup`'s "already enabled" check, which skips on a DB error (`src/api/totp.rs:60`).
- [ ] **M-02 — Changing/downgrading TOTP mode requires re-authentication** — BUGS-SEC7. `src/api/totp.rs:524-580`.
- [ ] **M-03 — CSV formula injection** — BUGS-SEC9. `src/api/mod.rs:1231-1289`; prefix `= + - @ \t \r` cells.
- [ ] **M-04 — Account enumeration and login timing** — BUGS-SEC10. Dummy-hash verify for unknown users; check `is_active` after the password.
- [ ] **M-05 — Typed `AppError` (`ResponseError`), proper DB error classification, no internal details to clients** — ARCH-A7, BUGS-ERR3, BUGS-SEC11. Replaces 125 hand-built 500s and string-matching on `"duplicate"`/`"unique"`.
- [ ] **M-06 — Stop swallowing security-relevant write failures (`let _ =`)** — BUGS-ERR1. Counter increments/resets, `mark_recovery_code_used`, `create_user_settings`, `try_exists().unwrap_or(false)`.
- [ ] **M-07 — Cookie-auth fallback and localhost CORS: finish or remove** — ARCH-H3. `src/auth/mod.rs:219-237`, `src/main.rs:170-188`.
- [ ] **M-08 — Reduce XSS blast radius: CSP without `unsafe-inline/unsafe-eval`, token handling** — BUGS-SEC13 (self-hosting fonts/icons from ARCH-D6 is the enabler).

### Correctness
- [ ] **M-09 — Recovery codes: atomic one-time use and atomic regeneration** — BUGS-BUG10. `UPDATE ... WHERE is_used = false`, transaction around delete/insert/update.
- [ ] **M-10 — One validation strategy: DB column limits, enum checks, strict dates (400 instead of silent NULL / 500)** — ARCH-P3, BUGS-BUG5, BUGS-BUG6.
- [ ] **M-11 — Organizer value validation (type ↔ inventory ↔ option ↔ input_type) and transactional batch set** — BUGS-BUG15.
- [ ] **M-12 — Transactions for multi-step writes; report partial failures honestly** — ARCH-P2, BUGS-ERR2 (setup/register, admin update, organizer values, item create + values toast).
- [ ] **M-13 — Edit form can clear price and organizer values** — BUGS-BUG7. `InventoryDetailPage.tsx:312-325, 1166-1170`.
- [ ] **M-14 — Image removal deletes the file before save; deleted items/inventories orphan files** — BUGS-BUG8.
- [ ] **M-15 — Wrong password when disabling 2FA logs the user out (401 misuse)** — BUGS-BUG4.
- [ ] **M-16 — Transient errors/offline log everyone out at startup** — BUGS-BUG12. Only 401 clears the session.
- [ ] **M-17 — Unknown `currency` crashes price pages; add a React error boundary** — BUGS-BUG16, BUGS-ERR4 (boundary part).
- [ ] **M-18 — Clear client state and service-worker API cache on logout/user switch** — ARCH-A12, BUGS-BUG13.
- [ ] **M-19 — Unify image storage (files, not base64 in Postgres) and include `uploads/` in backups** — ARCH-S2, BUGS-PERF1, (backup part of ARCH-A3). Needs **H-08**.
  Also: record the uploader per file (so unreferenced uploads are deletable only by their uploader; H-02 leaves them deletable by any signed-in user) and serve `/uploads/img/*` behind authentication.
- [ ] **M-20 — Retry only idempotent requests** — ARCH-A13. `frontend/src/services/api.ts:198-262`.
- [ ] **M-21 — Server-side notifications endpoint; fix the empty Notifications page and split UI vs data context** — ARCH-A11, BUGS-BUG11.
- [ ] **M-22 — N+1 requests and queries; per-inventory item counts from the server** — BUGS-PERF2 (pairs with FEAT-F7).
- [ ] **M-23 — Money as exact decimal (or drop `rust_decimal`)** — ARCH-P4, ARCH-D1. Decide: migrate `purchase_price` to `NUMERIC` and use the crate, or remove the dependency.

### Structure and configuration
- [ ] **M-24 — Split `api/mod.rs`, `db/mod.rs`, `api/auth.rs`; `DatabaseService` registered once; `TryFrom<&Row>` for `Item`** — ARCH-A6, ARCH-S1.
- [ ] **M-25 — Single runtime path config (`DATA_DIR`, `STATIC_DIR`)** — ARCH-A8.
- [ ] **M-26 — Secrets bootstrap: correct precedence, independent TOTP key** — ARCH-A9.
- [ ] **M-27 — Real `DATABASE_URL` parsing (and TLS option), shared with tests** — ARCH-A10.
- [ ] **M-28 — Generate TypeScript types from Rust (`null` vs `undefined`, naming drift)** — ARCH-S3.
- [ ] **M-29 — Document/repair sample-data migration history; make `inventories.user_id` NOT NULL after backfill** — ARCH-H4 (non-`DEFAULT 1` part).

### Dependencies and release hygiene
- [ ] **M-30 — Enforce pinning policy (`refinery`, `time`, npm carets, `rust:1-alpine`, unpinned Actions)** — ARCH-D2.
- [ ] **M-31 — Resolve MIT vs GPL-3.0 metadata and placeholder repository URL** — ARCH-D3.
- [ ] **M-32 — Version check blocked by CSP; single source of truth for the version string; fix pre-release comparison** — ARCH-D4, BUGS-BUG17 (version items).

### Features
- [ ] **M-33 — Pagination and list/table view using `items_per_page`** — FEAT-F4.
- [ ] **M-34 — Reorder organizers and options** — FEAT-F5.
- [ ] **M-35 — Snooze, warning window, batch dismiss, sidebar badge for notifications** — FEAT-F6. After **M-21**.
- [ ] **M-36 — Cross-inventory reports route and home dashboard** — FEAT-F7.
- [ ] **M-37 — Number and date organizer types** — FEAT-C4.
- [ ] **M-38 — Maintenance reminders** — FEAT-C5. After **M-37, M-21**.
- [ ] **M-39 — Receipt/manual (PDF) attachments** — FEAT-C6. After **M-19**.
- [ ] **M-40 — Copy organizers from another inventory** — FEAT-C7.
- [ ] **M-41 — Insurance/claim report with photos** — FEAT-C8. After **H-14**.
- [ ] **M-42 — Printable QR labels** — FEAT-C9. Pairs with FEAT-F8 (L-20).
- [ ] **M-43 — Bulk item actions** — FEAT-C10. After **H-17**.
- [ ] **M-44 — Personal data export for non-admin users** — FEAT-G2.
- [ ] **M-45 — Activity history on shared inventories** — FEAT-G3. After **M-12**.
- [ ] **M-46 — Warranty/maintenance webhook alerts** — FEAT-A2. After **H-19, M-21**.

---

## LOW priority

### Hygiene and cleanup
- [ ] **L-01 — Untrack `.env`, `package-lock.json.backup`, `tsconfig.tsbuildinfo`; fix test-DB credential drift** — ARCH-S6.
- [ ] **L-02 — Lint configuration in one place (`[lints]`)** — ARCH-S7.
- [ ] **L-03 — Route/static-file duplication in `main.rs`, `App.tsx`, test harnesses (use `api_scope()` in tests)** — ARCH-S4.
- [ ] **L-04 — Naming consistency (handler params, `*_endpoint`, `init_routes` alias, docs layout)** — ARCH-S5.
- [ ] **L-05 — Token/storage keys and `AuthContext` bypass in Register/Setup pages** — ARCH-P5, BUGS-ERR2 (frontend part).
- [ ] **L-06 — Raw `fetch` vs `fetchWithRetry`; duplicate health endpoints** — ARCH-P6, BUGS-ERR4 (fetch part).
- [ ] **L-07 — Preflight scripts vs CI (coverage, `--include-ignored`, MSRV, migrations via refinery)** — ARCH-P7.
- [ ] **L-08 — Remove dead code: backend + frontend** — ARCH-H5, BUGS-DEAD1, BUGS-DEAD2 (orphaned model structs are covered by **H-09**).
- [ ] **L-09 — Replace the stub `integration_test.rs` with real `/health` coverage** — ARCH-H6.
- [ ] **L-10 — Remove routes for non-existent logo files; drop unused `logo_full2.png`** — ARCH-H7.
- [ ] **L-11 — Redundant/no-op code (`::float8`, `SET CONSTRAINTS ALL DEFERRED`, redundant JOIN, image magic-byte length checks, backup upload limit 50 vs 100 MB)** — BUGS-DEAD3 (items already fixed by H-08/H-11 drop out).
- [ ] **L-12 — Crate hygiene: `validator` usage, two RNG paths, `tokio` `full`, log format vs docs** — ARCH-D5.
- [ ] **L-13 — Self-host fonts and icons; remove CDN allowances** — ARCH-D6 (enabler for **M-08**).

### Edge cases and polish
- [ ] **L-14 — First-run setup race; `JWT_TOKEN_LIFETIME_HOURS` bounds; short `JWT_SECRET` silently replaced** — BUGS-SEC14 (non-rate-limit part).
- [ ] **L-15 — Backups contain password and recovery-code hashes; encrypt or exclude** — BUGS-SEC15.
- [ ] **L-16 — Smaller logic defects list** — BUGS-BUG17 (excluding search/version items already assigned): quantity 0, AboutDialog stale closure, stale form state across `await`, out-of-order loads, `history.replaceState`, toast IDs/timers, dismissal state mutation and pruning, `settings_json` NULL decode, `default_inventory_id` validation, last-admin race, admin reset ordering, print/close, PNG transparency.
- [ ] **L-17 — Smaller performance items (extra auth round-trip, trigram search index, context `useMemo`, streaming CSV/backup, version-check frequency, `info!` log volume)** — BUGS-PERF3.
- [ ] **L-18 — Remaining frontend error handling (silent `catch {}`, `handleResponse` logging, version-check failures)** — BUGS-ERR4 (remainder).

### Features
- [ ] **L-19 — Small form/display gaps (Notes on add form, card totals, location display)** — FEAT-G4.
- [ ] **L-20 — Item deep-link route `/inventory/:id/item/:itemId`** — FEAT-F8.
- [ ] **L-21 — Server-synced theme (+ `system`)** — FEAT-F9.
- [ ] **L-22 — `/metrics` endpoint (or remove the scrape job from docs)** — FEAT-F10.
- [ ] **L-23 — PWA share target and app shortcuts** — FEAT-A3.
- [ ] **L-24 — Personal API tokens** — FEAT-A4. After **H-06**.
- [ ] **L-25 — Estimated current value (depreciation)** — FEAT-C11.

---

## Deduplication log

| Merged task | Combined source findings |
|---|---|
| H-01 | ARCH-A1 = BUGS-SEC1 (same root cause, reported twice) |
| H-05 | ARCH-A4 + BUGS-SEC5 + BUGS-SEC14 (rate limiter) |
| H-06 | ARCH-A5 + BUGS-SEC8 + FEAT-G1 (stale claims / token invalidation) |
| H-08 | ARCH-A3 + BUGS-BUG2 + BUGS-BUG14 (backup/restore) |
| H-09 | ARCH-H1 + model part of BUGS-DEAD1 |
| H-10 | BUGS-BUG3 + `DEFAULT 1` from ARCH-H4 |
| H-13 | BUGS-SEC12 + FEAT-F3 |
| H-14 | FEAT-F2 + ARCH-H2 + BUGS-BUG9 |
| H-15 | FEAT-F1 + search part of BUGS-BUG17 |
| M-05 | ARCH-A7 + BUGS-ERR3 + BUGS-SEC11 |
| M-10 | ARCH-P3 + BUGS-BUG5 + BUGS-BUG6 |
| M-12 | ARCH-P2 + BUGS-ERR2 |
| M-17 | BUGS-BUG16 + error boundary from BUGS-ERR4 |
| M-18 | ARCH-A12 + BUGS-BUG13 |
| M-19 | ARCH-S2 + BUGS-PERF1 + `uploads/` part of ARCH-A3 |
| M-21 | ARCH-A11 + BUGS-BUG11 |
| M-23 | ARCH-P4 + ARCH-D1 |
| M-24 | ARCH-A6 + ARCH-S1 |
| L-08 | ARCH-H5 + BUGS-DEAD1 + BUGS-DEAD2 |

## Decisions needed from you (they change implementation, not order)

1. **H-01:** should an *admin* be able to read/edit other users' inventories through the normal data endpoints? Current report/permission code does not grant admins that; the proposed change keeps it that way.
2. **H-09/H-13:** drop `password_reset_tokens` and create a fresh `invite_tokens` table (proposed), or repurpose it?
3. **M-23:** keep `f64` and remove `rust_decimal`, or migrate prices to `NUMERIC`?
4. **M-31:** which license is intended — GPL-3.0 (the `LICENSE` file) or MIT (`Cargo.toml`, `flake.nix`)?
