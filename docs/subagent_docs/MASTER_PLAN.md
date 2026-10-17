# MASTER PLAN — home-registry

Consolidated from [ANALYSIS_ARCH.md](ANALYSIS_ARCH.md), [ANALYSIS_BUGS.md](ANALYSIS_BUGS.md), and [ANALYSIS_FEATURES.md](ANALYSIS_FEATURES.md) (baseline `main` @ `89e1e94`). Duplicates are merged into one task. Source IDs are prefixed with the report they come from, because ARCH and FEATURES both use `A1`, `H1` and so on: **ARCH-A1**, **BUGS-SEC1**, **FEAT-F1**. Each points back to the full write-up (with file and line references) in that report.

## How we work through this list
1. Pick the first unchecked item (top to bottom; order inside a tier already reflects dependencies).
2. I show the proposed change (files, diff summary, tests, risks). **Nothing is applied until you approve.**
3. Apply → run the verification commands from `CLAUDE.md` → tick the box → move to the next.
4. Tier 3 items (security, schema/migration, public interface, many files) also get a `reviewer` pass before the commit message. You run all git commands.

**Progress:** 11 / 19 high · 0 / 48 medium · 0 / 31 low

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
- [x] **H-04 — Require the current password to regenerate recovery codes**
  Source: BUGS-SEC4. Files: `src/api/auth.rs:1923-1994`, frontend `RecoveryCodesSection`. A stolen JWT currently regenerates codes, resets the password, and locks the owner out.
- [x] **H-05 — Rate limiter fix + bound unauthenticated Argon2 work**
  Sources: ARCH-A4, BUGS-SEC5, BUGS-SEC14 (rate-limit part). Files: `src/main.rs:130-170`, `src/api/auth.rs:2174-2185`.
  Also cover password-verifying authenticated endpoints (`PUT /auth/password`, `POST /auth/recovery-codes/generate`): a stolen token can otherwise guess the password there.
  Build the limiter backend once (not per worker), correct `interval/max_requests` (currently ~5,000 req/s), guard `RATE_LIMIT_RPS=0`, key on real client IP behind a trusted proxy, and add per-account/per-IP attempt limits on login/register/recovery. Replace "verify against 10 hashes" for recovery codes with a single lookup.
  Done in H-05 except the single-lookup recovery codes, which moved to **M-47**.
- [x] **H-06 — Token versioning, auth context from the DB user, and "sign out everywhere"**
  Sources: ARCH-A5, BUGS-SEC8, FEAT-G1. Files: `src/auth/mod.rs:143-217, 280-288`, `src/api/auth.rs:63-102`, new migration (`users.token_version`).
  Build `AuthContext.is_admin` from the loaded row (demoted admins lose access immediately), invalidate tokens on password change/reset/recovery/deactivation, add the Settings button.
  Done: `users.token_version` (migration V024), `tv` claim, auth context from the stored user, `POST /auth/logout-all` ("Sign out all other devices"), fresh token after password change / TOTP enable / disable. The per-user version counter is standard practice, not a verified OWASP source (the OWASP JWT cheat sheet could not be retrieved). Plain logout remains client-side only; see **L-27**.
- [x] **H-07 — One source of truth for "who can access this inventory"**
  Source: ARCH-A2. Files: `src/db/mod.rs:1765-1869, 1919-1961, 2362-2384, 2397-2500`. Four copies of the owner/share/grant rule; `get_effective_permissions` falls back to `View` on a parse error (fails open). Consolidate in SQL (view/function) and have the Rust permission object derive from it.
  Done: view `inventory_access` (migration V025) is the single definition; `get_effective_permissions`, `get_accessible_inventories`, `check_inventory_access` and the four report queries read it; unknown share levels are not emitted by the view and `EffectivePermissions::from_access` also denies them. Plans checked on 300k seeded items: the user-id filter is pushed into every branch and uses indexes (0.13 ms / 0.16 ms / 1.9 ms), so no SQL function was needed. Intentional exceptions that do not decide access: share and grant CRUD by id, the ownership-transfer cleanup, backup/restore (admin, whole tables), and the owner-only check in `transfer_inventory_ownership` (compares `inventories.user_id` directly because it is owner-only by design). Any change to who may access an inventory belongs in the view; see **M-48**.

### Data integrity

- [x] **H-08 — Backup/restore correctness (2FA wipe, consistent snapshot, schema drift)**
  Sources: ARCH-A3, BUGS-BUG2, BUGS-BUG14. Files: `src/db/mod.rs:2694-2894`, `src/models/mod.rs:1026-1050`, `src/api/backup.rs`.
  Include `user_totp_settings`; derive the table list from the catalog; take the export in one `REPEATABLE READ` transaction; store the schema version in the backup metadata; fix the sequence-reset error path; make backup filenames collision-safe. (Backing up `uploads/` is tracked in **M-19**.)
  Done: tables come from the catalog (`src/db/backup.rs`), export is one `REPEATABLE READ` read-only snapshot, restore is one transaction in foreign-key order with catalog-derived sequence reset, format "2.0" records `schema_version` and a TOTP key fingerprint (1.0 backups still restore), a newer-schema backup is refused, unique collision-proof file names with `create_new`, strict name validation, restore ends all sessions (`token_version` raised past both the restored and the previous value). A TOTP key mismatch refuses by default; `restore_without_2fa` restores everything but 2FA (users without an enrolment row log in with their password; there is no server-wide mandatory-2FA rule). Self-referencing foreign keys need no ordering (one INSERT per table, checked at statement end); none exist today. `numeric` columns are exported as text so no digits are lost. Known limit: a JSON `null` stored in a `jsonb` column restores as SQL NULL (only `user_settings.settings_json`, which treats both alike). Backup files still contain password and recovery-code hashes and encrypted 2FA secrets (see **L-15**); `uploads/` is **M-19**; the existing `H-09` migration will drop six tables that older backups still contain, and restore skips them with a warning.
- [x] **H-09 — Drop the six orphaned tables and their dead models**
  Source: ARCH-H1 (plus the model/DEAD1 part of BUGS-DEAD1). Files: `migrations/V005-V007, V013`, `src/models/mod.rs:215-312`, `src/db/mod.rs:2805-2876`. New migration (next free number; V024 is used by H-06) drops `categories`, `tags`, `item_tags`, `custom_fields`, `item_custom_values`, `password_reset_tokens`; remove from export/import. **Note:** FEAT-F3 (invite links) needs a new token table; do not reuse the dropped one.
  Done (migration **V026**): also drops `items.category_id`, its FK and index. History check: no tagged release (v0.1.0-beta.1..3) and no commit ever contained DML on `categories`, `tags`, `item_tags`, `custom_fields`, `item_custom_values`, and no Rust code ever referenced `items.category_id` in SQL, so no backfill into `items.category` was added. `password_reset_tokens` was written only by a reset-token feature on the development branch from 2026-02-05 (2ebf242) to 2026-02-09 (09a9dfd), removed before beta.1. Safety net: before migrating, `startup::protect_legacy_data_before_migration` backs up (prefix `home_registry_pre_migration_v026`) when any of the objects holds data, logs counts and file name at WARN, and the server refuses to start if that backup fails. The SQL `NOTICE` does reach the application log (tokio-postgres logs notices at INFO through `log`), verified by the end-to-end startup test, but it needs `RUST_LOG` to allow INFO for that crate, so the WARN from Rust is the one to rely on. refinery runs each migration with its history row in one transaction. No view, function, trigger, policy or rule referenced the tables, and no table has a self-referencing foreign key, so the restore code needed no special case to remove. Restore of a pre-V026 backup warns only about skipped data, with counts. The frontend `Page` type entries `'categories' | 'tags'` are left for **L-08**.
- [x] **H-10 — Items must not default into inventory #1**
  Sources: BUGS-BUG3, ARCH-H4 (the `DEFAULT 1` part). Files: `src/db/mod.rs:225`, `src/models/mod.rs:60`, `migrations/V001-V003`. Make `inventory_id` required in `CreateItemRequest`, and drop the column default in a new migration.
  Done (migration **V027**, drops `items.inventory_id` default): `CreateItemRequest.inventory_id` is a required `i32` with `range(min = 1)`; `POST /items` validates before authorizing (0/negative are a clear 400, not a 404). Item-insert paths audited: the only Rust writer is `db::create_item` (called by the API and the test helpers); the V019 sample-data migration supplies `inventory_id` explicitly; restore inserts the backup's own `inventory_id` and now refuses, with the table, column, row number and id, a backup row that lacks a required column (no default is ever substituted). Legacy 1.0 backups always carry `inventory_id` on items (exported with `to_jsonb`, no skip/Option on the old struct; fixture test). No other `DEFAULT`/`unwrap_or` on an `inventory_id`/`user_id`-like column exists. The create form refuses to submit when the URL id is not a positive integer. The error envelope for a body with a missing field is the JSON extractor's 400, whose format is **H-11**'s concern. Changelog and README upgrade note added. `UpdateItemRequest.inventory_id` (move), the report filter and ARCH-H4/M-29 are unchanged. Not yet in `CHANGELOG.md`: the earlier items (H-01..H-09) — worth one docs pass before the next release.

### Core bugs

- [x] **H-11 — JSON error handler never runs (config overridden at `/api` scope)**
  Source: ARCH-P1. Files: `src/main.rs:192-208`, `src/api/mod.rs:1652-1653`. Configure `JsonConfig`/`PayloadConfig` once; malformed JSON should return the standard envelope, without echoing internals.
  Done: `src/api/extractors.rs` configures JSON, Query and Path extractors (and the 15/20 MiB limits) once, on the `/api` scope; `main.rs` no longer sets them. Every extractor failure returns the `ErrorResponse` envelope (the same type every handler uses; there is no separate AppError yet, see **M-05**) with the framework's own status (400, 413, 404 for a bad path id). Messages come from an allowlist of serde / `serde_json` / `serde_urlencoded` / actix path formats; anything else gets a fixed sentence (`The request body is not valid JSON`, `Invalid query parameters`, `Invalid path parameter`) and a `*_unrecognized` log category. Mapped: `missing field` and `duplicate field` (kept; the name is the struct's own); `unknown field` and `unknown variant` (the name the client sent is dropped, only the valid list is kept, so ``unknown variant, expected one of `view`, `edit_items`, `edit_inventory` ``); `invalid type` and `invalid value` (received value removed, struct names replaced by "an object"); `invalid length N`; `data did not match any variant` (type name dropped); the ten std integer/float/bool/char parse errors that query values produce (pinned to the toolchain's own wording by a test); and the path error `can not parse "x" to a i32` (segment removed; 404 kept). Position suffix stripped, capped at 200 chars. Logging: method, route pattern (`/api/inventories/{id}`, not the concrete path, so path values are never logged), category; never body, query or serde text. The access log prints the same route pattern (`%{ROUTE}xo`), no query string, no `Referer` and no `User-Agent` (the rule is that no client-chosen text is logged; the reverse proxy's log has those); a method outside the nine standard ones is logged as `OTHER` (access log, 404 line, rejection lines); the uploaded-backup parse error is logged through the same sanitizer (`loggable_json_error`), with a sentinel test; the scope is the HTTP-layer logs only, and application/audit logging is **L-31**; a request that matched no route is logged as `<unmatched>` (access log, `api_not_found` and `log_rejection`), never with its path, and the 404 message is a fixed sentence; the README says to use the reverse proxy's access log for probes. A well-formed body that does not match the expected shape in an unrecognised format says "The request body does not match the expected format"; "not valid JSON" is kept for syntax and truncation errors. The README warns against `actix_web=debug` in production (actix logs concrete paths for failed extractors at debug level; capping it in the default filter is not a one-liner with `env_logger`, which cannot take the lower of the configured level and `info`). The existing `/api` 404 fallback also stopped echoing/logging the query. Unknown routes and wrong methods already returned an envelope (404 "Endpoint not found"). The H-05 limiter wraps the scope so it runs before extractors (tested: a flood of bad bodies is limited and logs one line per admitted request). Because the access log now records route patterns, search terms in path parameters (`/items/search/{term}`) are no longer logged either. Follow-ups: **L-28**, **L-29**.
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
  Also revoke other sessions (`bump_token_version`, fresh token to the caller) when the mode is downgraded; H-06 only covers enable and disable.
- [ ] **M-03 — CSV formula injection** — BUGS-SEC9. `src/api/mod.rs:1231-1289`; prefix `= + - @ \t \r` cells.
- [ ] **M-04 — Account enumeration and login timing** — BUGS-SEC10. Dummy-hash verify for unknown users; check `is_active` after the password.
- [ ] **M-05 — Typed `AppError` (`ResponseError`), proper DB error classification, no internal details to clients** — ARCH-A7, BUGS-ERR3, BUGS-SEC11. Replaces 125 hand-built 500s and string-matching on `"duplicate"`/`"unique"`.
- [ ] **M-06 — Stop swallowing security-relevant write failures (`let _ =`)** — BUGS-ERR1. Counter increments/resets, `mark_recovery_code_used`, `create_user_settings`, `try_exists().unwrap_or(false)`.
- [ ] **M-07 — Cookie-auth fallback and localhost CORS: finish or remove** — ARCH-H3. `src/auth/mod.rs:219-237`, `src/main.rs:170-188`.
- [ ] **M-08 — Reduce XSS blast radius: CSP without `unsafe-inline/unsafe-eval`, token handling** — BUGS-SEC13 (self-hosting fonts/icons from ARCH-D6 is the enabler).
- [ ] **M-47 — Single-lookup recovery code verification** — split out of H-05. `POST /auth/recovery-codes/use` still verifies against all of a user's (up to 10) Argon2 hashes per attempt (H-05 pads to exactly 10 so timing is constant, and the throttle plus Argon2 concurrency cap bound the cost). A single lookup needs a new indexed lookup column (e.g. an HMAC of the code). Existing codes are stored only as Argon2 hashes and cannot be backfilled, so this needs the column plus a fallback scan for old rows, or forced regeneration. `src/api/auth.rs`, `src/db/mod.rs`, new migration.
- [ ] **M-48 — Decide how inventories of deactivated users are treated** — Today sharees and All Access grantees keep access to inventories owned (or granted) by a deactivated user, because the access rule never looks at the owner's or grantor's `is_active`. Options: keep access (current), read-only, or no access. The `inventory_access` view (V025) is now the single place to implement the choice (join `users` in the owner and all_access branches, or filter by `is_active`). A deactivated *grantee* is already refused at authentication.

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
- [ ] **L-26 — Make the `tv` claim required (remove the compatibility shim)** — `Claims.tv` defaults to 0 so tokens issued before migration V024 keep working. Once every such token has expired (`JWT_TOKEN_LIFETIME_HOURS`, 24 h by default), remove `#[serde(default)]` and the `TODO(L-26)` in `src/models/mod.rs`. Do it in the first release cut at least two days after the release that ships V024, and drop the legacy-token test in `tests/test_sessions.rs`.
- [ ] **L-27 — Per-token revocation for ordinary logout** — Plain logout only clears local storage; the token stays valid until expiry (the OWASP Session Management cheat sheet expects server-side invalidation). Add a `jti` claim and a denylist of revoked `jti` values with expiry pruning (in memory, or a small table so it survives restarts and multiple instances), checked in `get_auth_context_from_request`; call it from logout. Until then, never describe logout as revoking the token; point to "Sign out all other devices" or a password change.
- [ ] **L-28 — Multipart upload errors use the error envelope** — `MultipartForm` failures (image upload, backup upload: size limit, bad form, missing field) still use actix's default plain-text responses. Add a `MultipartFormConfig` error handler on the `/api` scope with the same redaction rules as `src/api/extractors.rs`. Split out of H-11. Also: the upload endpoint answers an unsupported backup version with `Unsupported backup version: {version}` in the response, echoing the client's version string; give it a fixed message.
- [ ] **L-29 — Report which JSON field failed** — include the field path (e.g. `items[0].quantity`) in the error envelope. Needs `serde_path_to_error` (a new pinned dependency, checked against `deny.toml`) and a custom `Json` extractor replacing `web::Json` in ~30 handlers, or a wrapper that re-parses the buffered body on failure. Split out of H-11.
- [ ] **L-30 — Three moderate `npm audit` findings (React Router open redirect)** — `@remix-run/router` 1.3.0–1.23.2 (GHSA-2j2x-hqr9-3h42: a same-origin redirect whose path starts with `//` is reinterpreted as protocol-relative, an open redirect), pulled in through `react-router` and `react-router-dom` (pinned `6.30.3`, listed under `dependencies`). All three are production dependencies and ship in the bundle. Exposure here looks low: every `navigate()` / `<Navigate>` target is a fixed path or `/inventory/${numeric id}`, and nothing reads a redirect target from the URL, a query parameter or an API response (checked by grep, not by test). The fix is `react-router-dom@6.30.6`, an exact-pin bump (Tier 3: new pinned version, changelog and `package-lock.json`). Found during H-11 verification; not fixed there.
- [ ] **L-31 — Audit and application logging of names, filenames and usernames** — about 32 `info!`/`warn!`/`error!` calls under `src/api` and `src/auth` write client-chosen text: inventory, item and organizer names (`src/api/mod.rs` ~236, 554, 809, 1062), image filenames (~1585, 1611, 1619), backup filenames (`src/api/backup.rs` ~589, 868), and `auth.username` (`src/api/mod.rs` ~1901, 1954, 2009, 2072; `src/api/backup.rs` ~526; `src/api/totp.rs` ~101). Decide what audit lines may contain (for example the acting user's id plus username, and ids instead of names and filenames for objects), and redact any error text that wraps client input (H-11 fixed the uploaded-backup parse error only). Found by the H-11 review; H-11's rule covers the HTTP-layer logs only.

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
