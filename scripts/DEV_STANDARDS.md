# Development Standards

## Principle: If Preflight Passes, CI Should Pass

This project enforces strict quality and security standards through automated checks. **All checks must pass before committing code.** The preflight script mirrors the CI/CD pipeline exactly to catch issues early.

---

## Required Tools

**You must install these tools before running preflight checks:**

### Core Requirements (Platform-Specific)

**Linux/Mac (and CI/CD):**
```bash
# 1. cargo-tarpaulin (code coverage) - REQUIRED
cargo install cargo-tarpaulin

# 2. Trivy (container security scanning) - REQUIRED
# See: https://aquasecurity.github.io/trivy/latest/getting-started/installation/
```

**Windows:**
```powershell
# 1. Trivy (container security scanning) - REQUIRED
choco install trivy
# Or download from: https://github.com/aquasecurity/trivy/releases

# 2. cargo-tarpaulin (code coverage) - OPTIONAL
# Note: cargo-tarpaulin has limited Windows support and may hang
# Coverage is enforced in CI (Linux). Install if you want to try:
cargo install cargo-tarpaulin
```

**Why the difference?**
- cargo-tarpaulin works reliably on Linux/Mac but has known issues on Windows
- Windows users can skip local coverage checks - CI will enforce 80% threshold
- Trivy is required on all platforms for container security scanning

### Optional Tools (Preflight will WARN but not fail)

```powershell
# MSRV toolchain check
rustup toolchain install 1.75.0

# SBOM generation
# Download from: https://github.com/anchore/syft/releases
```

### Database for Integration Tests

**CRITICAL:** Integration tests require a running PostgreSQL database.

**Start the database before running preflight:**

```powershell
docker compose up -d
```

The preflight script will:
- Automatically set `DATABASE_URL=postgres://postgres:password@localhost:5432/home_inventory`
- Run `cargo test -- --include-ignored` to execute ALL tests (including integration tests)
- Verify database container is running
- Fail if database is not accessible

**Note:** You don't need to manually set DATABASE_URL - the preflight script handles it.

---

## Running Preflight Checks

Run **all** checks before committing:

```bash
# Option 1: Via npm script (cross-platform)
npm run preflight

# Option 2: Via cargo alias (cross-platform)
cargo preflight

# Option 3: Direct script execution
# Windows (PowerShell):
powershell -ExecutionPolicy Bypass -File scripts/preflight.ps1

# Linux/Mac (bash):
bash scripts/preflight.sh
```

**Platform Notes:**
- Windows: Uses PowerShell script (`preflight.ps1`) with native Rust tool integration
- Linux/Mac: Uses bash script (`preflight.sh`)
- Both scripts are functionally identical and mirror CI/CD checks

**Exit on first failure:** The preflight scripts use strict error handling and exit immediately on any check failure. Fix issues iteratively.

**Exit on first failure:** The preflight script uses `set -euo pipefail` and exits immediately on any check failure. Fix issues iteratively.

---

## Required Checks

### ✅ Rust Backend

| Check | Command | Enforcement |
|-------|---------|-------------|
| **Formatting** | `cargo fmt -- --check` | Must pass. Rejects non-formatted code. Run `cargo fmt` to fix. |
| **Lints** | `cargo clippy -- -D warnings` | Must pass. Zero warnings allowed. All warnings treated as errors. |
| **Dependency Policy** | `cargo deny check` | Must pass. Rejects banned/vulnerable/duplicate dependencies. |
| **Tests** | `cargo test -- --include-ignored` | Must pass. All unit and integration tests must succeed. **Requires database running.** |
| **Code Coverage** | `cargo tarpaulin` | **≥80% required**. **REQUIRED in CI (Linux)**. OPTIONAL on Windows (limited support). |
| **MSRV Compatibility** | `cargo +1.75.0 check` | OPTIONAL: Warns if Rust 1.75.0 not installed. Run `rustup toolchain install 1.75.0` |

**Configuration files:**
- `rustfmt.toml` - Rust formatting rules
- `clippy.toml` - Clippy lint configuration
- `deny.toml` - Dependency policy (licenses, advisories, bans)

---

### ✅ Frontend (TypeScript/React)

| Check | Command | Enforcement |
|-------|---------|-------------|
| **Type Checking** | `tsc --noEmit` | Must pass. Zero TypeScript errors allowed. |
| **Linting** | `eslint . --max-warnings 0` | Must pass. Zero warnings allowed. |
| **Formatting** | `prettier --check` | Must pass. Run `npm run format` to fix. |
| **Build** | `npm run build` | Must pass. Production build must complete successfully. |

**Configuration files:**
- `tsconfig.json` - TypeScript compiler options
- `eslint.config.mjs` - ESLint rules
- `.prettierrc` - Prettier formatting rules (if exists)

---

### ✅ Container Security

| Check | Tool | Enforcement |
|-------|------|-------------|
| **Multi-Stage Build** | Docker | Must pass. Verifies Dockerfile builds successfully. |
| **Vulnerability Scan** | Trivy | **Fails on HIGH/CRITICAL**. **REQUIRED TOOL - install from https://github.com/aquasecurity/trivy** |

**Configuration file:**
- `Dockerfile` - Multi-stage build definition

---

### ✅ Supply Chain Security

| Check | Tool | Enforcement |
|-------|------|-------------|
| **Rust Dependencies** | `cargo audit` | Must pass. Rejects known vulnerabilities. |
| **NPM Dependencies** | `npm audit --audit-level=high` | Must pass. Rejects HIGH/CRITICAL vulnerabilities. |
| **SBOM Generation** | Syft | OPTIONAL: Warns if not installed. Install from https://github.com/anchore/syft |

---

## Enforcement Thresholds

**Do not weaken these thresholds:**

- Test coverage: **≥80%** (configured in `tarpaulin.toml`)
- ESLint warnings: **0** (`--max-warnings 0`)
- Clippy warnings: **0** (`-D warnings`)
- Trivy severity: **HIGH/CRITICAL** (`--exit-code 1`)
- NPM audit level: **HIGH** (`--audit-level=high`)
- MSRV: **Rust 1.75.0**

---

## Required Tools

**Core (must install):**
- Rust 1.75.0 or later
- Node.js 20+ with npm 9+ (`npm test` in `frontend/` runs the unit tests once per time zone with
  `TZ` set for each run; it stops with an error if a zone does not take effect, for example on a
  Windows machine whose Node ignores `TZ`. Use WSL or rely on CI there)
- Docker

**Optional (recommended for full local validation):**
- `cargo-tarpaulin` - Coverage (`cargo install cargo-tarpaulin`)
- `cargo-audit` - Dependency auditing (`cargo install cargo-audit`)
- `trivy` - Container scanning ([install guide](https://github.com/aquasecurity/trivy))
- `syft` - SBOM generation ([install guide](https://github.com/anchore/syft))

If optional tools are missing, the preflight script will **warn but continue**. However, CI **will fail** if checks don't pass.

---

## CI/CD Pipeline

The GitHub Actions CI pipeline runs **identical checks** to the preflight script:

1. **Rust checks** (formatting, lints, tests, coverage, MSRV)
2. **Frontend checks** (TypeScript, ESLint, Prettier, build)
3. **Container checks** (Docker build, Trivy scan)
4. **Supply chain checks** (cargo audit, npm audit, SBOM)

**CI configuration file:** `.github/workflows/*.yml`

---

## Best Practices

### ✅ DO:
- Run `cargo preflight` before every commit
- Fix issues locally before pushing
- Add tests to maintain ≥80% coverage
- Review Clippy suggestions and fix warnings
- Keep dependencies up to date
- Use `cargo fmt` and `npm run format` to auto-fix formatting

### ❌ DO NOT:
- Push code without running preflight checks
- Disable or bypass lint rules without team discussion
- Use `#[allow(clippy::*)]` without strong justification
- Lower coverage threshold below 80%
- Merge PRs with failing CI checks
- Add `--allow-warnings` flags to bypass checks

---

## Troubleshooting

### "Rust formatting check failed"
```bash
cargo fmt
```

### "Clippy found warnings/errors"
```bash
cargo clippy --fix --all-targets --all-features
```

### "Coverage below 80%"
Add more unit tests to increase coverage. Focus on untested branches and functions.

### "Prettier formatting check failed"
```bash
cd frontend && npm run format
```

### "MSRV 1.75.0 compatibility check failed"
Install Rust 1.75.0 and test:
```bash
rustup toolchain install 1.75.0
cargo +1.75.0 check --all-targets --all-features
```

### "Trivy found HIGH/CRITICAL vulnerabilities"
Update base images in Dockerfile or pin specific versions. Check Trivy output for CVE details.

### "Docker build failed"
Verify Dockerfile syntax and ensure all dependencies are available. Test build manually:
```bash
docker build --pull --build-arg APK_CACHE_BUST=$(date -u +%F) -t home-registry:test .
```
Without the date build argument, Docker caches the apk layer indefinitely and the image can keep outdated packages.

---

## Contributing

**All contributors must:**
1. Run preflight checks before opening a PR
2. Ensure all checks pass locally
3. Not disable or weaken enforcement thresholds
4. Add tests for new functionality (maintain ≥80% coverage)
5. Follow existing code style and patterns

**If preflight passes locally but CI fails:**
- Verify you're on the latest `main` branch
- Check for environment-specific issues (paths, dependencies)
- Review CI logs for differences

---

## Questions?

If you believe a check is too strict or preventing valid work:
1. Open an issue for team discussion
2. Document the justification
3. Get team approval before modifying enforcement rules

**Default answer: Keep standards high. Do not bypass checks.**
