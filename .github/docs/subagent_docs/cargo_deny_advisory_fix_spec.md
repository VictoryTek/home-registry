# Spec: Fix failing `cargo-deny check` advisories in CI

## Current State Analysis

GitHub Actions CI (`EmbarkStudios/cargo-deny-action@v2`, `command: check`) has been failing on
the `advisories` check. Full output was supplied by the user. Relevant findings:

1. **`error[unsound]` RUSTSEC-2026-0097** — `rand 0.8.5` (pinned `Cargo.toml:46` as `rand =
   "=0.8.5"`). Unsound under a narrow custom-logger + `ThreadRng` reseed scenario.
   Solution per advisory: upgrade to `>=0.8.6` (also fixed on `>=0.9.3` / `>=0.10.1`, not
   applicable here since we're on the 0.8 line).
2. **`error[vulnerability]` RUSTSEC-2026-0178** — `tokio-postgres 0.7.12` (pinned
   `Cargo.toml:35` as `tokio-postgres = { version = "=0.7.12", ... }`). A malicious/MITM
   Postgres server can send a truncated `DataRow` and panic the client (DoS). Solution:
   upgrade to `>=0.7.18`.
3. A third block (unmaintained `proc-macro-error2`, RUSTSEC-2026-0173, pulled in
   transitively via `validator 0.19.0` -> `validator_derive 0.19.0`) appears in the pasted
   log with "Solution: No safe upgrade is available!" — this is an `unmaintained` advisory
   (config has `unmaintained = "all"` in `deny.toml`), not one of the two `error[...]` blocks
   that caused the final `advisories FAILED`. It is not confirmed to be blocking on its own,
   but `validator 0.21.0` drops `proc-macro-error2` entirely (replaced with
   `proc-macro-error3`), so fixing it costs nothing extra and removes the noise.
4. `warning[duplicate]`: two `base64` versions in the tree (`0.21.7` via `jsonwebtoken 9.3.0`
   -> `jsonwebtoken-9.3.0` deps, `0.22.1` via everything else). `multiple-versions = "warn"`
   in `deny.toml`, so this is **not** a failing check — out of scope for this fix (would
   require a `jsonwebtoken` major-version bump 9.x -> 11.x, which is a larger, riskier change
   unrelated to the CI failure).

Context7 MCP was unavailable this session (connection timeout). Verified current crate
versions directly via crates.io API and the `validator` GitHub changelog instead:

- `tokio-postgres` latest 0.7.x: `0.7.18` (2026-06-12) — fixes RUSTSEC-2026-0178.
- `rand` latest 0.8.x: `0.8.8` (2026-08-25) — includes the RUSTSEC-2026-0097 fix from `0.8.6`.
- `validator` latest: `0.21.0` (2026-07-09) — changelog confirms no breaking changes to the
  `length`/`range` validators or the `Validate` derive between 0.19.0 and 0.21.0; only change
  of note is swapping `proc-macro-error2` for `proc-macro-error3`. Requires `rust-version
  1.88`, which matches this crate's pinned `rust-version = "1.88"` exactly.

Usage of `validator` in this codebase (`src/models/mod.rs`, `src/api/*.rs`,
`src/auth/mod.rs`) is limited to `#[derive(Validate)]` plus `#[validate(length(...))]` and
`#[validate(range(...))]` attributes — all unaffected by the 0.19 -> 0.21 changes.

`deadpool-postgres 0.14.0` and `refinery-core 0.8.16` both depend on `tokio-postgres` via a
`^0.7` (caret) requirement, so bumping the pinned version to `0.7.18` will let Cargo unify
the whole tree on a single `tokio-postgres` version without needing to touch those crates.

## Problem Definition

`Cargo.toml` pins exact dependency versions (`=X.Y.Z`) for supply-chain reasons. Two of those
pins (`rand`, `tokio-postgres`) are older than the patched versions for known RUSTSEC
advisories, so `cargo deny check --all-features` fails in CI on every run. A third
(`validator`) pulls in an unmaintained transitive crate that has since been removed upstream.

## Proposed Solution

Bump three exact version pins in `Cargo.toml`, then regenerate `Cargo.lock` so the advisory
database no longer flags these crates:

| Crate | Current pin | New pin | Reason |
|---|---|---|---|
| `rand` | `=0.8.5` | `=0.8.8` | RUSTSEC-2026-0097 fix (min `0.8.6`); `0.8.8` is latest 0.8.x |
| `tokio-postgres` | `=0.7.12` | `=0.7.18` | RUSTSEC-2026-0178 fix (min `0.7.18`) |
| `validator` | `=0.19.0` | `=0.21.0` | Drops unmaintained `proc-macro-error2` (RUSTSEC-2026-0173) |

No code changes are required — all three are pure dependency bumps with no breaking API
changes affecting how this crate uses them (confirmed above).

## Implementation Steps

1. Edit `Cargo.toml`:
   - `rand = "=0.8.5"` -> `rand = "=0.8.8"`
   - `tokio-postgres = { version = "=0.7.12", ... }` -> `tokio-postgres = { version = "=0.7.18", ... }`
   - `validator = { version = "=0.19.0", features = ["derive"] }` -> `validator = { version = "=0.21.0", features = ["derive"] }`
2. Regenerate `Cargo.lock` for just these three crates: `cargo update -p rand@0.8.5 --precise 0.8.8`, `cargo update -p tokio-postgres --precise 0.7.18`, `cargo update -p validator --precise 0.21.0` (and `validator_derive` will follow automatically as a dependency of `validator`).
3. Verify with `cargo check --all-features` that the workspace still compiles.
4. Verify with `cargo deny check --all-features` (this itself is not in FORBIDDEN COMMANDS —
   only the `cargo preflight` alias and the DB-dependent `cargo test` invocations are
   forbidden) that the `advisories` section now reports OK.

## Dependencies

No new dependencies added. Version bumps only, verified against crates.io (Context7
unavailable this session — see note above).

## Configuration Changes

None. `deny.toml` does not need edits — the advisories simply stop firing once the
vulnerable/unsound/unmaintained versions are out of the dependency tree.

## Addendum (discovered during Phase 2 implementation)

Two additional issues surfaced that were not visible in the user's originally pasted CI log:

1. **Cascading pin conflicts.** `tokio-postgres 0.7.18` requires `postgres-types >=0.2.14`,
   which requires `serde_json >=1.0.144` (was pinned `=1.0.138`) and `serde_core >=1.0.221`
   (from `serde`, was pinned `=1.0.220`). Both are patch-level bumps within the already-used
   1.x line, so also bumped:
   - `serde` `=1.0.220` -> `=1.0.229` (latest)
   - `serde_json` `=1.0.138` -> `=1.0.151` (latest)
2. **New advisory: RUSTSEC-2026-0258** (`h2` "unbounded empty DATA frames", low severity) on
   `h2 0.3.27`, pulled in transitively via `actix-http` (itself pulled in by our pinned
   `actix-web = "=4.12.1"` / `actix-files = "=0.6.10"`). This did not appear in the user's
   pasted log — likely published to the RustSec DB after that CI run. Verified via crates.io
   and `actix-http`'s current `main` branch `Cargo.toml` that **no safe upgrade path exists**:
   the fix landed in `h2 >=0.4.16`, but `actix-http` (including its latest `main` branch as of
   this check) still pins `h2 = "0.3.27"` exactly — even the newest `actix-web 4.15.0` does not
   pull a fixed `h2`. This is an upstream-only fix with no release timeline.
   - **Resolution:** add a documented `ignore` entry for `RUSTSEC-2026-0258` in `deny.toml`,
     consistent with the project's existing pattern of justified exceptions (see
     `deny.toml`'s license `exceptions`/`clarify` sections). Noting it here so it's revisited
     once `actix-web`/`actix-http` migrate to `h2 0.4.x`.

## Risks and Mitigations

- **Risk:** `validator 0.21.0` requires `rust-version 1.88`. — **Mitigation:** confirmed this
  crate already pins `rust-version = "1.88"` in `Cargo.toml`, so no MSRV conflict.
- **Risk:** Lockfile drift for unrelated transitive crates when running `cargo update`. —
  **Mitigation:** use `--precise` targeting only the three crates in question, not a blanket
  `cargo update`, to keep the change surgical.
- **Risk:** `base64` duplicate-version warning and the `jsonwebtoken` major-version gap remain
  unresolved. — **Mitigation:** explicitly out of scope — it's a warning, not a CI failure,
  and fixing it means a `jsonwebtoken` 9.x -> 11.x major bump that needs its own review. Note
  this for the user as a follow-up item.
