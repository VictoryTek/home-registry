# Spec: Resolve GitHub Actions Node.js 20 deprecation warnings

## Current State Analysis

The user observed this warning in a CI run:

```
Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to
run on Node.js 24: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683,
Swatinem/rust-cache@9d47c6ad4b02e050fd481d890b2ea34778fd09d6
```

GitHub Actions runners are forcibly upgrading these JS actions to run on Node 24 already
(so the current run isn't failing), but the pinned action versions still internally declare
`using: node20` in their `action.yml`, which is deprecated and will eventually stop being
silently upgraded.

Inventoried every `uses:` line across all 4 workflow files
(`ci.yml`, `security.yml`, `release.yml`, `gitlab-mirror.yml`). Two pinning styles exist:
- **SHA-pinned with version comment** (`ci.yml`, `security.yml`) — used for every action
  except the two floating-tag exceptions below. This is the pattern this repo uses for
  supply-chain-pinned actions, and matches the two actions named in the warning.
- **Floating major-version tag** (`release.yml`, `gitlab-mirror.yml`, plus
  `EmbarkStudios/cargo-deny-action@v2` and `taiki-e/install-action@v2` in `ci.yml`) — these
  auto-track the newest release under that tag, so they already pick up Node 24 support the
  moment the upstream project ships it. No action needed there.

Every SHA-pinned action in `ci.yml`/`security.yml` is on an old enough version that it will
hit this same "targets Node 20" warning as soon as its job runs (not just the two the user
happened to see fire first). Checked each one's current latest release and changelog for
breaking changes against this repo's actual usage of it:

| Action | Current pin | Latest | Node ver | Breaking changes affecting this repo? |
|---|---|---|---|---|
| `actions/checkout` | v4.2.2 | v7.0.1 | 24 (since v5.0.0) | v7.0.0 blocks fork-PR checkout on `pull_request_target`/`workflow_run` — repo uses neither trigger anywhere. Safe. |
| `Swatinem/rust-cache` | v2.7.8 | v2.9.2 | 24 (since v2.9.0) | None documented. Safe. |
| `actions/upload-artifact` | v4.6.2 | v7.0.1 | 24 (since v5.0.0) | v5.0.0 note is Node-version-only ("treating it as breaking"); repo's usage (`name`, `path`) unaffected. Safe. |
| `actions/setup-node` | v4.4.0 | v7.0.0 | 24 (since v5.0.0) | v5.0.0 auto-enables caching when `package.json` has a `packageManager` field — checked `frontend/package.json`, no such field present, and repo already sets `cache: "npm"` explicitly. Safe. |
| `docker/setup-buildx-action` | v3.10.0 | v4.4.1 | 24 (since v4.0.0) | v4.0.0 removed deprecated inputs/outputs — repo passes no inputs to this action at all. Safe. |
| `docker/build-push-action` | v6.18.0 | v7.4.0 | 24 (since v7.0.0) | v7.0.0 removed `DOCKER_BUILD_NO_SUMMARY`/`DOCKER_BUILD_EXPORT_RETENTION_DAYS` env vars and legacy export-build tool — repo uses neither; only uses standard `context`/`push`/`load`/`tags`/`cache-from`/`cache-to` inputs. Safe. |
| `github/codeql-action` (`init`/`autobuild`/`analyze`/`upload-sarif`) | v3.28.18 | v4.38.1 | 24 | v4.31.1 removed the `add-snippets` input (not used here); repo only uses `languages`, `category`, `sarif_file`. Safe. |
| `actions/dependency-review-action` | v4.6.0 | v5.0.0 | 24 | Node-version-only major bump; repo only uses `fail-on-severity`, a stable input. Safe. |
| `aquasecurity/trivy-action` | 0.31.0 | v0.36.0 | (composite/Go binary wrapper, no Node deprecation itself, but bumping for currency since Trivy CLI itself gets fixes) | No breaking changes documented across this range; repo uses `image-ref`/`format`/`output`/`severity`/`exit-code`, all stable. Safe. |
| `anchore/sbom-action` | v0.18.0 | v0.24.2 | (Go binary wrapper) | No breaking changes documented; repo uses `path`/`format`/`output-file`, all stable. Safe. |

Not touched (out of scope, no warning applies):
- `dtolnay/rust-toolchain@a54c7afa936fefeb4456b2dd8068152669aa8203 # master` — pinned to a
  specific commit off the `master` branch, not a Node-based action susceptible to this
  deprecation pattern; re-pinning it is an unrelated toolchain-version decision, not a Node20
  fix.
- `EmbarkStudios/cargo-deny-action@v2`, `taiki-e/install-action@v2` — floating tags, already
  auto-current.
- `release.yml`, `gitlab-mirror.yml` — floating tags (`@v4`, `@v3`, `@v5`, `@v1`, `@v0`),
  already auto-current; not SHA-pinned so nothing to bump.

Context7 MCP remains unavailable this session (connection timeout); versions and changelogs
were verified via the GitHub REST API (`api.github.com/repos/.../releases`) and each project's
`CHANGELOG.md`/release notes directly.

## Problem Definition

Every SHA-pinned JS-based action in `ci.yml` and `security.yml` is pinned to a version old
enough to still declare `node20` as its runtime, triggering (or about to trigger) GitHub's
deprecation warning across every job, not just the "Rust" job the user saw first.

## Proposed Solution

Re-pin each affected `uses:` line to the commit SHA of its current latest stable release
(verified Node-24-capable), keeping this repo's existing "SHA + `# vX.Y.Z` comment" pinning
convention. No workflow logic, inputs, triggers, or job structure changes — version bumps only.

## Implementation Steps

1. In `ci.yml` and `security.yml`, replace each pinned SHA + version comment per the mapping
   above, for every occurrence of each action (some appear multiple times across jobs).
2. Validate resulting YAML parses correctly (`python -c "import yaml; yaml.safe_load(open(...))"`
   or equivalent) since GitHub Actions execution itself can't be run locally.
3. Diff-review that only the SHA/comment pairs changed — no other line touched.

## Dependencies

No new dependencies. Pure version-pin bumps to existing third-party GitHub Actions.

## Configuration Changes

None beyond the `uses:` pins themselves.

## Risks and Mitigations

- **Risk:** A SHA typo silently pins to the wrong commit. — **Mitigation:** every SHA above
  was resolved via `git ls-remote --tags` against the exact target tag, not typed by hand.
- **Risk:** A major-version jump changes default behavior in a way not caught by changelog
  review (workflows can't be executed locally to verify). — **Mitigation:** cross-checked
  every documented breaking change against this repo's actual `with:` blocks for each action;
  flagged "Safe" only where the repo doesn't use the affected input/env var/trigger. Residual
  risk is inherent to CI-only verification — recommend the user watch the next Actions run for
  each affected job.
- **Risk:** `codeql-action` v4 requires a newer minimum CodeQL bundle version than the runner
  ships by default. — **Mitigation:** the action bundles/downloads its own CodeQL CLI; this
  only matters for self-hosted runners pinning an old CodeQL CLI, which this repo (GitHub-
  hosted `ubuntu-latest` runners) does not do.
