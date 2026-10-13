# Nix Flake Packaging — Spec

## Current State Analysis

- Backend: single-package Cargo crate (`home-registry`, edition 2021, MSRV 1.88), no
  workspace, `unsafe_code = "forbid"`. Dependencies are pinned exactly in `Cargo.toml`
  and locked in `Cargo.lock` — no C dependencies beyond OpenSSL-adjacent pure-Rust
  crates (`aes-gcm`, `argon2`, `sha2`, `tokio-postgres` w/ `NoTls`); no vendored/FFI
  build requiring system libs. `rustc` 1.98.1 is available in `nixpkgs unstable`,
  comfortably above the 1.88 MSRV.
- Migrations (`migrations/V001..V023*.sql`) are embedded into the binary at **compile
  time** via `refinery::embed_migrations!("migrations")` in `src/main.rs`, and the app
  runs them itself at **startup** (`src/main.rs:97`, `migrations::runner().run_async(...)`)
  before binding the HTTP listener — exits non-zero if migrations fail. **No external
  migration step is needed**; a NixOS module must NOT add a separate migration
  `preStart` step, since the binary already self-migrates.
- DB connection (`src/db/mod.rs::get_pool`) already supports two paths, in order:
  1. `DATABASE_URL` (single URL, naively split on `@`/`:`/`/` — breaks on special
     characters in the password; this is the bug in the pasted request).
  2. Discrete `POSTGRES_HOST`/`POSTGRES_PORT`/`POSTGRES_USER`/`POSTGRES_PASSWORD`/
     `POSTGRES_DB` env vars (added in commit `4c9dfc4`, already merged). This path
     takes `POSTGRES_PASSWORD` as an opaque string with no delimiter splitting, so it
     already fixes the special-character problem completely.
  **Decision: no further Rust code change is needed for the DATABASE_URL bug.** The
  NixOS module will exclusively use the discrete-env-var path (never set
  `DATABASE_URL`), which sidesteps the parser entirely. This is documented explicitly
  in the module and README so the fix is discoverable, per the user's "either
  percent-decode or add discrete vars" framing — the second option already exists.
- Runtime filesystem layout — the app uses **relative paths from its current working
  directory**, mirroring the Docker image's `WORKDIR /app`:
  - `static/` — frontend build output, served via `actix-files` and
    `NamedFile::open`/`open_async` calls throughout `src/main.rs` and
    `src/api/mod.rs:32`. Read-only at runtime; produced by
    `cd frontend && npm run build` (`tsc -b && vite build`) then synced to
    `../static` by `frontend/sync-dist.js` (mirrors what the Docker frontend-builder
    stage + `COPY --from=frontend-builder .../dist ./static` does).
  - `uploads/img/` — read-write, image upload storage (`src/main.rs:59`,
    `src/api/mod.rs:1075-1142`). Created with `create_dir_all` on startup if missing.
  - JWT secret: `src/auth/mod.rs::get_or_init_jwt_secret` checks, in order:
    `JWT_SECRET_FILE` env var → `/run/secrets/jwt_secret` (Docker secret, N/A here) →
    hardcoded `/app/data/jwt_secret` → `JWT_SECRET` env var → auto-generate and try to
    persist to the hardcoded `/app/data/jwt_secret` (best-effort, absolute path, not
    relative to CWD). **This hardcoded absolute path is out of scope to change** (not
    part of the request, and changing shared auth code is a bigger blast radius than
    this task warrants) — instead the NixOS module sets `JWT_SECRET_FILE` explicitly
    to a path under `dataDir` so the app never falls through to the unwritable
    `/app/data/jwt_secret` branch, and the module itself pre-generates that file on
    first start so restarts don't invalidate tokens.
  - `TOTP_ENCRYPTION_KEY` (optional; derived from `JWT_SECRET` if unset — no separate
    file, no module action needed).
- Frontend build (`frontend/package.json`): `npm run build` = `tsc -b && vite build`;
  needs `frontend/package-lock.json` (present) for reproducible installs. No custom
  native bindings observed in `devDependencies`/`dependencies` (React, Vite, ESLint,
  Prettier, TS — all pure JS/WASM-free as far as this crate needs at build time).
- Existing Docker path (`Dockerfile`, `docker-compose*.yml`) is the reference for
  correct runtime directory layout and must not be modified — this task adds a
  parallel, independent packaging path only.

## Problem Definition

1. No Nix packaging exists. Need a flake exposing a buildable package and a NixOS
   module to run it as a native systemd service, without touching the Docker path.
2. The module must supply a working directory layout (`static`, `uploads/img`, a
   writable JWT-secret location) since the binary hardcodes relative-to-CWD and one
   absolute-path assumption, and must wire Postgres connectivity through the
   already-fixed discrete-env-var path so passwords with `@`/`:`/`/` work.

## Proposed Solution Architecture

### `flake.nix`

- Inputs: `nixpkgs` (`nixos-unstable`, for rustc 1.98 › MSRV 1.88 and current
  `buildNpmPackage`/`importNpmLock` support), `flake-utils` for per-system output
  plumbing.
- Two build steps combined into one package:
  1. **Frontend derivation** — `pkgs.buildNpmPackage` over `frontend/`, using
     `npmDepsHash` (fixed-output derivation, so `npm ci`-equivalent dependency fetch
     is allowed network access at build time in a way Nix can still verify/cache;
     the final package build itself remains network-free). `npmBuildScript = "build"`
     (`tsc -b && vite build`, matching CI's `npm run build`), `installPhase` copies
     `dist/` (and anything `public/` that Vite doesn't already inline) to `$out`,
     mirroring what `sync-dist.js` does for local dev and what the Dockerfile's
     `COPY --from=frontend-builder /app/frontend/dist ./static` does for the image.
  2. **Backend derivation** — `pkgs.rustPlatform.buildRustPackage`, `cargoLock.lockFile
     = ./Cargo.lock` (pinned, no network at build time — matches the exact-pin supply
     chain policy already in place), `src = ./.` filtered to exclude `frontend/`,
     `target/`, `.github/`, etc. (`nix flake check`/`nix build` must not depend on
     anything under `frontend/node_modules` or `target/`). `migrations/` must remain
     in the source tree the crate sees, since `embed_migrations!` reads it at Rust
     compile time.
  3. `postInstall` on the backend derivation copies the frontend derivation's output
     into `$out/share/home-registry/static`, so the flake's `packages.<system>.default`
     is a single package containing both the binary and its static assets — the
     NixOS module only needs to reference one store path.
- `packages.<system>.default` — the combined derivation above.
- `nixosModules.default` — imports `./nix/module.nix` (kept in its own file for
  readability), parameterized with the flake's own package as the module's default
  `package` option (via `specialArgs`/`_module.args` or simply hardcoding
  `self.packages.${system}.default` as the option default — using the latter since
  NixOS modules can't easily see `system` without `specialArgs` plumbing, and this
  keeps the module importable standalone if someone pins `package` explicitly).
- `nix flake check` coverage: the flake's `checks.<system>.default` = the package
  build itself (already covered by `packages.default`) plus, if cheap enough,
  `cargo fmt --check`/`cargo clippy` as a separate lightweight check derivation is
  **out of scope** — Phase 1 decision: keep `nix flake check` to structural
  validation (package + module evaluate), since `cargo fmt`/`clippy` are already
  covered by existing CI/preflight and duplicating them as Nix checks risks drifting
  out of sync with `clippy.toml`. `nix flake check` will therefore validate: the
  package builds, and the NixOS module evaluates cleanly against a minimal
  `nixosSystem` invocation (`checks.<system>.module-eval`, using
  `pkgs.nixosTest`-free `evalModules` or a full `nixosSystem` eval is heavier — a
  minimal `lib.nixosSystem { modules = [ nixosModules.default { services.home-registry.enable
  = true; } ]; }` evaluated is sufficient and cheap).

### `nixosModules.default` (`services.home-registry`)

Namespace: **`services.home-registry`** — matches the project name and the
convention already used by comparable single-binary self-hosted NixOS services.

Options:

| Option | Type | Default | Notes |
|---|---|---|---|
| `enable` | bool | `false` | |
| `package` | package | flake's own package | override point |
| `port` | port | `8210` | matches app's own default (`PORT` env, `src/main.rs:50`) |
| `dataDir` | path | `/var/lib/home-registry` | holds `static` (symlink), `uploads/img`, `backups`, and the persisted JWT secret |
| `environmentFile` | nullOr path | `null` | `systemd.services.home-registry.serviceConfig.EnvironmentFile` — for `POSTGRES_PASSWORD`, optional `JWT_SECRET` override, etc. Never stored in the Nix store. |
| `openFirewall` | bool | `true` | opens `port`/tcp |
| `database.createLocally` | bool | `false` | provisions `services.postgresql` role+db locally |
| `database.host` | str | `"127.0.0.1"` | |
| `database.port` | port | `5432` | |
| `database.user` | str | `"home_registry"` | |
| `database.name` | str | `"home_registry"` | |
| `rateLimitRps` / `rateLimitBurst` | nullOr int | `null` | pass-through to `RATE_LIMIT_RPS`/`RATE_LIMIT_BURST` only if set, otherwise app defaults apply — included because they're one-line env passthroughs already supported by the app, not new surface |

Service (`systemd.services.home-registry`):

- **User/group strategy** (per the "DynamicUser where feasible" instruction):
  - When `dataDir` is the default (`/var/lib/home-registry`): `DynamicUser = true`,
    `StateDirectory = "home-registry"`, `WorkingDirectory =
    "/var/lib/home-registry"`. Systemd owns directory creation/permissions.
  - When `dataDir` is overridden to a non-default path: `DynamicUser` cannot
    manage an arbitrary absolute path (its directory management is scoped to
    `StateDirectory`/`CacheDirectory` under fixed roots), so fall back to a
    dedicated static system user (`users.users.home-registry`,
    `users.groups.home-registry`, both `isSystemUser`/system group) plus a
    `systemd.tmpfiles.rules` entry that creates `dataDir` owned by that user, and
    `WorkingDirectory = cfg.dataDir`. This is the "otherwise a dedicated system
    user" branch.
  - No `unsafe_code` implications either way — this is orchestration only.
- `ExecStartPre` (runs as the same user as `ExecStart`, no root needed since it only
  touches its own writable `WorkingDirectory`):
  - `mkdir -p uploads/img backups data`
  - `ln -sfn <package>/share/home-registry/static static` — symlinks the immutable
    Nix-store static assets into the writable working directory so the app's
    relative `static/...` opens keep working without copying files on every start.
  - If `data/jwt_secret` doesn't exist yet, generate 48 random bytes (`head -c48
    /dev/urandom | base64`) into it with `chmod 600` — makes JWT sessions durable
    across restarts without relying on the app's own hardcoded
    `/app/data/jwt_secret` fallback, which would silently fail to persist under
    this working directory.
- `Environment=`:
  - `PORT=<port>`
  - `POSTGRES_HOST=<database.host>` / `POSTGRES_PORT=<database.port>` /
    `POSTGRES_USER=<database.user>` / `POSTGRES_DB=<database.name>`
    (never `DATABASE_URL` — see the parser note above)
  - `JWT_SECRET_FILE=<WorkingDirectory>/data/jwt_secret`
  - `RATE_LIMIT_RPS=...` / `RATE_LIMIT_BURST=...` only if the corresponding option
    is non-null
  - `RUST_LOG=info` (fixed sane default; not exposed as an option — not asked for,
    and `environmentFile` can override it if a user wants more)
- `EnvironmentFile = cfg.environmentFile` if set (must supply `POSTGRES_PASSWORD` at
  minimum — asserted at eval time when `database.createLocally` is false and no
  password is otherwise reachable... **not statically enforceable** since
  `environmentFile` contents aren't visible at eval time; documented instead, and
  the app itself already fails fast with a clear error — `db::get_pool` returns
  `Err("Either DATABASE_URL or POSTGRES_PASSWORD environment variable must be set")`
  — if it's missing).
- `ExecStart = "${cfg.package}/bin/home-registry"`.
- Hardening: `NoNewPrivileges = true`, `ProtectSystem = "strict"`,
  `ReadWritePaths = [ WorkingDirectory ]`, `ProtectHome = true`,
  `PrivateTmp = true` — standard for a single-binary web service; not explicitly
  requested but a one-line addition consistent with the project's existing security
  posture (`unsafe_code = "forbid"`, hardened Dockerfile) and low risk of breaking
  functionality since all app I/O is confined to `dataDir` + the DB socket.
- `openFirewall`: `networking.firewall.allowedTCPPorts = [ cfg.port ]` when true.

### Local Postgres wiring (`database.createLocally`)

- `services.postgresql.enable = true`
- `services.postgresql.ensureDatabases = [ cfg.database.name ]`
- `services.postgresql.ensureUsers = [{ name = cfg.database.user; ensureDBOwnership
  = true; }]`
- NixOS's `ensureUsers` deliberately never sets a role password (avoids leaking it
  into the Nix store via activation scripts). Since the app connects over TCP
  (`database.host = 127.0.0.1`) it needs password auth, so:
  - `services.postgresql.authentication` gets one extra line:
    `host  <name>  <user>  127.0.0.1/32  scram-sha-256`.
  - A oneshot unit `home-registry-db-password` (`after = [ "postgresql.service" ]`,
    `before = [ "home-registry.service" ]`, `wantedBy = [ "home-registry.service" ]`,
    runs as the `postgres` system user so it can `ALTER ROLE` via peer auth) reads
    `POSTGRES_PASSWORD` from `cfg.environmentFile` (required — asserted at eval time:
    `assertions` entry: `cfg.database.createLocally -> cfg.environmentFile != null`)
    and runs `psql -c "ALTER ROLE ... WITH PASSWORD '...'"`, quoting via `psql`'s
    `\set` + `:'var'` idiom (not raw shell interpolation) to avoid SQL-injecting a
    password that happens to contain a quote — reusing the same "no naive string
    splitting/building" principle behind the DATABASE_URL fix.
  - This keeps the plaintext password out of the Nix store (it's only ever read at
    service-activation time from `environmentFile`, which is expected to be an
    out-of-store secret, e.g. from `sops-nix`/`agenix`/manually deployed).
- When `database.createLocally = false`, none of the above is added — the module
  only emits the `POSTGRES_*` env vars and expects `environmentFile` (or an
  externally-supplied `POSTGRES_PASSWORD`) to point at a reachable external Postgres.

## Implementation Steps

1. `nix/module.nix` — the NixOS module described above.
2. `flake.nix` — inputs, frontend derivation, backend derivation (combining static
   assets via `postInstall`), `packages.<system>.default`, `nixosModules.default`
   (importing `nix/module.nix`, wiring its default `package` to the built package),
   `checks.<system>.default` (package build) and `checks.<system>.module-eval`
   (minimal `nixosSystem` eval), for each of `x86_64-linux`/`aarch64-linux` (Linux
   only — this is a systemd/NixOS module, no Darwin service target; the package
   derivation itself could build on Darwin but the module can't, so system list is
   scoped to Linux to keep `nix flake check` meaningful everywhere it runs).
3. `flake.lock` — generated by `nix flake lock`, committed.
4. README — new "Nix / NixOS Deployment" section: package usage
   (`nix build .#default`, `nix run .#default`), module import example, full option
   table (mirrors the table above), and a minimal `configuration.nix` usage example
   covering both the `database.createLocally = true` (local Postgres) and
   externally-provided-Postgres cases. Explicitly documents: (a) migrations run
   automatically on service start, no manual step; (b) the DATABASE_URL
   special-character bug and why the module sidesteps it via discrete env vars.
5. Verify `nix build .#default` and `nix flake check` locally; fix any evaluation or
   build errors surfaced.

## Dependencies

- `nixpkgs` (`nixos-unstable` branch/tag) — provides `rustPlatform.buildRustPackage`,
  `buildNpmPackage`, `rustc` 1.98.1. No Context7 lookup applicable (Context7 covers
  application libraries/frameworks with versioned API docs, not Nix/nixpkgs itself —
  the `nixos` MCP server is the correct tool for current nixpkgs API/attribute
  verification and was used above to confirm the `rustc` version and general
  `buildNpmPackage`/`buildRustPackage` availability).
- `flake-utils` — thin per-system boilerplate helper, widely used, MIT-licensed.
- No new Rust or npm dependencies are added to `Cargo.toml`/`package.json` — this is
  packaging only.

## Configuration Changes

- New files only: `flake.nix`, `flake.lock`, `nix/module.nix`. No changes to
  `Cargo.toml`, `Dockerfile`, `docker-compose*.yml`, or application source.
- README gains a new section; no existing README content is restructured.

## Risks and Mitigations

- **Frontend fixed-output hash drift**: `buildNpmPackage`'s `npmDepsHash` must be
  updated whenever `frontend/package-lock.json` changes. Mitigated by documenting the
  `nix build` error message (it prints the correct hash on mismatch) in the README.
- **`embed_migrations!` needing `migrations/` at compile time**: ensured by not
  filtering `migrations/` out of the backend derivation's `src`.
- **JWT secret divergence from the Docker deployment's path
  (`/app/data/jwt_secret`)**: irrelevant — the two deployment paths are entirely
  separate environments/machines; each persists its own secret independently, which
  is correct (no requirement that Docker and NixOS deployments share a JWT secret).
- **`database.createLocally` password rotation**: the oneshot `ALTER ROLE` unit is
  `wantedBy = [ "home-registry.service" ]` and re-runs on every `home-registry`
  restart, so changing `POSTGRES_PASSWORD` in `environmentFile` and restarting the
  service picks up the new password automatically (idempotent `ALTER ROLE`).
- **Two Linux architectures listed but only one may be tested locally**: `nix build`
  verification in Phase 6 will only cover the host system's architecture; this is
  called out in the review rather than silently assumed to cover both.
