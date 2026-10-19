# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Search, filter and sort for items (FEAT-F1, BUGS-BUG17).** An inventory page has a toolbar:
  a search box (name, description, notes and organizer values), a sort menu (recently added,
  name, price, purchase date, warranty expiry, either direction; items without a price or date
  always come last), one dropdown per select organizer ("Any", each option, "No value"), removable
  filter chips and a "Showing x of y" line. The choices live in the page address
  (`?q=drill&sort=price&dir=desc&o=3:12`), so reload, back and shared links keep them. Item cards
  show their organizer values (three, then "+N"). A search box in the sidebar opens a new
  `/search?q=` page with matches from every inventory you can see, grouped by inventory, each
  with the organizer value that matched; choosing one opens the item. API:
  `GET /api/items/search?q=<text>&limit=<n>` returns `{results, total, limit}`: each result is the
  item plus `inventory_name` and `matches` (`[{organizer, value}]`); name matches come first, then
  the newest. `limit` defaults to 100 and is lowered to 500; `total` counts every match. The term
  is trimmed, 1 to 100 characters, with no control characters (fixed 400 messages). It matches
  organizer option names and text values too. New `GET /api/inventories/{id}/organizer-values`
  returns the select and text values of every item in one call (view access).
- **Organizer-aware reports (FEAT-F2, BUGS-BUG9).** The report page groups its breakdown by any
  select organizer of the inventory (the first one by default) instead of the legacy category
  column that nothing writes any more, and can filter by organizer, one dropdown per select
  organizer with "Any", each option, or "No value". The statistics, the breakdown, the item list
  and the CSV all describe the same filtered set of items. The items table and the CSV get one
  column per select or text organizer. API: `group_by=<organizer id>` and
  `organizers=<type>:<option>[,...]` (an option id or `none`; OR within one organizer, AND across
  organizers; at most 50 pairs, strictly parsed) on `GET /api/reports/inventory`, `.../statistics`
  and `.../categories`; the report gains `group_by` and per-item `organizer_values`, and breakdown
  rows gain `organizer_option_id`.
- **Registration mode, invitation links and password-reset links (BUGS-SEC12).** Administrators
  choose who may register (open, invite only or closed) under Settings → User Management. They
  create single-use invitation links (7 days by default, 1 hour to 30 days) and password-reset
  links (24 hours by default, 1 hour to 7 days). A reset link lets the user choose a new password
  and signs them out everywhere. Tokens are 32 random bytes, stored only as a SHA-256 hash, shown
  once, and carried in the URL fragment (`/register#invite=...`, `/reset#token=...`) so they never
  reach a server log or a `Referer` header. Redeeming a token and creating the user (or changing
  the password) is one transaction, so a token cannot be used twice and a failed attempt does not
  use it up. New endpoints: `GET`/`PUT /api/admin/settings/registration`,
  `POST`/`GET /api/admin/invites`, `DELETE /api/admin/invites/{id}`,
  `POST /api/admin/users/{id}/reset-link`, `POST /api/auth/invite/check` and
  `POST /api/auth/reset-password`. `GET /api/auth/setup/status` now reports `registration_mode`.
  Restoring a backup revokes every unused invitation and reset link. Audit log lines (target
  `audit`) record the acting user and the objects, never a token.

### Fixed
- **Reports filtered by a date no longer fail.** `from_date` and `to_date` were sent to the
  database as text for a `date` comparison, which the driver refuses, so any report with a date
  filter answered `500`. The statistics and category endpoints now validate their parameters like
  the report does (an invalid date used to reach the database), and the report endpoints no
  longer put raw database error text in their responses.
- **The report's "Average Price" card showed nothing useful** (the page read `average_price`, the
  API sends `average_item_value`). The average is over the items that have a price.
- **Report statistics and breakdown ignored the filters.** They covered the whole inventory next
  to a filtered item list. A missing quantity now counts as 1 everywhere (it did on the item page
  but not in the report totals), and a legacy category that is empty and one that is missing are
  one group.
- **Dates no longer show one day early west of UTC, and the warranty countdown is no longer off
  by one.** Purchase dates and warranty expiry dates are calendar days (`YYYY-MM-DD`), but the
  browser read them as UTC midnight, which is the previous evening in the Americas. They are now
  shown as stored, in every time zone. The warranty countdown counts calendar days from the
  local date, so it is right in the evening and also across a daylight saving change (a week
  used to read as 8 days when the clocks went back). A warranty is valid through its expiry
  date: the item detail shows "(Active)" that day and "(Expired)" from the next, matching the
  notifications. The inventory report's CSV file name now carries the local date, not the UTC
  date.
- **Malformed requests under `/api` now get the standard JSON error response.** Bad JSON, a
  missing or wrong-typed field, an oversized body (413), a wrong `Content-Type`, an invalid
  query string and an unparsable path id used to get actix's plain-text error (the custom
  handler in `main.rs` never ran because the `/api` scope's own config shadowed it). They now
  return `{"success": false, "error": ..., "message": ...}` with the same status as before.
  Messages name the field and the expected type (``missing field `inventory_id` ``,
  `invalid type: string, expected i32`) but never repeat what was sent, including the name of an
  unknown field or enum variant and the value of a bad path id. Formats that are not
  recognised get a fixed generic message. Logs record the method, route pattern and error
  category only.
- **Access logs no longer contain query strings, the `Referer` or the `User-Agent`.** (This covers
  the access log, the rejected-request lines and the unknown-endpoint line; other application
  log lines can still contain names and usernames. A non-standard HTTP method is logged as
  `OTHER`. A backup upload that fails to parse is logged without the file's content.) The default format printed
  the full request line; the new one logs the matched route pattern (`/api/inventories/{id}`),
  so neither the query string nor values in path parameters are written. The unknown-route
  (404) response and log no longer include the query string either. A request that matches no
  route is logged as `<unmatched>` and the 404 message is a fixed sentence; use your reverse
  proxy's access log to see probes.

### Changed
- **Deprecated: `GET /api/items/search/{query}`.** It still works and returns the same list of
  items, but it is now the same search as `GET /api/items/search?q=` (so it also finds notes and
  organizer values, lists name matches first, and returns at most 500 items). Because the term is
  a path segment it cannot contain a `/` (BUG17), which the new route can; the term must be 1 to
  100 characters (a fixed `400` otherwise); the response message no longer repeats the term
  (`Found N items`), and the term is no longer written to the application log. Use `?q=`; the
  path form will be removed in a later release.
- **Breaking: the CSV export's "Category" and "Location" columns are replaced by one column per
  select or text organizer** (named after the organizer, so an inventory with a "Category"
  organizer still has a "Category" column, now filled in). The old columns read the legacy
  `items.category` / `items.location` text, which nothing in the web interface writes. If any
  exported item still holds a legacy value, "Legacy category" / "Legacy location" columns are
  added so nothing is dropped. A missing quantity is exported as 1.
- **CSV export: text that starts with `=`, `+`, `-`, `@`, a tab, a carriage return or a line feed
  (or the full-width forms of `=`, `+`, `-`, `@`) gets a leading `'`** so a spreadsheet does not
  run it as a formula. This applies to names, descriptions, inventory names, organizer names and
  values, and legacy values; numbers and dates are untouched. The quote is common practice
  (CWE-1236), and a spreadsheet that saves the file again can undo it.
- **Deprecated: the breakdown by legacy category.** `GET /api/reports/inventory` without
  `group_by` still groups by the legacy `items.category` column (now filtered, with empty and
  missing categories merged), for API clients. It is removed together with the legacy columns
  (plan item L-38); send `group_by` to group by an organizer.
- **Breaking: registration is invite-only by default (migration V028).** `POST /api/auth/register`
  used to create an account for anyone. Every install, new or upgraded, now starts as invite
  only, and the request needs an `invite_token` from an administrator's invitation link. An
  administrator can switch back to open registration in Settings → User Management; after an
  upgrade that screen shows a notice until it is dismissed, and the server logs a one-time warning
  at start-up. The password field was removed from the admin edit-user form: reset a password with
  a reset link instead (`PUT /api/admin/users/{id}` still accepts `password` for API clients).
  `GET /api/auth/setup/status` gains a `registration_mode` field, and the sign-in page offers
  "Create one" only when registration is open.
- **Breaking (tooling): building the frontend now needs Node.js 20 or newer** (`engines.node` in
  `frontend/package.json` was `>=18`; `engine-strict` is on, so `npm install` refuses older
  versions). The Docker image and CI already used Node 20. Needed for the new test runner.
- **The frontend now has unit tests.** `npm test` runs them once per time zone (eight zones,
  including ones west and east of UTC and ones with daylight saving), and CI and
  `scripts/preflight.sh` / `preflight.ps1` run them. Adds Vitest 4.1.12 as a pinned dev dependency.
- **Breaking (API): `POST /api/items` now requires `inventory_id`.** It must be a positive integer.
  Omitting it, or sending `0` or a negative number, returns `400`; a body without the field is
  rejected while the JSON is parsed. Previously a request without it silently created the item in
  inventory #1, which belongs to whoever created the first inventory. The web interface always
  sent it. Migration V027 removes the database default behind this behaviour
  (`items.inventory_id` no longer defaults to `1`); existing items are not changed. Updating an
  item (`PUT /api/items/{id}`) is unchanged: `inventory_id` stays optional and means "move".

## [0.1.0-beta.3] - 2026-02-20

### Added
- **Complete TOTP 2FA Authentication System**
  - 7 new API endpoints: setup, verify-setup, verify, recover, mode, delete, status
  - Three authentication modes: 2FA only, recovery only, or both
  - TOTP secret encryption at rest using XChaCha20-Poly1305
  - QR code generation for easy authenticator app setup
  - Rate limiting protection (max 5 failed attempts)
  - Database migration V023 for TOTP settings storage
  
- **Frontend TOTP Integration**
  - TotpSettings component with full setup wizard
  - Two-step login flow (password → TOTP code)
  - Password recovery via TOTP (alternative to recovery codes)
  - TOTP mode selection UI (2FA only, recovery only, both)
  - Mobile-optimized code input with numeric keyboard
  - QR code display with manual secret entry fallback

- **Test Coverage Expansion**
  - Added 103+ comprehensive tests across backend and auth
  - TOTP unit tests (generation, encryption, verification)
  - TOTP integration tests (login flow, recovery flow, rate limiting)
  - Test coverage now exceeds 85+ passing tests
  - Full API endpoint coverage in test_api_integration.rs

- **Deployment Documentation**
  - Added 10 comprehensive deployment guides in docs/deployment/
  - Reverse proxy configurations (Nginx, Caddy, Traefik)
  - Production PostgreSQL setup and tuning guide
  - Security hardening best practices
  - Monitoring and logging with Prometheus/Grafana
  - High availability deployment patterns
  - Troubleshooting guide with common issues

### Security
- TOTP secret encryption using XChaCha20-Poly1305 AEAD cipher
- Rate limiting on TOTP verification (max 5 failed attempts before timeout)
- Partial JWT tokens for two-step authentication flow
- TOTP recovery as alternative to recovery codes (reduces attack surface)
- Security hardening documentation with CSP, secrets management

### Changed
- Login flow now supports two-step authentication when TOTP is enabled
- Recovery page now offers TOTP as alternative to recovery codes
- Settings page includes TOTP configuration section
- Authentication context updated to handle TOTP flow state

### Fixed
- All preflight checks now passing (cargo deny, clippy, fmt, test)
- TypeScript compilation clean (tsc, eslint zero warnings)
- Prettier formatting consistent across all frontend files

## [0.1.0-beta.2] - 2026-02-17

### Security
- **CRITICAL:** Fixed 2 GitHub CodeQL DOM-based XSS security alerts in InventoriesPage
  - Added inline sanitization for image preview URLs (lines 526 and 694)
  - Implemented defense-in-depth security with IIFE pattern
  - Prevents potential XSS via malicious image URLs

### Added
- Multi-architecture Docker images (linux/amd64, linux/arm64)
- Automated GHCR publishing workflow
- Security attestation (SBOM, provenance)
- Production Docker Compose configuration
- PWA icon improvements:
  - Multiple favicon sizes (16x16, 32x32, 48x48)
  - Apple touch icon for iOS devices (180x180)
  - Android Chrome icon for PWA (192x192)
  - Updated manifest.webmanifest with new icon references

### Changed
- Split docker-compose.yml into dev and prod variants
- Improved container deployment documentation

### Fixed
- Prettier formatting issues in InventoriesPage.tsx and sanitizeImageUrl.ts
- Trivy vulnerability scanning in release pipeline
- SLSA provenance attestation in CI/CD

## [0.1.0-beta.1] - 2026-02-15

### Added
- **Authentication & User Management**
  - User authentication with JWT tokens
  - Argon2id password hashing
  - Admin and standard user roles
  - Recovery codes for account recovery
  - User settings management

- **Inventory Management**
  - Create, read, update, delete inventories
  - Multi-user support with data isolation
  - Flexible permission system (view/edit/full access)
  - Inventory sharing between users
  - Inventory image uploads

- **Item Management**
  - Full CRUD operations for items
  - Item images and attachments
  - Purchase date and price tracking
  - Warranty information
  - Custom fields support

- **Organization Features**
  - Categories with hierarchical structure
  - Tags for flexible item organization
  - Organizers (locations/containers)
  - Custom fields with type validation

- **Web Interface**
  - Progressive Web App (PWA) support
  - Mobile-responsive design
  - Offline capability
  - Dark mode support
  - Intuitive dashboard

- **API Features**
  - RESTful API with JWT authentication
  - Rate limiting (100 requests/second)
  - Comprehensive error handling
  - Backup/restore endpoints

- **Infrastructure**
  - Docker & Docker Compose deployment
  - PostgreSQL 17 database
  - Automated database migrations
  - Health check endpoints
  - Auto-generated JWT secrets

### Security
- Argon2id password hashing with secure parameters
- JWT token-based authentication
- Rate limiting middleware
- Non-root container user
- Static-linked Alpine-based image
- HTTPS-ready deployment
- No default credentials in production

### Known Issues
- Recovery codes cannot be retrieved after initial setup (save them immediately)
- No email notification system yet
- Limited search filters in current version
- First-time setup requires manual steps

### Technical Details
- **Backend**: Rust 1.88+ with Actix-Web 4.x
- **Frontend**: React 18 + TypeScript + Vite
- **Database**: PostgreSQL 17 with connection pooling
- **Container**: Alpine Linux with multi-stage builds

---

## Release Links

- [0.1.0-beta.1](https://github.com/VictoryTek/home-registry/releases/tag/v0.1.0-beta.1)

[Unreleased]: https://github.com/VictoryTek/home-registry/compare/v0.1.0-beta.3...HEAD
[0.1.0-beta.3]: https://github.com/VictoryTek/home-registry/compare/v0.1.0-beta.2...v0.1.0-beta.3
[0.1.0-beta.2]: https://github.com/VictoryTek/home-registry/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/VictoryTek/home-registry/releases/tag/v0.1.0-beta.1
