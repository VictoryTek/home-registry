# Database Configuration Fix — Review

## Scope Reviewed

- `src/db/mod.rs` (`get_pool()`)
- `docker-compose.yml`
- `docker-compose.prod.yml`
- `.env.example`

## Specification Compliance

All three issues from the spec are addressed:

1. **Discrete-var fallback**: `get_pool()` now tries `DATABASE_URL` first (unchanged
   parse path, full back-compat), and falls back to `POSTGRES_HOST` /
   `POSTGRES_PORT` / `POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB` when
   `DATABASE_URL` isn't set. `POSTGRES_PASSWORD` is required in that path; a clear
   error names both options if neither is set.
2. **Configurable role/db**: both compose files now use `${POSTGRES_USER:-postgres}`
   and `${POSTGRES_DB:-home_inventory}` for the `db` service, its healthcheck, and
   the `app` service — verified overrides propagate to all three (see manual `docker
   compose config` check below). Defaults intentionally left as `postgres` /
   `home_inventory` per explicit user decision (avoids a ~15-file blast radius across
   CI, test scripts, and deployment docs that isn't part of this task).
3. **Dev password fallback removed**: `docker-compose.yml`'s `db` and `app` services
   no longer have `uK8m3NvQ7wPxRj2Y5tLz` baked in; `POSTGRES_PASSWORD` has no default
   and `docker compose config` now warns "not set, defaulting to blank string" rather
   than silently using a working password. `docker-compose.prod.yml`'s
   `INSECURE_DEFAULT_CHANGE_ME` canary is left untouched (out of scope per the
   original ask, which named only the dev file).

## Best Practices / Consistency

- New code in `get_pool()` follows the existing tolerant-parsing style (`unwrap_or`
  defaults for port) rather than introducing `.expect()`/panics.
- No new dependency added — discrete vars are set directly on the existing
  `deadpool_postgres::Config`, avoiding a round-trip through a URL string.
- Compose changes match the existing `${VAR:-default}` idiom already used elsewhere
  in both files (e.g. `PORT`, `RUST_LOG`).

## Completeness

- Verified (via `grep`) that `DATABASE_URL` is read only in `src/db/mod.rs` — no
  other code path depends on it being present in the container environment, so
  switching the `app` service to discrete vars has no other blast radius.
- `.env.example` updated: password section no longer claims a working default
  exists; `POSTGRES_USER`/`POSTGRES_DB` documented as optional overrides.

## Security

- Removes a real, working default password from version control (issue 3).
- Enables (but does not force) connecting as a non-superuser-named role, without
  breaking any of the ~15 other places that assume the default role is `postgres`
  (CI, `.cargo/config.toml`, test scripts/fixtures, deployment docs) — those were
  explicitly confirmed out of scope by the user.

## Performance

No regressions — same pool construction, same recycling config, one extra `env::var`
read on the (already cold, startup-only) fallback path.

## Build Validation

Commands run (all from the spec's safe list; test-DB-dependent commands correctly
skipped per FORBIDDEN COMMANDS):

```
$ cargo fmt -- --check
(clean, no output)

$ cargo check --all-targets --all-features
    Checking home-registry v0.1.0-beta.3 (C:\Projects\home-registry)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.10s

$ cargo clippy --all-targets --all-features -- -D warnings
    Checking home-registry v0.1.0-beta.3 (C:\Projects\home-registry)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.70s

$ cargo test --test test_models
running 9 tests
test test_api_response_structure ... ok
test test_update_item_validation ... ok
test test_create_inventory_validation ... ok
test test_error_response_structure ... ok
test test_item_model_with_optional_fields ... ok
test test_update_inventory_validation ... ok
test test_create_item_validation ... ok
test test_user_response_serialization ... ok
test test_login_request_validation ... ok
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Additionally validated both compose files with `docker compose config -q` (safe —
renders/validates only, does not start containers):

- `docker-compose.yml` and `docker-compose.prod.yml` both parse cleanly.
- Confirmed `POSTGRES_PASSWORD` unset → compose warns "not set, defaulting to blank
  string" (no silent fallback) for both `db` and `app` services.
- Confirmed `POSTGRES_USER=customuser POSTGRES_DB=customdb` overrides propagate to
  the `db` service, the `app` service, and the healthcheck's `pg_isready -U
  customuser`.

## Score Table

| Category                  | Score | Grade |
|----------------------------|-------|-------|
| Specification Compliance   | 100%  | A     |
| Best Practices             | 100%  | A     |
| Functionality              | 100%  | A     |
| Code Quality               | 100%  | A     |
| Security                   | 100%  | A     |
| Performance                | 100%  | A     |
| Consistency                | 100%  | A     |
| Build Success              | 100%  | A     |

**Overall Grade: A (100%)**

## Result: PASS
