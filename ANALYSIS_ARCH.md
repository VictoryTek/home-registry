# Architecture & Structure Analysis — home-registry

**Scope:** architecture and structure only (design, module boundaries, consistency, abandoned code, dependencies). This is not a full security or performance audit. Several structural problems do have security consequences, and those are called out where they apply.
**Baseline:** `main` @ `89e1e94` (2026-10-13). Line numbers refer to that commit.

**Priority key**
- **High**: causes wrong behaviour, data loss, or an authorization gap now, or blocks safe evolution of the codebase.
- **Medium**: a real maintenance or correctness risk that is not currently causing visible breakage.
- **Low**: hygiene, drift, or clutter.

---

## Summary table

| ID | Priority | Category | Title |
|----|----------|----------|-------|
| A1 | High | Anti-pattern | Authorization is opt-in per handler, and most data handlers never opted in |
| A2 | High | Anti-pattern | Three independent implementations of "can user X access inventory Y" |
| A3 | High | Anti-pattern | Backup/restore is a hand-maintained table list that has already drifted (2FA wiped on restore, images not backed up) |
| A4 | High | Anti-pattern | Rate limiter is misconfigured, is per-worker, and is keyed on the proxy IP |
| A5 | High | Inconsistency | `is_admin` comes from JWT claims even though the user row is loaded on every request |
| A6 | Medium | Anti-pattern | `src/api/mod.rs` and `src/db/mod.rs` are monoliths with no layering between HTTP and SQL |
| A7 | Medium | Anti-pattern | `Box<dyn Error>` everywhere, with no `ResponseError`; 125 hand-built 500 responses |
| A8 | Medium | Anti-pattern | Runtime filesystem layout is hardcoded and relative to the working directory |
| A9 | Medium | Anti-pattern | Secrets bootstrap: the documented precedence is wrong, and the TOTP key is coupled to the JWT secret |
| A10 | Medium | Anti-pattern | Hand-rolled `DATABASE_URL` parser, duplicated in tests |
| A11 | Medium | Anti-pattern | Frontend `AppContext` mixes UI state with a global data cache that pages overwrite inconsistently |
| A12 | Medium | Anti-pattern | Service worker caches authenticated `/api/*` responses for 24 hours |
| A13 | Medium | Anti-pattern | Client retries non-idempotent requests on network errors |
| S1 | Medium | Structure | `src/api/auth.rs` and `authApi` are a grab-bag (sharing, grants, admin, ownership transfer) |
| S2 | Medium | Structure | Two image-storage strategies (base64 in a DB column vs files on disk) |
| S3 | Medium | Structure | Backend/frontend contract is duplicated by hand, and nullability already disagrees |
| S4 | Low | Structure | Route definitions duplicated in `main.rs`, `App.tsx`, and test harnesses |
| S5 | Low | Structure | Naming inconsistencies in handler parameters, type names, and module names |
| S6 | Low | Structure | Tracked generated or local files (`.env`, `package-lock.json.backup`, `tsconfig.tsbuildinfo`) |
| S7 | Low | Structure | Lint configuration declared in three places |
| P1 | High | Pattern | API-level `JsonConfig` silently overrides the app-level one, so the custom JSON error handler never runs |
| P2 | Medium | Pattern | Multi-statement writes are inconsistently transactional; errors inside a transaction are "ignored" |
| P3 | Medium | Pattern | Validation is done three different ways, and invalid dates are silently stored as NULL |
| P4 | Medium | Pattern | Mixed money representation (`DECIMAL` → `DOUBLE PRECISION` → `f64`) |
| P5 | Low | Pattern | Auth/token storage keys duplicated, and some pages bypass `AuthContext` |
| P6 | Low | Pattern | Mixed `fetch` vs `fetchWithRetry`, and duplicate health endpoints |
| P7 | Low | Pattern | Preflight scripts (`.sh` and `.ps1`) have diverged from each other and from CI |
| H1 | High | Half-implemented | Six orphaned tables (categories, tags, custom fields, password resets) still in the schema and backups |
| H2 | Medium | Half-implemented | Legacy `items.category`/`items.location` columns coexist with Organizers |
| H3 | Medium | Half-implemented | Cookie-based auth fallback that nothing sets, plus localhost-only CORS |
| H4 | Medium | Half-implemented | Sample-data migrations V019–V021 and stale references to them |
| H5 | Low | Half-implemented | Dead code: unused models, DB methods, handlers, and frontend exports |
| H6 | Low | Half-implemented | `tests/integration_test.rs` tests a stub, not the application |
| H7 | Low | Half-implemented | Routes for logo files that don't exist |
| D1 | Medium | Dependency | `rust_decimal` is a pinned dependency with zero uses |
| D2 | Medium | Dependency | Pinning policy is violated (`refinery`, `time`, npm carets, floating Docker/Action tags) |
| D3 | Medium | Dependency | License metadata contradicts itself (MIT vs GPL-3.0) |
| D4 | Medium | Dependency | The frontend's GitHub version check is blocked by the backend's own CSP |
| D5 | Low | Dependency | Overlapping or underused crates (`validator`, two RNG paths, `tokio` "full") |
| D6 | Low | Dependency | Frontend runtime depends on third-party CDNs, which the CSP has to accommodate |

---

## 1. Architectural anti-patterns and design problems

### A1 — Authorization is opt-in per handler, and most data handlers never opted in · **High**
**Files:** `src/api/mod.rs:140-950` (inventory/item/organizer handlers), `src/api/mod.rs:1115-1215`, `src/api/auth.rs:31-103`, `src/api/auth.rs:1344-1392`

Authentication works by calling `auth::get_auth_context_from_request` at the top of each handler. No middleware or extractor enforces it, so each handler has to remember to call it. In practice:

- **No authentication at all:** `get_inventory` (`mod.rs:140`), `get_inventory_items` (`:171`), `update_inventory` (`:207`), `delete_inventory` (`:263`), `get_items` (`:298`, which returns **every item in the database** via `get_all_items`), `get_item` (`:323`), `create_item` (`:354`), `update_item` (`:391`), `delete_item` (`:435`), `search_items` (`:466`), and every organizer type/option/value handler (`:504-950`).
- **Authenticated but not authorized** (the result is bound to `_auth` and never used): `upload_image` (`:1023`), `delete_image` (`:1122`, so any user can delete any file in `uploads/img`), `get_inventory_item_images` (`:1178`, whose comment says "consistent with other inventory endpoints" when it is not), and `update_inventory_share` (`auth.rs:1351`, so any user can change any share's permission level).
- **Correctly authorized:** only `get_inventories`, `create_inventory`, the sharing/grant handlers, and the report handlers.

The permission model (`EffectivePermissions`, `PermissionLevel::can_edit_items` and so on in `src/models/mod.rs:546-610`, `src/db/mod.rs:1765`) is well designed, but the data endpoints never consult it. The root cause is structural: the model lets handlers skip the check silently. **Recommendation:** use a `FromRequest` extractor (`AuthContext` as a handler argument) so a missing check becomes a compile-visible signature difference. Also add a service-layer method such as `require_inventory_permission(user, inventory_id, Perm)` that every inventory-scoped handler has to call.

### A2 — Three independent implementations of "can user X access inventory Y" · **High**
**Files:** `src/db/mod.rs:1765-1869` (`get_effective_permissions`, procedural, 3 queries), `src/db/mod.rs:2362-2384` (`check_inventory_access`, single SQL `EXISTS`-style query), `src/db/mod.rs:1919-1961` (`get_accessible_inventories`, its own SQL), plus the report query at `src/db/mod.rs:2397-2500`, which embeds the rule a fourth time as a `WHERE` clause.

All of these encode the same rule (owner OR share OR all-access grant), but they are separate code paths. A future change, such as revoking access for deactivated grantors or adding an expiry to grants, has to be made in 3–4 places, and the paths will disagree in between. `get_effective_permissions` also falls back to `PermissionLevel::View` if parsing fails (`:1837`; also `:1701`, `:1736`), which fails open instead of closed. **Recommendation:** use one SQL view or function (for example `inventory_access(user_id)`) as the single source of truth, and have the Rust-side permission object derive from it.

### A3 — Backup/restore is a hand-maintained table list that has already drifted · **High**
**Files:** `src/db/mod.rs:2694-2790` (export), `src/db/mod.rs:2792-2894` (import), `src/models/mod.rs:1026-1050` (`BackupDatabaseContent`), `migrations/V023__create_totp_settings_table.sql:4-6`, `src/api/backup.rs:76-124`

- The export and import lists, and the struct fields, enumerate tables by hand. **`user_totp_settings` (added in V023) is in none of them.** Restore runs `TRUNCATE users ... CASCADE` (`:2805-2826`), and `user_totp_settings.user_id` is `ON DELETE CASCADE`, so **every restore silently deletes all users' 2FA enrolment**. Every new table repeats this risk.
- Images are stored on disk in `uploads/img` (see S2), but backups contain only DB rows (`backup.rs:76-124`). Restoring on a fresh host gives item records that point at missing files.
- The sequence-reset loop "ignores errors" (`:2879-2885`). In PostgreSQL, any error inside a transaction aborts it, so a single failed `setval` makes the final `commit()` fail. The comment describes behaviour the code cannot have.
- `jsonb_populate_record(NULL::{table}, $1)` with `INSERT ... SELECT *` depends on column order matching between the backup's source schema and the current schema. Combined with the hardcoded `"1.0"` version check (`backup.rs:519`), which does not encode schema version, a backup taken before a column-adding migration can fail or mis-map on restore.

**Recommendation:** derive the table list from the catalog (`information_schema`, excluding `refinery_schema_history`), store the refinery migration version in `BackupMetadata`, and include `uploads/` in the backup artifact (for example tar or zip).

### A4 — Rate limiter is misconfigured, is per-worker, and is keyed on the proxy IP · **High**
**File:** `src/main.rs:130-170`

- `SimpleInput { interval: 1000/rps ms, max_requests: burst }` (`:160-164`) means "`burst` requests per `1000/rps` ms window". With the defaults (rps=50, burst=100) that is 100 requests per 20 ms, or **~5,000 req/s per client**, not the 50 req/s the comments and README describe. `RATE_LIMIT_RPS=0` causes a divide-by-zero panic inside the worker factory.
- `InMemoryBackend::builder().build()` runs inside the `HttpServer::new` closure (`:149`), so each worker thread gets its own counter. The effective limit is multiplied by the worker count (CPU cores).
- The key is `req.peer_addr()` (`:157-159`). Every documented deployment (`docs/deployment/reverse-proxy-*.md`) puts Caddy, nginx, or Traefik in front, so all users share the proxy's IP and therefore one bucket. That inverts the intent: it does little against one attacker and throttles everyone together.

Combined with the frontend's 5× retry on 429 (A13), this design amplifies load instead of shedding it. **Recommendation:** build the backend once outside the closure (it is `Clone`), compute `interval = 1s, max_requests = rps` (with burst handled by the algorithm, or drop the burst setting), and key on `realip_remote_addr()` with a trusted-proxy setting.

### A5 — `is_admin` comes from JWT claims even though the user row is loaded on every request · **High**
**Files:** `src/api/auth.rs:63-102`, `src/auth/mod.rs:280-288`, `src/auth/mod.rs:156-167`

`get_auth_context_from_request` already fetches the user from the DB on every request (to check `is_active`), but then discards it and builds `AuthContext` from the token claims (`AuthContext::from_claims`). A demoted admin keeps admin rights until the token expires (24 h by default, configurable without an upper bound via `JWT_TOKEN_LIFETIME_HOURS`). The design already pays the DB round-trip cost, so the stale claim brings no benefit. **Recommendation:** build `AuthContext` from the loaded `User`, and keep only `sub` (plus `totp_pending`) in the token.

### A6 — `src/api/mod.rs` and `src/db/mod.rs` are monoliths with no layering between HTTP and SQL · **Medium**
**Files:** `src/api/mod.rs` (1,744 lines, ~45 handlers plus image I/O, CSV formatting, and route table), `src/db/mod.rs` (3,107 lines, ~90 methods on one `DatabaseService`), `src/models/mod.rs` (1,050 lines)

- There is no service or domain layer. Handlers call `DatabaseService` directly, and business rules live in whichever side the author happened to put them in. For example, "can't demote the last admin" is in the handler (`auth.rs:950-990`), "share can't grant delete or manage-sharing" is in the DB layer (`db/mod.rs:1846-1847`), and CSV formatting is in the HTTP module (`api/mod.rs:1231-1290`).
- `DatabaseService::new(pool.get_ref().clone())` is constructed **101 times**, once per handler. It is a zero-state wrapper around `Pool`, so it should be registered once as `web::Data<DatabaseService>`.
- `Item` row mapping (13 positional `row.get(n)` calls) is copy-pasted **7 times** (`db/mod.rs:139, 176, 239, 355, 412, 603, 2487`), and the matching `SELECT` column list is copy-pasted 6+ times. Adding one column to `items` means about 14 coordinated edits with no compiler help, because positional indexes are not type-checked against the query.

**Recommendation:** split by domain (`api/{inventories,items,organizers,images,reports,sharing,admin}.rs`, with matching `db/` modules), add `impl TryFrom<&Row> for Item`, and add a const column list.

### A7 — `Box<dyn Error>` everywhere, with no `ResponseError`; 125 hand-built 500 responses · **Medium**
**Files:** every method in `src/db/mod.rs` (`Result<_, Box<dyn std::error::Error>>`), `src/api/*.rs`

- DB errors are type-erased, so handlers cannot tell a unique-violation (409), an FK violation (400/404), and a connection failure (503) apart. Everything becomes a 500.
- `Box<dyn Error>` is not `Send`, which is why `get_pool` alone uses `Box<dyn Error + Send + Sync>` (`db/mod.rs:51`). That is an inconsistent error type within the same module.
- `HttpResponse::InternalServerError().json(ErrorResponse {...})` is written out by hand **125 times** (`auth.rs`: 60, `mod.rs`: 38, `totp.rs`: 16, `backup.rs`: 11). Only 66 use the sanitized `"An internal error occurred"` message. Others leak detail, for example `format!("Invalid token: {e}")` at `auth.rs:48` and `format!("Failed to read upload data: {e}")` at `mod.rs:1036`.
- Handlers return `Result<impl Responder>` but never use the `Err` path (`Err(e) => return Ok(e)` throughout), so the `?` operator and actix's error pipeline go unused.

**Recommendation:** add a single `AppError` enum (`thiserror` is already a transitive dependency) implementing `ResponseError`, return `Result<HttpResponse, AppError>` from handlers, and use `?`.

### A8 — Runtime filesystem layout is hardcoded and relative to the working directory · **Medium**
**Files:** `src/main.rs:40-41, 59, 238-360` (`"static/..."`, `"uploads/img"`), `src/api/mod.rs:32, 1061, 1127` (`"uploads/img"`, `"static/index.html"`), `src/api/backup.rs:18` (`BACKUPS_DIR = "backups"`), `src/auth/mod.rs:54-55, 88, 97` (absolute `/app/data`, `/run/secrets`), `src/auth/totp.rs:61`, `nix/module.nix:18-19, 218`

Paths come in three styles: relative to the working directory (`static/`, `uploads/`, `backups/`), absolute container paths (`/app/data/jwt_secret`), and Docker-secret paths (`/run/secrets/*`). None of them can be configured. The Nix module has to emulate the Docker layout by `mkdir`-ing directories and symlinking `static` into `WorkingDirectory` (`nix/module.nix:18-19`). On Nix or bare metal, the JWT auto-persist path `/app/data` is not writable, so it only works because the module also sets `JWT_SECRET_FILE`. Running the binary from any other directory breaks static serving silently, because the SPA fallback then returns 404s. **Recommendation:** add a single `DATA_DIR` / `STATIC_DIR` config read once at startup and passed via `web::Data<AppConfig>`.

### A9 — Secrets bootstrap: the documented precedence is wrong, and the TOTP key is coupled to the JWT secret · **Medium**
**Files:** `src/auth/mod.rs:26-114`, `src/auth/totp.rs:52-85`

- The doc comment (`mod.rs:27-32`) lists precedence as Docker secret → `JWT_SECRET_FILE` → `JWT_SECRET` env → persisted file. `read_jwt_secret` actually does `JWT_SECRET_FILE` → Docker secret → **persisted file → env** (`:75-113`). Consequence: once a container has started without `JWT_SECRET` and auto-generated one into the `appdata` volume, a later `JWT_SECRET` setting is **silently ignored forever**, even though `docker-compose.yml:61` presents the env var as the way to set it.
- The TOTP encryption key falls back to an HKDF of the JWT secret (`totp.rs:77-83`). Rotating the JWT secret (a routine operation that only logs users out) therefore makes **every stored TOTP secret undecryptable**, which locks out 2FA users. Two concerns with very different rotation semantics share one secret.
- `jwt_secret()` (`mod.rs:130`) allocates a new `String` on every sign/verify call, wrapping a `&'static str`.

### A10 — Hand-rolled `DATABASE_URL` parser, duplicated in tests · **Medium**
**Files:** `src/db/mod.rs:54-87`, `tests/common/mod.rs:10-45`

The URL is split on `@`, `:`, and `/` by hand. It breaks for passwords containing `@`, `:`, or `/` (common in generated secrets), rejects the `postgresql://` scheme that libpq and most tooling emit, drops query parameters such as `sslmode`, and **silently defaults the password to `"password"`** when it is missing (`:66`). That contradicts commit `4c9dfc4` ("drop dev password default"). TLS is hardcoded off (`NoTls`, `:113`), which rules out managed or remote Postgres, a setup `docs/deployment/database-production.md` discusses. `tokio_postgres::Config: FromStr` (or deadpool's `Config { url }`) handles all of this. The test harness has a second, slightly different copy of the parser.

### A11 — Frontend `AppContext` mixes UI state with a global data cache that pages overwrite inconsistently · **Medium**
**Files:** `frontend/src/context/AppContext.tsx:6-23, 80-91`, `frontend/src/pages/InventoriesPage.tsx:44-89`, `frontend/src/pages/InventoryDetailPage.tsx:33, 123`

`AppContext` holds theme, toasts, sidebar state, **and** domain data (`inventories`, `items`, `warrantyNotifications`). Warranty notifications are derived from whichever `items` array was written last:
- `InventoriesPage` fetches every inventory's items in an N+1 fan-out (`:56-60`, one request per inventory just to show counts) and writes **all** items.
- `InventoryDetailPage` overwrites the same global with **one** inventory's items (`:123`).

The notification badge and `NotificationsPage` therefore show different results depending on the last page visited. Because any context change re-renders every consumer, the code also has four "CRITICAL FIX" comments and `eslint-disable react-hooks/exhaustive-deps` suppressions (`AppContext.tsx:53-65`, `InventoriesPage.tsx:38-43, 87`, `InventoryDetailPage.tsx:102`) working around infinite render loops. **Recommendation:** compute warranty notifications server-side (or from a dedicated endpoint), keep UI state and server data in separate contexts, and add an items-count field to the inventories response.

### A12 — Service worker caches authenticated `/api/*` responses for 24 hours · **Medium**
**File:** `frontend/vite.config.ts:70-82`

A `NetworkFirst` rule caches every `/api/` GET (up to 100 entries, 1 day). These responses are user-specific and authenticated by a bearer header, which Workbox does not use as a cache key. After logout, or when someone else signs in on the same device, offline or failed-network requests are served the **previous user's** inventories, items, and admin user lists. Logout (`AuthContext.tsx:193-202`) does not clear `api-cache`. At the architecture level, offline support was bolted on without an ownership model for cached data.

### A13 — Client retries non-idempotent requests on network errors · **Medium**
**File:** `frontend/src/services/api.ts:198-262`

`fetchWithRetry` retries **any** method up to 5 times with exponential backoff (up to ~31 s) on network exceptions. A `POST /items` whose response is lost (timeout, proxy reset) is replayed and creates duplicates. The same happens for `POST /inventories/{id}/transfer-ownership` and `POST /auth/recovery-codes/generate`, where a replay invalidates the codes the user just saw. Retries should be limited to idempotent methods, or to 429/503 responses where the server confirms it did not process the request.

---

## 2. Structural inconsistencies

### S1 — `src/api/auth.rs` and `authApi` are a grab-bag · **Medium**
**Files:** `src/api/auth.rs` (2,252 lines), `src/api/mod.rs:1655-1685`, `frontend/src/services/api.ts:517-917`

`api/auth.rs` contains login and registration, but also **inventory sharing** (`:1138-1432`), **all-access grants** (`:1471-1711`), **inventory permissions** (`:1713`), **ownership transfer** (`:1752-1920`), **admin user CRUD** (`:770-1136`), and **user settings** (`:692-768`). Some of these routes live under `/inventories/...` and `/shares/...` but are defined in the auth module, while the rest of `/inventories/...` is in `api/mod.rs`. The frontend copies this layout: `authApi` holds sharing, admin, grants, recovery codes, and TOTP. The `src/auth/` (domain) vs `src/api/auth.rs` (HTTP) split is also undermined because `get_auth_context_from_request` and `require_admin` (core auth plumbing) live in the HTTP module and are imported back by `api/totp.rs:22` and `api/backup.rs`.

### S2 — Two image-storage strategies · **Medium**
**Files:** `frontend/src/pages/InventoriesPage.tsx:243-263` + `frontend/src/utils/imageCompression.ts:11` (inventory images → base64 data URI → `inventories.image_url TEXT`), `frontend/src/components/ImageOrganizerInput.tsx:42` + `src/api/mod.rs:1016-1113` (item images → multipart → `uploads/img/*.ext` → URL in `item_organizer_values.text_value`), `migrations/V009`, `migrations/V022`

Inventory cover images are stored inline in Postgres as multi-megabyte data URIs. That is why the JSON body limit had to be raised to 15 MiB (`main.rs:192-197`, `api/mod.rs:1652`) and why `validate_image_url` exists (`api/mod.rs:20-27`). Item images go through a proper upload endpoint with magic-byte validation and a 5 MB cap. The two paths differ in validation, size limits, backup coverage (A3), and deletion semantics: deleting an inventory removes its inline image, but deleting an item leaves its file orphaned in `uploads/img`, because nothing links files to rows.

### S3 — Backend/frontend contract is duplicated by hand, and nullability already disagrees · **Medium**
**Files:** `src/models/mod.rs` (1,050 lines), `frontend/src/types/index.ts` (538 lines)

Every DTO is written twice. Drift is already visible:
- Rust `Option<T>` serializes as `null`, but TS declares `field?: T` (that is, `undefined`), for example `Item` (`types/index.ts:27-41` vs `models/mod.rs:18-32`). Code that checks `=== undefined` misbehaves.
- The same concept has different names: `CategoryBreakdown` (Rust, `models/mod.rs:174`) vs `CategorySummary` (TS, `types/index.ts:443`).
- TS still declares `Category` (`types/index.ts:43`), `CreateUserRequest` (`:300`), and others for endpoints that do not exist (see H1).

**Recommendation:** generate the TS types (`ts-rs` or `typeshare`) or publish an OpenAPI schema.

### S4 — Route definitions duplicated in `main.rs`, `App.tsx`, and test harnesses · **Low**
**Files:** `src/main.rs:254-360`, `frontend/src/App.tsx:55-160`, `tests/test_api_integration.rs:29-34, 88-95, 179+`

- `main.rs` has ~11 nearly identical closures for individual static files (`/logo_*.png`, `/favicon.*`, `/manifest.*`, `/sw.js`), and `/manifest.json` and `/manifest.webmanifest` are two copies of the same handler (`:322-330`, `:350-358`). `spa_fallback` (`:40`) and the `/` route (`:254-262`) both serve `index.html` with different cache headers.
- `App.tsx` hardcodes the list of auth paths (`:101-106`) and a separate path→page mapping (`:55-88`) on top of the `<Routes>` table, so a new route has to be registered in three places. The sidebar and layout render outside `ProtectedRoute`.
- Integration tests build their own `web::scope("/api").service(...)` subsets instead of using `api::api_scope()`, so they cannot catch scope-level problems such as P1 or route-ordering conflicts.

### S5 — Naming inconsistencies · **Low**
- The handler parameter `req` means `HttpRequest` in some handlers (`api/mod.rs:49`) and `web::Json<...>` in others (`api/mod.rs:89, 211, 357`), where the `HttpRequest` is then named `http_req` (`:88`). `auth.rs` uses `body` for JSON (`:1349`). Parameter order (`pool, req` vs `req, pool`) also varies (`mod.rs:49` vs `mod.rs:1018`).
- `*_endpoint` suffixes appear only on two report handlers (`get_inventory_statistics_endpoint`, `get_category_breakdown_endpoint`, `mod.rs:1511, 1574`).
- `api_scope()` and its alias `init_routes()` "for backward compatibility" (`mod.rs:1740-1744`) in a crate with one internal caller.
- `create_token` is described as an "Alias for `generate_token` to match common naming convention" (`auth/mod.rs:342-357`) but has a different signature and fabricates a `User`.
- Table `user_totp_settings` vs Rust `TotpSettings` vs DB methods `*_totp_settings`. Organizer "types" are called `organizers` in URLs (`/organizers/{id}`) but `organizer_types` in the DB.
- `scripts/DEV_STANDARDS.md` is documentation inside `scripts/`. Process docs are spread across `.github/docs/`, `docs/`, `release_notes/`, `scripts/`, `CHANGELOG.md`, `CLAUDE.md`, and a 535-line `.github/copilot-instructions.md` that defines a conflicting agent workflow.

### S6 — Tracked generated or local files · **Low**
**Files:** `.env` (tracked despite `.gitignore:17`, and contains `RUST_LOG=debug` and a `DATABASE_URL` with a password), `frontend/package-lock.json.backup` (7,789 lines), `frontend/tsconfig.tsbuildinfo` (build cache), `sbom.json` (generated artifact, 1 line). `.cargo/config.toml:28` hardcodes a `TEST_DATABASE_URL` whose password (`uK8m3...`) matches neither CI (`ci.yml:42`, `password`), `scripts/setup-test-db.sh:13`, nor `tests/common/mod.rs:13`. It only works in CI because the service sets `POSTGRES_HOST_AUTH_METHOD: trust` (`ci.yml:44`), which masks the mismatch.

### S7 — Lint configuration declared in three places · **Low**
**Files:** `Cargo.toml:10-22` (`[lints]`, `unsafe_code = "forbid"`), `src/lib.rs:5-10` and `src/main.rs:5-7` (`#![deny(unsafe_code)]`, `#![warn(clippy::pedantic)]`, `allow` lists). The crate attributes are redundant with `[lints]`. `main.rs` repeats only part of the allow list, and `deny` is weaker than the manifest's `forbid`, which reads as if it were meant to override it. `clippy.toml:26` sets `min-ident-chars-threshold`, which does nothing unless the `restriction` lint `min_ident_chars` is enabled (it is not).

---

## 3. Inconsistent patterns

### P1 — API-level `JsonConfig` silently overrides the app-level one, so the custom JSON error handler never runs · **High**
**Files:** `src/main.rs:192-208`, `src/api/mod.rs:1652-1653`

`main.rs` registers a `JsonConfig` with a 15 MiB limit **and** an `error_handler` that returns the standard `{success, error, message}` envelope. `api_scope()` registers its own `JsonConfig::default().limit(15_728_640)` with **no** error handler. actix resolves `app_data` from the innermost scope outward, and every JSON endpoint is inside `/api`, so the `main.rs` handler is dead code. Malformed JSON gets actix's default plain-text 400 instead. The frontend's `handleResponse` (`api.ts:100-110`) then reports it as "Expected JSON but received text/plain". `PayloadConfig` is also set in both places. Configure it once.

### P2 — Multi-statement writes are inconsistently transactional · **Medium**
**Files:** `src/db/mod.rs:2141-2220` (`transfer_inventory_ownership`, transactional), `src/db/mod.rs:2792` (`import_all_data`, transactional), vs `src/db/mod.rs:1099-1110` (`set_item_organizer_values`, a loop of independent auto-commit upserts), `src/db/mod.rs:2221-2255` (`store_recovery_codes`), `src/api/auth.rs:150-260` (`initial_setup`: create user + settings + recovery codes as separate calls), `src/api/totp.rs:380-520` (recovery: password reset + TOTP state)

A failure midway leaves partial state: some organizer values saved, a user without settings, or a password changed without the TOTP counter reset. There is no shared transaction helper, so whether a write is atomic depends on whether that author remembered `client.transaction()`.

### P3 — Validation is done three different ways · **Medium**
**Files:** `src/models/mod.rs:34-115, 132` (`validator` derive on 5 structs), `src/auth/mod.rs:292-319` (hand-written `validate_password` / `validate_username`), `src/api/mod.rs:1305-1365` (inline date and price validation in the report handler), `src/db/mod.rs:206-214, 312-335` (date parsing in the DB layer)

- Only 5 of ~40 request structs derive `Validate`. Organizer, share, grant, admin-user, and TOTP requests have none, and `create_organizer_type` (`api/mod.rs:544`) does not even call `.validate()`.
- Invalid dates are **silently stored as NULL**: `NaiveDate::parse_from_str(...).ok()` in `create_item` and `update_item` (`db/mod.rs:208, 214, 319, 332`) turns `"2024-13-45"` into "clear the field" without an error. The report handler, by contrast, rejects bad dates with a 400.
- `UpdateItemRequest` uses `Option<T>` to mean "don't change", so non-date fields can never be cleared. Dates use an empty-string convention to clear. These are two incompatible partial-update semantics in one struct.

### P4 — Mixed money representation · **Medium**
**Files:** `migrations/V001__create_items_table.sql:9` (`DECIMAL(10,2)`), `migrations/V004__fix_price_column_type.sql:2` (changed to `DOUBLE PRECISION` "for better Rust compatibility"), `src/models/mod.rs:26` (`f64`), `src/db/mod.rs` (`::float8` casts throughout, `:131, 2537-2651`), `Cargo.toml:40` (`rust_decimal` with `tokio-pg`, unused, see D1)

The schema was downgraded from exact decimal to float to avoid wiring up `rust_decimal`, which was then added as a dependency anyway and never used. Report totals (`SUM(purchase_price::float8 * quantity)`) accumulate binary floating-point error. The `::float8` casts on a column that is already `float8` are leftovers from the `DECIMAL` era.

### P5 — Auth/token storage keys duplicated, and some pages bypass `AuthContext` · **Low**
**Files:** `frontend/src/services/api.ts:64, 90-91`, `frontend/src/context/AuthContext.tsx:12-13, 355-366`, `frontend/src/pages/RegisterPage.tsx:79-80`, `frontend/src/pages/SetupPage.tsx:93-94`

`'home_registry_token'` is defined twice and `'home_registry_user'` appears as a literal in four files. `RegisterPage` and `SetupPage` write the token directly to `localStorage`, so `AuthContext`'s `user`/`token` state is not updated until a reload. `api.ts` clears storage on 401 behind the context's back and hard-navigates with `window.location.href` instead of going through the router or context. `getAuthToken` and `getAuthHeaders` in `AuthContext.tsx` duplicate `api.ts`'s `getToken`/`getHeaders` and have no callers.

### P6 — Mixed `fetch` vs `fetchWithRetry`, and duplicate health endpoints · **Low**
**Files:** `frontend/src/services/api.ts:1015, 1041` (backup download/upload use raw `fetch`), `frontend/src/hooks/useVersionCheck.ts:100, 120` (raw `fetch`), `src/main.rs:30-37` (`/health`: `{status:"healthy", version, ...}`) vs `src/api/mod.rs:38-45` (`/api/health`: `{status:"ok", message, ...}`, no version)

Two health endpoints with different shapes. The frontend's `checkHealth()` (`api.ts:508`) targets `/api/health` and is never called, while `useVersionCheck` hits `/health` directly.

### P7 — Preflight scripts have diverged from each other and from CI · **Low**
**Files:** `scripts/preflight.sh` (179 lines), `scripts/preflight.ps1` (350 lines), `.github/workflows/ci.yml:73-82`

- Coverage: `preflight.sh:79-80` makes `cargo-tarpaulin` **required**, while `preflight.ps1:119-122` skips it with a warning.
- Tests: both preflights run `cargo test -- --include-ignored`. CI runs `cargo test --all-features`.
- Migrations: CI applies migrations by piping each `V*.sql` through `psql` (`ci.yml:73-78`), bypassing the refinery runner the application uses (`main.rs:97`), so `refinery_schema_history` is never exercised in CI. This may explain why every migration is written defensively with `IF NOT EXISTS` / `IF EXISTS`.
- MSRV: preflight checks `cargo +1.88.0` (`preflight.sh:98`), but CI only ever builds with `stable` (`ci.yml:57-60`), so the declared `rust-version = "1.88"` is never enforced in CI.

---

## 4. Half-implemented or abandoned

### H1 — Six orphaned tables still in the schema and backups · **High**
**Files:** `migrations/V005` (`categories`), `V006` (`tags`, `item_tags`), `V007` (`custom_fields`, `item_custom_values`), `V013:18-27` (`password_reset_tokens`), `src/models/mod.rs:215-312` (`Category`, `CreateCategoryRequest`, `Tag`, `CreateTagRequest`, `CustomField`, `CustomFieldValue`, `ItemWithRelations`, `CustomFieldWithValue`), `src/db/mod.rs:2805-2876`

No handler, DB method, or frontend screen reads or writes any of these six tables. The only code that touches them is backup export/import, which carries them along. Their purpose was superseded by Organizers (V010/V011). Password reset was replaced by recovery codes and TOTP (V018/V023), so `password_reset_tokens` is a dead table holding token columns. The model structs are dead (only `Category` is referenced, by the unrelated `CategoryBreakdown`). This is priority High because it is the main reason A3's hand-maintained list grows, and because the backup code applies `TRUNCATE ... CASCADE` to tables nobody is responsible for. **Recommendation:** add a migration that drops them, and delete the structs.

### H2 — Legacy `items.category`/`items.location` coexist with Organizers · **Medium**
**Files:** `migrations/V001:6-7, 18-19`, `migrations/V011` (copies them into organizers, but never drops them), `src/models/mod.rs:22-23`, `src/db/mod.rs` (`category`/`location` in every item query), `src/api/mod.rs:1270-1271` (CSV export), `src/db/mod.rs:2603-2690` (category breakdown report uses the **legacy column**)

Items now have two sources of truth for category and location. The UI edits organizer values, but the "category breakdown" report and CSV export read the legacy text column, which nothing in the current UI updates. Reports therefore go stale for any item created after V011. V011 also skips migration when an organizer named "Category" already exists (`ON CONFLICT DO NOTHING` + `IF ... IS NOT NULL`), so the data was not reliably moved.

### H3 — Cookie-based auth fallback that nothing sets, plus localhost-only CORS · **Medium**
**Files:** `src/auth/mod.rs:219-237`, `src/main.rs:170-188`

`extract_token` falls back to an `auth_token` cookie, but no code anywhere sets that cookie (the frontend uses `localStorage` and the `Authorization` header). The fallback is unreachable through normal use but still accepted, which makes it a cookie-auth path with no CSRF protection, kept alive by `.supports_credentials()` on the CORS config. The CORS policy itself only admits `localhost`/`127.0.0.1` origins. The SPA is served same-origin (and in development, Vite proxies `/api`, `vite.config.ts:95-100`), so CORS is not needed in any supported topology, and its comment ("Allow requests with no origin") describes something the code doesn't do. Either finish the cookie design (HttpOnly cookie, SameSite, CSRF token) or remove both.

### H4 — Sample-data migrations V019–V021 and stale references · **Medium**
**Files:** `migrations/V019__add_sample_inventory_data.sql`, `V020__assign_sample_data_to_first_admin.sql`, `V021__remove_sample_data.sql`

Sample data was inserted with explicit IDs 100–104 (V019), assigned to the first admin (V020), then deleted by **ID range** (V021: `DELETE FROM inventories WHERE id BETWEEN 100 AND 104`). V019's explicit inserts never advanced `inventories_id_seq`, so on an install that had already reached IDs 100–104 through normal use before upgrading past V019, V019 would conflict. On one that reached them between V019 and V021, **V021 would delete real user inventories and their items**. V019 and V020 also describe "application logic in `/auth/setup`" that assigns sample inventories (`V019:4-6`, `V020:5`), but no such logic exists in `src/api/auth.rs` any more. Applied migrations must not be edited, but this history should be documented, and `inventories.user_id` (V014) is still nullable, which is how the "ownerless sample inventory" state could exist at all.

Related: `items.inventory_id` still has `DEFAULT 1` (V001/V002/V003), so an insert that omits it silently files the item into inventory #1, which may belong to another user.

### H5 — Dead code · **Low**
- Backend: `api::index` (`src/api/mod.rs:29-36`, never registered), `PermissionLevel::can_edit` / `can_delete` (`models/mod.rs:590, 598`), `CreateUserRequest` (`models/mod.rs:457`), `RecoveryCode` (`:800`), `DatabaseService::clear_item_organizer_values` (`db/mod.rs:1137`), and the test-only `hash_password_sync` / `verify_password_sync` / `create_token` exposed as **public production API** (`auth/mod.rs:321-357`) because `tests/` can only reach `pub` items. They belong behind `#[cfg(any(test, feature = "test-helpers"))]`.
- Frontend: `checkHealth` (`services/api.ts:508`), `getAuthToken` / `getAuthHeaders` (`AuthContext.tsx:355-366`), `currentInventoryId` / `setCurrentInventoryId` (`AppContext.tsx:12-13, 37`, never read), `escapeHtml` (`utils/security.ts:9`, only used inside the same file).

### H6 — `tests/integration_test.rs` tests a stub · **Low**
**File:** `tests/integration_test.rs:10-39`

The file defines its own `test_health_handler` and asserts against it, so it never touches the application's `/health`. It also contains `assert_eq!(2 + 2, 4)` ("keep one simple test that always passes"). It inflates test counts while covering no production code.

### H7 — Routes for logo files that don't exist · **Low**
**Files:** `src/main.rs:264-303`, `frontend/public/`

`main.rs` serves `/logo_icon.png` and `/logo_icon3.png`, but neither file exists in `frontend/public/` (which holds only `logo_full.png`, `logo_full2.png`, `logo_full3.png`). `logo_full2.png` is served and synced (`sync-dist.js`) but referenced by no source file. These are leftovers from logo iterations.

---

## 5. Dependencies: unnecessary, misused, or outdated

### D1 — `rust_decimal` is a pinned dependency with zero uses · **Medium**
**File:** `Cargo.toml:40`

`grep -rw rust_decimal src tests` returns nothing. It pulls in its `tokio-pg` integration and transitive crates, enlarges the `cargo deny` / SBOM surface, and its comment ("For PostgreSQL DECIMAL support") documents an intent that V004 reversed (see P4). Either remove it, or use it and migrate `purchase_price` back to `NUMERIC`.

### D2 — Pinning policy is violated · **Medium**
**Files:** `Cargo.toml:62` (`refinery = "0.8"`), `Cargo.toml:66` (`time = ">=0.3.47"`), `frontend/package.json:25, 39, 43` (`heic2any ^0.0.4`, `fs-extra ^11.2.0`, `rimraf ^6.0.1`), `Dockerfile:38` (`rust:1-alpine`), `.github/workflows/ci.yml:92, 128`, `release.yml:54-126`, `gitlab-mirror.yml:14` (tag-pinned actions)

The stated policy (CLAUDE.md; Cargo.toml comment line 25) is exact `=X.Y.Z` pins and SHA-pinned actions. Exceptions:
- `refinery` accepts any 0.8.x, even though it compiles SQL into the binary.
- `time = ">=0.3.47"` is an unbounded floor that would accept a future 0.4 or 1.0. It is also a **direct dependency used only to force a transitive version**. That job belongs in `Cargo.lock` (`cargo update -p time --precise`) or a `[patch]`, not the dependency list.
- `heic2any ^0.0.4` is effectively pinned (caret on `0.0.x`), but it is inconsistent with the exact pins around it. `heic2any` is also unmaintained, with its last release in 2020.
- `rust:1-alpine` floats across Rust minor versions, while the MSRV is 1.88 and CI uses `stable`, so the Docker image can be built with a different compiler than the one tested.
- `ci.yml` SHA-pins most actions but leaves `cargo-deny-action@v2` and `install-action@v2` on tags. `release.yml`, the workflow that publishes images, SHA-pins **none** of its 8 actions.

### D3 — License metadata contradicts itself · **Medium**
**Files:** `LICENSE` (GPL-3.0), `Cargo.toml:6` (`license = "MIT"`), `flake.nix:87` (`licenses.mit`), `deny.toml:49-61` (copyleft denied "for proprietary compatibility"), `src/main.rs:124` (comment on migrating away from GPL-licensed `actix-governor`)

The repository ships a GPL-3.0 `LICENSE` while the package metadata and Nix derivation declare MIT, so the SBOM (`sbom.json`) and downstream packagers will report the wrong license. The dependency policy was written for a permissive or proprietary product (it rejected `actix-governor` for being GPL), which is inconsistent with the project itself being GPL. Pick one and align all four places. `Cargo.toml:7` also still has the placeholder `repository = "https://github.com/your-org/home-registry"`.

### D4 — The frontend's GitHub version check is blocked by the backend's own CSP · **Medium**
**Files:** `frontend/src/hooks/useVersionCheck.ts:36-45, 119-123`, `src/main.rs:218-225`

`useVersionCheck` calls `https://api.github.com/repos/.../releases/latest`, but the CSP's `connect-src` allows only `'self'`, Google Fonts, and cdnjs. In production (served by actix with that header) the request is always blocked, and the update notifier silently never fires. It works only in `vite dev`, where the header is absent. The same hook also hardcodes `'0.1.0-beta.3'` as a fallback version (`:110`), which is a third copy of the version string alongside `Cargo.toml:3` and `frontend/package.json:4`.

### D5 — Overlapping or underused crates · **Low**
- `validator` is used on 5 structs only (see P3), while username and password rules are hand-written. Commit to one approach.
- Two RNG paths: `rand 0.8` `thread_rng` (`auth/mod.rs:117-124`, `api/auth.rs:1940-1945`) and `argon2::password_hash::rand_core::OsRng` (`auth/mod.rs:245`). Both are fine cryptographically, but `rand 0.8` is now a major version behind and is kept mainly for `Alphanumeric` sampling.
- `tokio` with `features = ["full"]` (`Cargo.toml:31`) when actix already provides the runtime and the code uses only `fs` and `task::spawn_blocking`. This enlarges the deny/SBOM surface for nothing.
- `env_logger` + `log` with `Logger::default()` gives unstructured text logs, while `docs/deployment/monitoring-logging.md` describes structured logging and aggregation. That is a documentation/implementation mismatch more than a dependency bug.

### D6 — Frontend runtime depends on third-party CDNs · **Low**
**Files:** `src/main.rs:218-225`, `frontend/vite.config.ts:27-68`, `frontend/index.html`

Font Awesome and Google Fonts load from CDNs, which forces the CSP to allow `'unsafe-inline'` and `'unsafe-eval'` scripts from `use.fontawesome.com` and `cdnjs.cloudflare.com`, and adds three runtime-caching rules to the service worker. For a self-hosted, PWA, privacy-oriented home inventory app, this external runtime dependency undercuts the offline story and the CSP. Bundling the fonts and icons through npm would remove the CDN allowances and leaks.

---

## Suggested order of attack

1. **A1 + A2 + A5**: centralize authentication into an extractor and authorization into one permission query. This closes the largest functional gaps.
2. **A3 + H1**: drop the orphaned tables, then make backup schema-driven and include `user_totp_settings` and `uploads/`.
3. **P1, A4, D4**: small, contained config fixes with outsized effect (JSON errors, rate limiting, update checks).
4. **A6 + A7**: split the monolith modules and introduce `AppError`. Doing this after step 1 avoids moving code that is about to change.
5. **S3**: generate the TS types, so the backend refactor does not cause frontend drift.
6. Hygiene (S6, S7, D1, D2, D3, H5–H7) can be done incrementally.
