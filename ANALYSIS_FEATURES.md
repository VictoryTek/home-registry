# Feature Recommendations — home-registry

**Scope:** features worth building, grounded in what the codebase already has: data models, endpoints, settings columns, helpers, dependencies, and the deferred items listed in the project's own specs. Nothing here requires re-architecting. Each recommendation builds on existing tables, endpoints, or components.
**Baseline:** `main` @ `89e1e94` (2026-10-13). Companion reports: [ANALYSIS_ARCH.md](ANALYSIS_ARCH.md) (structure) and [ANALYSIS_BUGS.md](ANALYSIS_BUGS.md) (defects).

**Priority = value ÷ effort**
- **High**: users clearly expect it, or it unlocks a lot, and most of the plumbing already exists (roughly ≤ 2–3 days).
- **Medium**: solid value with moderate effort, or high value that needs a migration plus new UI.
- **Low**: nice to have, niche, or the effort is high relative to the payoff.

> **Prerequisite for anything that touches items or inventories:** fix ANALYSIS_BUGS.md SEC1/SEC2 (missing authorization on data endpoints) first. Search, import, bulk actions, and moving items all widen the reach of those endpoints, and building on them before authorization exists would multiply the exposure.

---

## Summary table

| ID | Priority | Category | Feature | Effort |
|----|----------|----------|---------|--------|
| F1 | High | Unfinished | Item search, filter, and sort UI | S–M |
| F2 | High | Unfinished | Organizer-aware reports (fix the "always Uncategorized" breakdown) | M |
| F3 | High | Unfinished | Invite links and registration mode (uses the dormant `password_reset_tokens` design) | M |
| F4 | Medium | Unfinished | Pagination and list density using the stored `items_per_page` | S |
| F5 | Medium | Unfinished | Reorder organizers and options (`display_order` exists, no UI) | S |
| F6 | Medium | Unfinished | Deferred notification features: snooze, custom threshold, sidebar badge | S–M |
| F7 | Medium | Unfinished | Cross-inventory report and home dashboard | S–M |
| F8 | Low | Unfinished | Item deep-link route | S |
| F9 | Low | Unfinished | Server-synced theme (`user_settings.theme` is ignored) | XS |
| F10 | Low | Unfinished | `/metrics` endpoint the monitoring docs already scrape | S |
| C1 | High | Complement | Move and duplicate items between inventories | S |
| C2 | High | Complement | CSV import that mirrors the existing CSV export | M |
| C3 | High | Complement | Phone-friendly item photos (HEIC, compression, camera capture) | XS–S |
| C4 | Medium | Complement | Number and date organizer types | S |
| C5 | Medium | Complement | Maintenance reminders (generalize warranty notifications) | M |
| C6 | Medium | Complement | Receipt and manual attachments (PDF) | M |
| C7 | Medium | Complement | Copy organizer set from another inventory | S |
| C8 | Medium | Complement | Insurance / claim report with photos | M |
| C9 | Medium | Complement | Printable QR labels for inventories and items | S–M |
| C10 | Medium | Complement | Bulk item actions (delete, move, set organizer) | M |
| C11 | Low | Complement | Estimated current value (depreciation) | S |
| G1 | High | Gap | "Sign out everywhere" and session invalidation | S |
| G2 | Medium | Gap | Personal data export for non-admin users | M |
| G3 | Medium | Gap | Activity history on shared inventories | M |
| G4 | Low | Gap | Small form and display gaps (Notes on add form, card totals, quantity 0) | XS |
| A1 | High | Automation | Scheduled backups with retention | S–M |
| A2 | Medium | Automation | Warranty and maintenance alerts via webhook (ntfy, Gotify, Home Assistant) | M |
| A3 | Low | Integration | PWA share target and app shortcuts | S |
| A4 | Low | Integration | Personal API tokens for scripts and Home Assistant | M |

Effort: **XS** < ½ day, **S** ≈ 1 day, **M** ≈ 2–4 days.

---

## 1. Features partially stubbed out or clearly intended but never finished

### F1 — Item search, filter, and sort UI · **High** · S–M
**What exists**
- Backend: `GET /api/items/search/{query}` (`src/api/mod.rs:466-500`) with LIKE-escaping (`src/db/mod.rs:44-49, 391-437`).
- Frontend client: `itemApi.search()` (`frontend/src/services/api.ts:355-360`). **No component calls it.** No page has a search box, filter, or sort control.
- Sort logic already exists for reports: `build_order_by` supports `name`, `price`, `date`, and `category` (`src/db/mod.rs:3088-3107`).

**Feature**
1. A search box in the `InventoryDetailPage` header that filters the loaded `items` client-side by name, description, notes, and organizer text. This needs no backend change, since the page already holds every item.
2. A global search box in the sidebar that calls the backend endpoint. The endpoint must first be scoped to the caller's accessible inventories (ANALYSIS_BUGS.md SEC1) and should also match `item_organizer_values.text_value` and option names (join `organizer_options`), so that "Brand: Bosch" is findable. Results are grouped by inventory and link to the item (see F8).
3. A sort dropdown (name, price, purchase date, warranty expiry, recently added) and filter chips for each `select` organizer (for example "Room = Garage"). Organizer options are already loaded on the detail page (`InventoryDetailPage.tsx:126`).

Move the search term from the path to `?q=`. A path segment breaks on `/` (ANALYSIS_BUGS.md BUG17).

### F2 — Organizer-aware reports · **High** · M
**What exists**
- The report category filter, the "category breakdown" table, CSV export, and `COUNT(DISTINCT category)` all read the **legacy** `items.category` column (`src/db/mod.rs:2425-2429, 2537-2690`; `frontend/src/pages/InventoryReportPage.tsx:133-137`).
- The UI stopped writing `items.category` / `items.location` when Organizers replaced them (V010/V011). No current form has a category input.
- Result: for any item created after V011, the breakdown shows **"Uncategorized: 100%"** and the category filter dropdown is empty. The report page looks finished but is functionally hollow.

**Feature**
- Add a "Group by" selector on the report page that lists the inventory's `select` organizers. The new endpoint `GET /reports/inventory/breakdown?inventory_id=&organizer_type_id=` joins `item_organizer_values → organizer_options` and returns the same `CategoryBreakdown` shape. That lets the existing table render it unchanged.
- Add an "Organizer filters" section to the report filter panel (one dropdown per select organizer), applied in `get_inventory_report_data` as `EXISTS (SELECT 1 FROM item_organizer_values ...)`.
- Add organizer columns to the CSV export (one column per organizer type, with the option name or text value).
- Optional: render the breakdown as horizontal bars, since the endpoint already returns `percentage_of_total`. This needs CSS only, no chart library.

This also resolves the stale-data problem in ANALYSIS_ARCH.md H2 without a data migration.

### F3 — Invite links and registration mode · **High** · M
**What exists**
- `password_reset_tokens (id, user_id, token VARCHAR(64) UNIQUE, created_at)` with three indexes (`migrations/V013:18-27`) has **never been used**. It is the scaffolding for token-based flows.
- `/auth/register` is open to anyone with no off switch (ANALYSIS_BUGS.md SEC12). Admin user creation exists (`POST /admin/users`), but the admin has to choose and communicate a password. Admin password reset also has the admin type the user's new password (`auth.rs:983-1016`).
- `RegisterPage`, `UserManagement`, and the recovery-codes onboarding flow already exist in the UI.

**Feature**
- A **registration mode** setting (`open` / `invite` / `closed`, default `invite` for new installs), stored in a small `app_settings` table (migration) and exposed in Settings → User Management. `register` checks it.
- **Invite links:** an admin clicks "Invite user" and gets `/register?invite=<token>`, with a single use and a configurable expiry (default 7 days). It reuses the `password_reset_tokens` shape (add `purpose` and `expires_at` columns, or a sibling `invite_tokens` table). Optionally, the invite carries "share inventory X with view access" so the new user lands with access.
- **Reset links:** admin "Reset password" produces `/reset?token=…` instead of making the admin set a password. It reuses the same table, which is what it was designed for.
- Store tokens hashed (SHA-256 is fine, since `sha2` is already a dependency) and compare in constant time.

### F4 — Pagination and list density using `items_per_page` · **Medium** · S
**What exists:** `user_settings.items_per_page INT DEFAULT 25` (`migrations/V015:6`), the `UserSettings.items_per_page` model field, and the API round-trip. `grep` finds **zero** frontend uses. The Settings page doesn't even show it.

**Feature:** expose "Items per page" (25 / 50 / 100 / All) in Settings → Display Preferences, and paginate the item grid on `InventoryDetailPage` client-side, since the page already loads all items. Add a grid/list toggle (persisted in `settings_json.itemView`) so large inventories can be scanned as a dense table with name, organizers, price, and warranty. Server-side pagination can come later if inventories grow into the thousands.

### F5 — Reorder organizers and options · **Medium** · S
**What exists:** `organizer_types.display_order` and `organizer_options.display_order`, with composite indexes on `(inventory_id, display_order)` and `(organizer_type_id, display_order)` (`migrations/V010:47-50`). The request models accept `display_order` (`src/models/mod.rs:326-364`), and queries `ORDER BY display_order`. `OrganizersPage` **never sets it**, so everything is 0 and ordering is effectively insertion order.

**Feature:** up and down buttons (or drag handles) on the Organizers page for types and for options. Add a batch endpoint `PUT /inventories/{id}/organizers/order { ids: [...] }` that rewrites `display_order` in one transaction. Organizer order also controls form field order on the add/edit item modals, and which image organizer becomes the card thumbnail ("first image organizer wins by display_order", `src/db/mod.rs:1162-1199`). Users currently have no control over either.

### F6 — Deferred notification features · **Medium** · S–M
**What exists**
- `checkWarrantyNotifications(items, dismissed, daysThreshold = 30)` already takes a threshold parameter, but it is hardcoded at the call site (`frontend/src/utils/notifications.ts:24-28`, `AppContext.tsx:84`).
- Dismissals persist in `settings_json.dismissedWarranties` with `dismissedAt` (`AuthContext.tsx:261-290`).
- The project's own specs list these as deferred (`.github/docs/SubAgent docs/notification_enhancements_spec.md` "Future Enhancements": snooze, custom thresholds, batch dismiss; `notifications_page_spec.md` §2.4.4: sidebar badge).

**Feature**
- **Warning window setting:** "Warn me N days before warranty expiry" (7 / 30 / 60 / 90), stored in `settings_json.warrantyThresholdDays` and passed through.
- **Snooze** instead of only dismiss: "Remind me in 7 days" stores `snoozedUntil` next to `dismissedAt`, and the filter becomes `dismissed && (!snoozedUntil || snoozedUntil > now)`.
- **Batch actions** on `NotificationsPage`: "Dismiss all expired" and "Dismiss selected".
- **Sidebar badge** on the Notifications nav item (the count is already computed for the header bell, `Header.tsx:19`).
- Also prune dismissals for items that no longer exist (ANALYSIS_BUGS.md BUG17 notes `settings_json` grows without limit).

Do this together with, or after, moving notification computation server-side (A2 / ANALYSIS_ARCH.md A11). Otherwise the page stays empty on direct load (ANALYSIS_BUGS.md BUG11).

### F7 — Cross-inventory report and home dashboard · **Medium** · S–M
**What exists**
- All three report endpoints already treat `inventory_id` as optional and, when it is absent, aggregate across **every accessible inventory** (`src/db/mod.rs:2525-2600, 2603-2690`).
- `GET /reports/inventory/statistics` returns total items, total value, quantity, category count, inventory count, and oldest/newest purchase (`src/models/mod.rs:161-171`).
- The frontend only routes to `/inventory/:id/report` (`App.tsx:189-196`). `InventoryReportPage` already handles `id === undefined` (`InventoryReportPage.tsx:20`), but no route reaches it.

**Feature**
- Add a `/reports` route (sidebar entry "Reports") that renders `InventoryReportPage` with an inventory selector that includes "All inventories".
- Add a summary strip at the top of `InventoriesPage` showing total items, total value, inventory count, and warranties expiring soon, fed by the statistics endpoint. This also gives the server a natural place to return per-inventory item counts, which removes the N+1 fan-out in ANALYSIS_BUGS.md PERF2.

### F8 — Item deep-link route · **Low** · S
**What exists:** notifications navigate to an inventory and pass `location.state.openItemId`, which opens the item modal (`InventoryDetailPage.tsx:157-168`). The spec explicitly notes the gap: "The app does not have a direct item detail route" (`notifications_page_spec.md:363`). `GET /items/{id}` exists.

**Feature:** a `/inventory/:id/item/:itemId` route that opens the existing view modal on top of the inventory page. That makes item URLs shareable and bookmarkable, lets global search (F1) and QR labels (C9) link straight to an item, and removes the `history.replaceState` workaround (ANALYSIS_BUGS.md BUG17).

### F9 — Server-synced theme · **Low** · XS
**What exists:** `user_settings.theme VARCHAR(20) DEFAULT 'light'` (`migrations/V015:5`) and `UpdateUserSettingsRequest.theme`. The frontend stores the theme **only** in `localStorage` (`AppContext.tsx:31-46`) and never reads or writes the server value.

**Feature:** on login, initialise from `settings.theme`. On toggle, call `updateSettings({ theme })`. Add `system` as a third value that follows `prefers-color-scheme`. Keep `localStorage` as the pre-login fallback to avoid a flash on load.

### F10 — `/metrics` endpoint the monitoring docs already scrape · **Low** · S
**What exists:** `docs/deployment/monitoring-logging.md:263-270` ships a Prometheus scrape job `home-registry-app → app:8210` labelled "if metrics endpoint added", and `:621-643` sketches the middleware. No `/metrics` route exists, so following the docs produces a permanently failing target.

**Feature:** a `/metrics` endpoint (actix-web-prom or a hand-rolled text exposition) with request counts and latencies by route, DB pool size and waiters, login success/failure counters, and a backup-age gauge (useful once A1 exists). Gate it behind an env flag and keep it outside `/api` so it bypasses the rate limiter. New dependencies must pass `deny.toml`. Otherwise, remove the scrape job from the docs.

---

## 2. Features that complement the existing code and data models

### C1 — Move and duplicate items between inventories · **High** · S
**What exists:** `UpdateItemRequest.inventory_id` is accepted and written by `update_item` (`src/models/mod.rs:113`, `src/db/mod.rs:306-310`). `EffectivePermissions` already distinguishes `can_add_items` and `can_remove_items` (`src/models/mod.rs:726-739`), which is exactly the permission pair a move needs. The edit modal has no inventory selector, and there is no duplicate action.

**Feature**
- **Move to…** in the item menu: pick from inventories where the user has `can_add_items`. The server must check `can_remove_items` on the source and `can_add_items` on the destination. Organizer values are tied to the source inventory's organizer types, so on move, values for types whose names match in the destination are re-mapped by name, and the rest are dropped with a warning listing them.
- **Duplicate:** `POST /items/{id}/duplicate` copies the row and its organizer values (same inventory), and opens the edit modal on the copy. This is useful for "4 identical dining chairs" or "same model, different room".

### C2 — CSV import that mirrors the existing CSV export · **High** · M
**What exists:** the `csv` crate is already a pinned dependency (`Cargo.toml:59`). `ItemExportRow` defines the exact column set (`src/models/mod.rs:182-196`). `format_items_as_csv` produces it (`src/api/mod.rs:1231-1289`). Multipart upload handling already exists for backups (`src/api/backup.rs:20-25, 297-437`).

**Feature:** `POST /inventories/{id}/import` (multipart CSV) in two phases:
1. **Preview:** parse, validate each row with the existing `CreateItemRequest` validator, and return per-row errors plus a column-mapping proposal. Unknown columns can be mapped to existing organizer types by header name, or create new text organizers.
2. **Commit:** insert all valid rows in **one transaction** and return created/skipped counts.

The UI is a modal on the inventory page: upload, mapping table, preview of the first 20 rows with errors highlighted, then Import. Provide a "Download template" link that hits the export with zero rows. This is the most-requested feature in self-hosted inventory tools, because users arrive with spreadsheets.

### C3 — Phone-friendly item photos · **High** · XS–S
**What exists:** the **inventory cover** upload path already does HEIC→JPEG conversion (`utils/heicConversion.ts`), canvas compression to ≤1920 px (`utils/imageCompression.ts`), and file validation (`utils/validateImageFile.ts`) (`InventoriesPage.tsx:225-275`). The **item image** path (`components/ImageOrganizerInput.tsx:20-53`) does none of this. It rejects HEIC outright (`accept` list, line 161) and rejects anything over 5 MB. Modern phone photos are HEIC by default on iOS and often 5–12 MB.

**Feature:** route `ImageOrganizerInput` uploads through `convertHeicToJpeg` and `compressImage` (converting the resulting data URL to a `Blob` before `FormData`), and add `capture="environment"` so mobile browsers offer the camera directly. That turns "photograph everything in the room" into a working flow for the core home-inventory use case. It uses only existing helpers and needs no backend change.

### C4 — Number and date organizer types · **Medium** · S
**What exists:** the organizer system supports `select`, `text`, and `image`. `image` was added by a 15-line migration that swapped the CHECK constraint (`migrations/V022__add_image_organizer_type.sql`). Values live in `text_value`.

**Feature:** a `V0NN` migration adding `'number'` and `'date'` to the constraint, with the value stored in `text_value` in canonical form (`1234.5`, `YYYY-MM-DD`) and validated server-side in `set_item_organizer_value`. The UI renders `<input type="number">` / `<input type="date">`. This covers common asks (serial number stays text; "Last serviced", "Filter size", "Watts", "Year") without reviving the unused `custom_fields` tables, and it is the foundation for C5.

### C5 — Maintenance reminders · **Medium** · M
**What exists:** the whole warranty notification pipeline: computation (`utils/notifications.ts`), badge, banner, page, dismissal, and the item-card badge (`InventoryDetailPage.tsx:88-98`). It is hardwired to the single `warranty_expiry` column.

**Feature:** let a `date` organizer (C4) be marked "remind me", with an optional repeat interval (for example "HVAC filter: every 90 days from Last changed"). Notifications then draw from both `warranty_expiry` and any reminder-enabled date organizer, using the same statuses and dismissal/snooze model (F6). Add a "Mark done" action that sets the date organizer to today, which restarts the cycle. Schema: two columns on `organizer_types` (`remind boolean`, `repeat_days int NULL`). This turns the app from a static list into something users return to.

### C6 — Receipt and manual attachments (PDF) · **Medium** · M
**What exists:** an upload endpoint with magic-byte sniffing, safe filenames, and size limits (`src/api/mod.rs:954-1113`), served from `/uploads/img`. The `image` organizer stores a URL in `text_value`.

**Feature:** a `file` organizer type (same migration pattern as V022) that accepts `application/pdf` (magic bytes `%PDF-`) and stores files under `uploads/files/`, served with `Content-Disposition: attachment` and `nosniff`. Items get "Receipt", "Manual", and "Warranty card" slots that the user defines per inventory. Proof of purchase is the second most common reason people keep a home inventory, after insurance. Include `uploads/` in backups at the same time (ANALYSIS_ARCH.md A3).

### C7 — Copy organizer set from another inventory · **Medium** · S
**What exists:** organizers are per-inventory (`organizer_types.inventory_id`), and every new inventory starts empty. V011 shows the data shape for bulk-creating types and options.

**Feature:** in the "Create inventory" modal and on the Organizers page, add "Copy organizers from…" (a list of accessible inventories). A single endpoint copies `organizer_types` and their `organizer_options` in one transaction (`INSERT … SELECT … RETURNING` with an id map). Optionally, ship 2–3 built-in starter sets (Room/Brand/Condition, Electronics, Tools) as a JSON constant. This removes the most repetitive setup step for users with several inventories.

### C8 — Insurance / claim report with photos · **Medium** · S–M
**What exists:** the report page already has a print button and `@media print` styles (`InventoryReportPage.tsx:112, 510-520`). `GET /inventories/{id}/item-images` returns item→thumbnail URLs in one query (`src/db/mod.rs:1162-1199`). Statistics give total value.

**Feature:** a "Claim report" print layout: a cover page with owner name, date, and total value, then one row per item with thumbnail, name, organizers (room, brand, serial), purchase date, price, quantity, and line total, grouped by a chosen organizer (for example Room, using F2's grouping). It is browser-printed to PDF, so no PDF library is needed. This is the primary job a home inventory exists to do, and today it requires manual assembly.

### C9 — Printable QR labels · **Medium** · S–M
**What exists:** `qrcodegen 1.8.0`, `qrcodegen-image 1.5.1`, and `image 0.25.9` are **already compiled into the binary** via `totp-rs`'s `qr` feature (`Cargo.toml:52`, `Cargo.lock:1586, 2130, 2136`), and they already pass `cargo deny`. `src/auth/totp.rs:189-192` shows the PNG/base64 pattern.

**Feature:** `GET /inventories/{id}/labels?items=…` returns a printable sheet of QR codes, each encoding the deep link (`/inventory/{id}` or `/inventory/{id}/item/{itemId}`, see F8) with the name underneath. Use cases: a QR on each storage bin or shelf ("Garage – Shelf 3"), and stickers on appliances that open the manual or warranty (C6). Promoting `qrcodegen` to a direct, exactly-pinned dependency adds no new crates to the tree. Alternatively, generate the QR client-side, but that does add a dependency.

### C10 — Bulk item actions · **Medium** · M
**What exists:** single-item delete and update endpoints, the organizer-values batch endpoint (`PUT /items/{id}/organizer-values`), the `ConfirmModal` component, and the permission model.

**Feature:** a selection mode on the item grid (checkboxes, select all in current filter) with bulk delete, move to inventory (C1), set organizer value ("Room = Attic" for 30 items), and export selected to CSV. The backend adds `POST /items/bulk` with `{ ids, action, payload }`, executed in one transaction with per-item permission checks. This is most useful after an import (C2) or a move.

### C11 — Estimated current value · **Low** · S
**What exists:** `purchase_price`, `purchase_date`, and `quantity` on every item. Statistics already sum value.

**Feature:** an optional per-inventory (or per-organizer-option) depreciation rate, for example straight-line over N years with a floor percentage. Show "Purchase value" next to "Estimated current value" in statistics, reports, and the claim report (C8). Calculation is done in SQL at report time, with no stored derived values. This is useful for insurance discussions but is not something most users ask for first.

---

## 3. Gaps users of an app like this would obviously expect

### G1 — "Sign out everywhere" and session invalidation · **High** · S
**What exists:** stateless JWTs (`src/auth/mod.rs:143-217`), a per-request user lookup that already loads the `users` row (`src/api/auth.rs:72-100`), and password change, reset, and recovery flows.

**Feature:** add `users.token_version INT NOT NULL DEFAULT 0`, put it in the claims, and reject tokens whose version doesn't match during the existing lookup, so no extra query is needed. Increment it on password change, recovery-code or TOTP reset, admin deactivation or demotion, and a new **"Sign out of all devices"** button in Settings → Account. This closes ANALYSIS_BUGS.md SEC8 and is the standard expectation once an app has 2FA and recovery codes.

### G2 — Personal data export for non-admin users · **Medium** · M
**What exists:** full-database backup is **admin-only** (`src/api/backup.rs`). Regular users can CSV-export one inventory's report but cannot get their organizer definitions, organizer values, or images. `get_accessible_inventories` and `export_all_data`'s `to_jsonb` pattern can be scoped per user.

**Feature:** Settings → "Export my data" streams a ZIP containing one JSON file per owned inventory (inventory, items, organizer types, options, and values) plus the referenced images from `uploads/`. An "Import my data" counterpart can come later. This gives users of a shared household instance ownership of their own data without admin involvement.

### G3 — Activity history on shared inventories · **Medium** · M
**What exists:** a multi-user permission model (owner, per-inventory shares with three levels, All Access grants), but no record of who changed what. `updated_at` is overwritten on every change.

**Feature:** an `inventory_activity (id, inventory_id, user_id, action, entity, entity_id, summary JSONB, created_at)` table, written in the item, organizer, share, and transfer handlers with one insert each, inside the same transaction once ANALYSIS_ARCH.md P2 is addressed. Show an "Activity" tab on the inventory page ("Alex moved 'Drill' to Garage, 2 h ago") and keep 90 days, pruned by the A1 scheduler. In a shared household, "who deleted the lawnmower?" is a predictable question.

### G4 — Small form and display gaps · **Low** · XS
Individually tiny, but each is a visible gap:
- The **Add item** form has no **Notes** field, although Edit does (`InventoryDetailPage.tsx:640-840` vs `:1244`) and the model and DB support it.
- **Inventory cards** show item counts but not total value, which is already computable from the loaded items or from the F7 statistics endpoint.
- **Quantity 0** cannot be entered (`parseInt(v) || 1`), although the backend validator allows 0. "Out of stock / used up" is a real state for consumables.
- **Location** on inventories exists (`inventories.location`) but is not shown on cards or used as a filter.

---

## 4. Integrations and automations the structure is ready for

### A1 — Scheduled backups with retention · **High** · S–M
**What exists:** `create_backup_file` is a self-contained async function (`src/api/backup.rs:76-124`), and tokio's full runtime is available. A `backups` volume is declared in both compose files and the Nix module (`docker-compose.yml:69`, `docker-compose.prod.yml:58`, `nix/module.nix:18`). The backup spec itself deferred "auto-cleanup of old auto-backups" (`backup_restore_spec.md:1308`), and pre-restore auto-backups currently accumulate forever.

**Feature:** env-configured `BACKUP_SCHEDULE` (for example `daily@03:00`, or simply `BACKUP_INTERVAL_HOURS=24`) and `BACKUP_RETENTION=14`. A `tokio::spawn` loop started in `main` after migrations calls `create_backup_file(…, "home_registry_scheduled")` and then deletes the oldest `home_registry_scheduled_*` and `home_registry_auto_pre_restore_*` files beyond retention. The Backup & Restore section shows "Last scheduled backup: …" and "Next: …". Fix the 2FA and images gaps first (ANALYSIS_BUGS.md BUG2/BUG14), because automating a backup that can't restore correctly gives false confidence.

### A2 — Warranty and maintenance alerts via webhook · **Medium** · M
**What exists:** warranty logic (client-side), `user_settings.notifications_enabled`, and `settings_json` for per-user preferences. There is no email anywhere: the `email` column was dropped in V017, so SMTP-based alerts would be a regression in scope. The app is self-hosted, where ntfy, Gotify, Apprise, and Home Assistant webhooks are the common notification channels.

**Feature**
- Move warranty and reminder computation to a SQL query (`warranty_expiry BETWEEN now() - 30d AND now() + threshold`, plus C5 reminders) behind `GET /notifications`. This also fixes ANALYSIS_BUGS.md BUG11 and ANALYSIS_ARCH.md A11.
- A daily job (same scheduler as A1) that, for each user with `settings_json.webhookUrl` set, POSTs a JSON digest (`{ expiring: [...], expired: [...] }`), with formats for ntfy and generic JSON. A "Send test notification" button goes in Settings.
- Restrict webhook URLs to `http(s)`, apply a timeout, and do not follow redirects to internal addresses unless an admin enables it. Webhooks are an SSRF surface, so treat them that way.

Web Push through the existing service worker is the other option, but it needs VAPID keys, a subscription table, and per-browser opt-in. Do webhooks first.

### A3 — PWA share target and app shortcuts · **Low** · S
**What exists:** a full PWA setup (`vite-plugin-pwa`, `frontend/public/manifest.webmanifest`, standalone display, icon set), but no `shortcuts` or `share_target`.

**Feature:** add `shortcuts` ("Add item", "Notifications", "Search") so long-pressing the home-screen icon on Android or desktop jumps straight in. Add a `share_target` (`POST`, `multipart/form-data`, accepting images) so "Share → Home Registry" from the phone gallery opens "Add item" with the photo pre-attached, which fits C3. This is manifest edits plus one route that reads the shared file from the service worker.

### A4 — Personal API tokens · **Low** · M
**What exists:** bearer-token extraction (`src/auth/mod.rs:219-237`) and a consistent JSON API, but the only credentials are 24-hour JWTs from interactive login, which is impractical for scripts, Home Assistant REST sensors, or cron exports.

**Feature:** Settings → "API tokens": create a named token with an expiry and read-only or read-write scope, shown once and stored hashed (`sha2`). `get_auth_context_from_request` accepts `Bearer hr_…` tokens by hash lookup, alongside JWTs. Example uses: a Home Assistant sensor for "warranties expiring this month", or a nightly script exporting CSV to a NAS. Build this after G1, so tokens are revocable through the same mechanism.

---

## Considered and not recommended

| Idea | Why not |
|------|---------|
| Revive the `tags` / `custom_fields` tables (V005–V007) | Organizers already cover both. Reviving them would create a third classification system. Drop the tables (ANALYSIS_ARCH.md H1) and extend organizers instead (C4). |
| Email password reset and email notifications | The `email` column was deliberately removed (V017), and there is no SMTP configuration. Invite and reset links (F3) and webhooks (A2) fit the self-hosted model without collecting email. |
| Barcode / UPC product lookup | Needs a third-party product database (usually paid or rate-limited) and camera barcode decoding. That is high effort for spotty data quality. QR labels (C9) give the "scan to find" benefit without the lookup. |
| Multi-language UI (i18n) | Touches every string in ~30 components. That is valuable eventually, but it is a cross-cutting refactor rather than a feature, and it is out of scope by the "no rearchitecture" rule. |
| Native mobile app | The PWA plus C3 and A3 deliver camera capture, home-screen install, and share-sheet integration with no second codebase. |
| Multi-select organizers ("tags") | Requires dropping the `unique_item_organizer (item_id, organizer_type_id)` constraint and reworking value upserts and every reader. That is doable later, but not cheap enough to rank today. |

---

## Suggested build order

1. **Foundation (after ANALYSIS_BUGS.md SEC1–SEC4):** G1 (session invalidation), F3 (invite links / registration mode), A1 (scheduled backups).
2. **Quick wins:** C3 (phone photos), G4 (form gaps), F9 (theme sync), F5 (reorder).
3. **Find and organize:** F1 (search/filter/sort), C1 (move/duplicate), C7 (copy organizers), F4 (pagination).
4. **Make reports real:** F2 (organizer-aware reports), F7 (dashboard / cross-inventory), C8 (claim report).
5. **Bring data in and out:** C2 (CSV import), C10 (bulk actions), G2 (personal export).
6. **Keep users coming back:** C4, then C5 (date organizers and maintenance reminders), F6 (snooze/thresholds), A2 (webhook alerts).
7. **Extras:** C6 (attachments), C9 (QR labels), G3 (activity log), F8, F10, A3, A4, C11.
