# CLAUDE.md
**home-registry** — Full-stack self-hosted web application (Rust REST API backend + React/TypeScript SPA frontend). Languages: Rust (edition 2021, MSRV 1.88), TypeScript. Frameworks: actix-web 4 (backend), React 18 + Vite 6 (frontend). Package manager(s): Cargo (backend), npm (frontend — Node >=18, npm >=9).

## Commands

Build:
- `cargo build --all-features` (backend)
- `cd frontend && npm run build` (frontend production build: `tsc -b && vite build`)
- `cd frontend && npm run build:full` (frontend build + sync compiled assets into backend static dir)

Test:
- `cargo fmt -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo deny check`
- `cargo check --all-targets --all-features`
- `cargo test --test test_models` (DB-independent unit tests)
- `cd frontend && npm run typecheck`
- `cd frontend && npm run lint`
- `cd frontend && npm run format:check`
- `cd frontend && npm run ci` (runs typecheck + lint + format:check + build)

Preflight (final gate): `bash scripts/preflight.sh` — only with explicit user approval (see Project-specific rules); otherwise run the Build and Test commands above individually as the gate.
Fast verify (also run by the Stop hook): `cargo build --all-features && cargo test --test test_models`

## Repository

- Key directories:
  - `src/api/` — actix-web route handlers (`auth.rs`, `backup.rs`, `totp.rs`, `mod.rs`)
  - `src/auth/` — authentication, JWT, and TOTP logic
  - `src/db/` — database pool setup and queries (deadpool-postgres / tokio-postgres)
  - `src/models/` — domain models
  - `migrations/` — sequential Refinery SQL migrations (`V001__...sql` … `V023__...sql`), embedded into the binary at compile time
  - `frontend/src/` — React + TypeScript SPA (Vite build, PWA-enabled)
  - `tests/` — integration tests (require the Postgres test DB) plus `tests/common/` shared test helpers; `tests/test_models.rs` is the only DB-independent test file
  - `scripts/` — `preflight.sh` / `preflight.ps1`, `setup-test-db.sh`/`.ps1`, `version-bump.ps1`
  - `flake.nix`, `nix/module.nix` — Nix flake packaging (Linux systems only) and NixOS module, alongside Docker deployment
  - `.github/workflows/` — `ci.yml`, `release.yml`, `security.yml`, `gitlab-mirror.yml`
- Architecture: Layered REST API — actix-web handlers in `src/api/` call into `src/auth/`, `src/db/`, and `src/models/`; PostgreSQL is the sole datastore, accessed through a `deadpool-postgres` connection pool; schema managed exclusively via compile-time-embedded Refinery migrations. The frontend is a separately built React SPA whose production bundle is synced into the backend's static assets directory (`frontend/sync-dist.js`) and served by actix-files from the same binary/container.
- CI environment: GitHub Actions, `ubuntu-latest` runners (free-tier hosted), with a Postgres 16 service container for the `rust` and `coverage` jobs; separate jobs for `cargo-deny`, `frontend` (Node 20), and `docker` (Buildx with registry + GHA layer caching)
- OS requirements: no OS-specific system libraries are required (pure Rust deps, `unsafe_code = "forbid"` at the crate level); the project targets Linux in production (Dockerfile, docker-compose, Nix flake) but is developed cross-platform — note the `cargo preflight` alias is Windows-only (see Forbidden commands)
- Build layout constraints: `Cargo.toml` defines a single package, not a workspace, so bare `cargo build` / `cargo check` work correctly with no `--workspace` flag needed; however integration tests require a live Postgres instance at `postgres://postgres:password@localhost:5432/home_inventory_test` (see Forbidden commands)
- Large disk side-effects: the Dockerfile is a multi-stage build (frontend build stage + Rust build stage) that produces multi-GB intermediate layers; `cargo tarpaulin` recompiles the crate with coverage instrumentation and writes to `coverage/`
- Other constraints: dependencies in `Cargo.toml` are pinned to exact versions (`=X.Y.Z`) for supply-chain security and validated against `deny.toml` (license allowlist + advisory database); do not loosen version pins casually. `clippy.toml` enforces pedantic lints as warnings with a few explicit allowances (see file for the list)
- Special constraints:
  - `unsafe_code = "forbid"` is set crate-wide in `Cargo.toml` — never introduce `unsafe` blocks
  - New DB schema changes must be added as a new sequential `migrations/V0NN__description.sql` file — never edit a previously applied migration
  - Dependency versions are pinned exactly (`=X.Y.Z`); any new/updated dependency must also be checked against `deny.toml`'s license and advisory policy

### Project-specific rules

- `scripts/preflight.sh` already exists and mirrors CI, but it also shells out to Docker, Trivy, cargo-tarpaulin, cargo-audit, and Syft (see Forbidden commands). Do not execute it wholesale without explicit user approval — instead run the safe Test Commands individually to validate Rust/frontend correctness, and defer the container/supply-chain sections of `preflight.sh` to the user or CI.

## Forbidden commands

Never run these. They are also blocked in `.claude/settings.json` where possible.

### Universal (all repositories)

- `git add` (any form)
- `git commit` (any form, including `--amend`)
- `git push` (any form, including `--force`)
- `git stash` (any form, including `pop`, `apply`, `drop`)
- Any other git command that stages, commits, pushes, or stashes changes

What gets staged, committed, pushed, or stashed is the user's decision; the commit message at the end of every change is what the user runs. Read-only git (`status`, `log`, `diff`, `show`) is allowed.

### Project-specific

- `cargo preflight` — reason: this cargo alias (defined in `.cargo/config.toml`) is hardcoded to `powershell -ExecutionPolicy Bypass -File scripts/preflight.ps1` unconditionally. On Linux/Mac (no PowerShell installed) it fails immediately regardless of repo state. Use instead: `bash scripts/preflight.sh` directly (with user approval, see Project-specific rules)
- `cargo test --all-features` / `cargo test -- --include-ignored` run without the test database up — reason: `tests/integration_test.rs`, `tests/test_api_integration.rs`, `tests/test_auth.rs`, and `tests/test_db.rs` open a live connection to `postgres://postgres:password@localhost:5432/home_inventory_test`. Without first running `docker compose up -d db` (or `bash scripts/setup-test-db.sh`), these fail with connection errors on every machine — not flakily. Use instead: `cargo test --test test_models` (pure unit tests, no DB dependency) for routine verification; only run the full suite with explicit user approval after confirming the test DB container is healthy
- `docker build -t home-registry:preflight .` and `trivy image ...` — reason: the multi-stage Docker build produces multi-GB image layers on every invocation and Trivy requires network access to fetch its vulnerability DB. Use instead: none — ask the user (or leave to the CI `docker` job)
- `syft . -o json --file sbom.json` — reason: silently overwrites the checked-in `sbom.json` file in place. Use instead: none — only run when explicitly asked to regenerate the SBOM
- `cargo tarpaulin` (bare) — reason: not installed by default, takes several minutes to run, and writes instrumented build artifacts to `coverage/`. Use instead: none — ask the user (or leave to the CI `coverage` job)

## How to work

### Task size

State the tier in one line before starting.

- **Tier 1 — ask or explore** (questions, reading code, anything that changes no files): just answer. No commit message.
- **Tier 2 — standard change** (bug fix, small feature, config or docs change, tests; a few files, no new dependency, no change to a public interface, schema, or build/CI config): brief plan in chat (goal, assumptions, steps with how each is verified) → implement → verify → preflight → commit message.
- **Tier 3 — major change** (new dependency; many files or modules; public interface, schema, or migration; security-sensitive code; build, CI, or deploy config; hard to reverse): full plan in chat and **wait for approval** → implement → verify → independent review → preflight → commit message.

If a task fits two tiers, use the larger. If a Tier 2 task grows, stop, say so, and continue as Tier 3. The user can ask for more or less process ("just do it", "full process"); verification, forbidden commands, and the commit message still apply.

### Principles

1. **Think before coding.** State your assumptions. If a request has several valid readings, present them or ask; never pick one silently. If a simpler approach exists, say so.
2. **Simplicity first.** Write the minimum code that solves the problem: no extra features, no abstractions for single-use code, no unrequested configurability, no handling for cases that cannot happen. If 200 lines could be 50, rewrite it.
3. **Surgical changes.** Every changed line must trace to the request. Do not refactor or tidy adjacent code; match the existing style. Remove only the imports, variables, and functions your change orphaned. Mention unrelated dead code instead of deleting it.
4. **Goal-driven.** Turn the task into a check you can run before you start: bug fix → a failing test first, then make it pass; new behaviour → tests for it; refactor → tests pass before and after. For multi-step work, list the steps with "→ verify: …" for each.

### Done means verified

- Do not call work finished until the build and tests have actually run and you have reported the real output. Never write "passes" for something you did not run.
- Final gate: run the preflight command; the exit code must be 0. If none exists, run the build and test commands instead, say so, and offer to create a preflight script (do not create it unasked). A failure means the work is not finished.
- A Stop hook (`.claude/hooks/verify.sh`) re-runs the fast verify command when files have changed and blocks finishing on failure. Treat its report as a failed verification: fix it and re-run.
- Tier 3: before preflight, have the `reviewer` subagent review the diff. Give it the approved plan and the changed files, not your conclusions. If it is unavailable, review `git diff` yourself and say it was a self-review. Fix CRITICAL findings only; list RECOMMENDED ones for the user without implementing them.
- After 2 failed fix cycles (a failed review or preflight, fixed, failed again), STOP and report everything. Do not loop silently.
- Do not create spec, plan, or review files in the repository unless asked.

### Do not guess state

Before saying what is committed, pushed, or up to date, check: `git status`; `git log --oneline origin/$(git branch --show-current)..HEAD` (empty output means fully pushed); `stat <lockfile>`. Never tell the user to push, commit, or update without checking first.

### Dependencies and APIs

Before adding a dependency or relying on a versioned API you are unsure of, check current documentation (Context7 MCP if available, otherwise the official docs). Not needed for internal changes, UI-only changes, or dependencies already in the lock file.

### Commit message (final step of every change)

Every change that modifies files ends with a commit message, however small (a one-line typo fix included). It is the last thing in your reply. You never run `git commit`; the user does. Before the message you may add a brief note: RECOMMENDED review items you did not implement, and unrelated issues you noticed.

Format, ready to paste into `git commit`:

```
<type>(<scope>): <description — MAX 72 characters total>

<PARAGRAPH EXPLAINING WHAT CHANGED AND WHY>

Modified Files:
- path/to/file1
- path/to/file2
- path/to/file3

✔ Build successful
✔ Tests passed
✔ Review approved
✔ Preflight passed
```

Valid types: `feat`, `fix`, `chore`, `refactor`, `docs`, `test`, `perf`. Example first line: `fix(network): disable swap on ZFS server roles`

Inside the message: no "Commit Message" heading, no "Edited" summary, no diff statistics, nothing outside the template.

Checkmark lines appear only for checks that actually ran and passed in this session; never include a line for a skipped check. `✔ Review approved` means the Tier 3 review returned PASS (write `✔ Review approved (self-review)` if no independent reviewer was used). Omit the Preflight line if no preflight exists.

After the message, if every check that ran passed, add one line:
> All checks passed. Code is ready to push to GitHub.
