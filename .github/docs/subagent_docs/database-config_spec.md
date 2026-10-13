# Database Configuration Fix — Spec

## Current State Analysis

- `src/db/mod.rs::get_pool()` requires `DATABASE_URL` and hand-parses it
  (`postgres://user:password@host:port/db`) into a `deadpool_postgres::Config`.
  There is no fallback for discrete `POSTGRES_*` vars — `DATABASE_URL` is mandatory.
- `docker-compose.yml` (dev): `db` service hardcodes `POSTGRES_USER: postgres` and
  `POSTGRES_DB: home_inventory` (not env-overridable); `POSTGRES_PASSWORD` has a
  hardcoded compose-level fallback (`uK8m3NvQ7wPxRj2Y5tLz`) checked into the repo.
  `app` service builds `DATABASE_URL` inline from the same password var.
- `docker-compose.prod.yml`: same `POSTGRES_USER`/`POSTGRES_DB` hardcoding. Password
  fallback is `INSECURE_DEFAULT_CHANGE_ME` (self-documenting canary, not in scope —
  user confirmed only the dev fallback is to be removed).
- `.env.example` documents `POSTGRES_PASSWORD` but frames the dev compose default as
  an acceptable fallback, and doesn't mention `POSTGRES_USER`/`POSTGRES_DB`.
- Sibling project `humidor` (`C:\Projects\humidor`) uses a single non-superuser-named
  role (`humidor_user`/`humidor_db`) for its one Postgres service, referenced by the
  user as the pattern to mirror for "least privilege."
- Investigated blast radius of renaming the default role away from `postgres`: the
  literal role name `postgres` is hardcoded in ~15 other places (CI workflow,
  `.cargo/config.toml`, `tests/common/mod.rs`, `scripts/setup-test-db.sh`,
  `scripts/preflight.sh`/`.ps1`, `README.md`, ~10 files under `docs/deployment/`).
  **User decision: do not rename the default.** Keep `POSTGRES_USER` defaulting to
  `postgres` and `POSTGRES_DB` defaulting to `home_inventory` everywhere; only make
  both configurable via env vars in the two compose files. An operator who wants an
  actual non-superuser role sets `POSTGRES_USER` in their own `.env`.

## Problem Definition

1. The app only accepts a single composed `DATABASE_URL`. Deployments that want to
   keep the password in a separate secret (mounted file, k8s Secret, etc.) and
   assemble the connection from discrete host/user/port/db pieces have no way to do
   that.
2. `POSTGRES_USER`/`POSTGRES_DB` are hardcoded literals in both compose files instead
   of env-overridable, and the app's connection isn't wired to use whatever role is
   configured — everything is stuck on the pattern of connecting as `postgres`.
3. `docker-compose.yml` ships a real, working password default
   (`uK8m3NvQ7wPxRj2Y5tLz`) baked into version control.

## Proposed Solution

### 1. `src/db/mod.rs` — discrete-var fallback (no new dependency)

In `get_pool()`:
- If `DATABASE_URL` is set, keep the existing parse path unchanged (back-compat).
- Else, read discrete vars directly into the `deadpool_postgres::Config` (no need to
  round-trip through a URL string):
  - `POSTGRES_HOST` — default `localhost`
  - `POSTGRES_PORT` — default `5432` (parse `u16`, fall back to `5432` on bad input,
    mirroring the existing tolerant parsing)
  - `POSTGRES_USER` — default `postgres`
  - `POSTGRES_PASSWORD` — required; if neither `DATABASE_URL` nor this is set, return
    a clear error naming both options
  - `POSTGRES_DB` — default `home_inventory`

### 2. Compose files — configurable role/db, discrete vars end-to-end

For both `docker-compose.yml` and `docker-compose.prod.yml`:
- `db` service: `POSTGRES_USER: ${POSTGRES_USER:-postgres}`,
  `POSTGRES_DB: ${POSTGRES_DB:-home_inventory}`.
- `db` service healthcheck: `pg_isready -U ${POSTGRES_USER:-postgres}` (so an override
  doesn't break the healthcheck).
- `app` service: replace the inline `DATABASE_URL: postgres://...` with discrete vars
  (`POSTGRES_HOST: db`, `POSTGRES_PORT: 5432`, `POSTGRES_USER: ${POSTGRES_USER:-postgres}`,
  `POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}` in dev / `${POSTGRES_PASSWORD:-INSECURE_DEFAULT_CHANGE_ME}`
  in prod (unchanged effective default, just moved out of the URL string),
  `POSTGRES_DB: ${POSTGRES_DB:-home_inventory}`). This exercises the new app-side
  discrete-var path and removes the duplicated password interpolation into a URL
  string.
- Dev `db` service: drop the `uK8m3NvQ7wPxRj2Y5tLz` fallback — `POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}`,
  no default.

### 3. `.env.example`

- Update the password section: no compose-level fallback exists anymore in dev; the
  variable is required (`docker compose up` will fail/misbehave without it).
- Document optional `POSTGRES_USER` / `POSTGRES_DB` (defaults `postgres` /
  `home_inventory`), noting they're already covered under "Advanced Configuration."

## Implementation Steps

1. Edit `src/db/mod.rs::get_pool()` to add the discrete-var fallback path.
2. Edit `docker-compose.yml`: `db` env, healthcheck, `app` env (discrete vars, no
   password fallback).
3. Edit `docker-compose.prod.yml`: `db` env, healthcheck, `app` env (discrete vars,
   keep existing `INSECURE_DEFAULT_CHANGE_ME` fallback).
4. Edit `.env.example`: password section wording + document `POSTGRES_USER`/`POSTGRES_DB`.

## Dependencies

None new. Uses `std::env` only, consistent with the existing hand-parsed approach;
`deadpool_postgres::Config` fields are set directly instead of formatting a URL
string and re-parsing it.

## Configuration Changes

New optional env vars: `POSTGRES_HOST`, `POSTGRES_PORT`, `POSTGRES_USER`,
`POSTGRES_DB`. `POSTGRES_PASSWORD` becomes required (no dev compose default).

## Risks and Mitigations

- **Risk:** dev `docker compose up` now fails without `.env`. **Mitigation:** this is
  the intended fix for issue 3; `.env.example` documents the required step, matching
  the header comment already present in `docker-compose.yml`.
- **Risk:** anything relying on `DATABASE_URL` being present inside the app container
  stops seeing it (compose now sets discrete vars instead). **Mitigation:** confirmed
  via grep that `DATABASE_URL` is read only in `src/db/mod.rs`; nothing else in `src/`
  depends on it.
- **Risk:** out-of-scope breakage from renaming the default role. **Mitigation:**
  explicitly avoided per user decision — defaults unchanged, so
  `scripts/setup-test-db.sh`, `scripts/init-test-db.sql`, CI, and docs continue to
  work untouched.

## Test Commands (safe, no live DB needed for this change)

- `cargo fmt -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo check --all-targets --all-features`
- `cargo test --test test_models`

(`cargo test -- --include-ignored` requires the test DB per CLAUDE.md FORBIDDEN
COMMANDS — not run here since defaults are unchanged and no test-DB-dependent code
path is touched.)
