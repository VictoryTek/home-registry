# Review: Fix failing `cargo-deny check` advisories in CI

Spec: `cargo_deny_advisory_fix_spec.md` (same directory)

## Changes Reviewed

- `Cargo.toml`: pin bumps only, no new dependencies, no code changes:
  - `rand` `=0.8.5` -> `=0.8.8`
  - `tokio-postgres` `=0.7.12` -> `=0.7.18`
  - `validator` `=0.19.0` -> `=0.21.0`
  - `serde` `=1.0.220` -> `=1.0.229` (cascading requirement of the `tokio-postgres` bump)
  - `serde_json` `=1.0.138` -> `=1.0.151` (cascading requirement of the `tokio-postgres` bump)
- `Cargo.lock`: regenerated via `cargo check --all-features` against the new pins.
- `deny.toml`: added one documented `ignore` entry for `RUSTSEC-2026-0258` (h2 0.3.27, low
  severity, no upstream fix available from `actix-http` yet — see spec addendum for the
  verification done).
- No `src/` files touched.

## 1. Specification Compliance
All three planned version bumps applied exactly as specified. The two cascading `serde`/
`serde_json` bumps and the `deny.toml` ignore entry were not in the original spec but are
documented in the spec's Addendum section before being applied, with rationale and evidence.
**Compliant.**

## 2. Best Practices
Exact-version pinning convention preserved (matches existing `Cargo.toml` style — every bump
uses `=X.Y.Z`, no bare ranges introduced). `deny.toml` ignore entry follows the file's existing
documentation convention (comment above the entry, dated, with a revisit condition), same as
the `licenses.allow` list's "Last reviewed" comment. **Compliant.**

## 3. Consistency
No formatting or structural changes beyond the touched lines in `Cargo.toml`/`deny.toml`.
`cargo fmt -- --check` passes with no diff. **Compliant.**

## 4. Maintainability
The `deny.toml` ignore comment explains why, cites the advisory ID, and gives a concrete
condition for revisiting it (once `actix-web`/`actix-http` migrate off `h2 0.3.x`) rather than
being a silent suppression. **Good.**

## 5. Completeness
Both `error[...]` blocks from the user's original CI log (RUSTSEC-2026-0097 `rand`,
RUSTSEC-2026-0178 `tokio-postgres`) are resolved. The `proc-macro-error2` unmaintained warning
is also resolved as a side effect of the `validator` bump. The newly-surfaced
RUSTSEC-2026-0258 (`h2`) is handled via a justified, narrowly-scoped ignore since no fix is
available upstream. The pre-existing `base64`/`block-buffer`/`convert_case`/`hashbrown`/`hmac`
duplicate-version warnings are unchanged — these are `bans.multiple-versions = "warn"`
(non-failing) and were explicitly called out as out-of-scope in the spec (would require a
`jsonwebtoken` 9.x -> 11.x major bump). **Complete for the stated scope.**

## 6. Performance
No performance-relevant change; dependency patch/minor bumps only. **N/A.**

## 7. Security
This work *is* the security fix — closes two known RUSTSEC advisories (unsound `rand`, DoS
`tokio-postgres`) and removes an unmaintained transitive crate (`proc-macro-error2`). The one
remaining advisory (`h2`, low severity) is explicitly ignored with justification because no fix
exists yet; this is the correct outcome, not a gap. **Compliant.**

## 8. API Currency
Verified via crates.io + `validator`'s GitHub changelog (Context7 MCP was unavailable this
session — connection timeout, noted to the user) that:
- `validator` 0.19.0 -> 0.21.0 has no breaking changes to `Validate`/`length`/`range`, which is
  the only surface this codebase uses (confirmed by grep of `src/`).
- `rand` 0.8.8 and `tokio-postgres` 0.7.18 are patch-level bumps within already-used minor
  lines.
**Compliant.**

## 9. Build Validation

Commands run (all from the approved Test Commands list in `CLAUDE.md`'s Project Context; none
are FORBIDDEN COMMANDS):

| Command | Result |
|---|---|
| `cargo check --all-features` | ✅ Finished, exit 0 (full workspace, 5m21s) |
| `cargo deny check` | ✅ `advisories ok, bans ok, licenses ok, sources ok` (was `advisories FAILED`) |
| `cargo fmt -- --check` | ✅ exit 0, no diff |
| `cargo clippy --all-targets --all-features -- -D warnings` | ✅ exit 0, no warnings |
| `cargo test --test test_models` | ✅ 9 passed; 0 failed (incl. `validator`-derived validation tests) |

Full DB-backed integration suite (`cargo test -- --include-ignored`) was **not** run — it's
listed under FORBIDDEN COMMANDS without the test Postgres container running, and no
confirmation was obtained that `docker compose up -d db` is active. This is out of scope for a
routine dependency-pin fix; flagged to the user as a follow-up if they want full integration
coverage verified too.

`cargo build --all-features` (full release-shaped build) and the frontend build commands were
not re-run — no frontend or `src/` files were touched by this change, only `Cargo.toml`/
`Cargo.lock`/`deny.toml`.

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | N/A | N/A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (100%)**

## Result: PASS

No CRITICAL or RECOMMENDED issues found. Proceeding to Phase 6 (Preflight).
