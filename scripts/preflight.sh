#!/usr/bin/env bash
# Preflight checks - mirrors CI pipeline exactly
# Run this before committing to ensure CI will pass

set -euo pipefail

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Minimum coverage threshold
MIN_COVERAGE=48

# Section header
section() {
    echo ""
    echo -e "${BLUE}===================================================================${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}===================================================================${NC}"
}

# Success message
success() {
    echo -e "${GREEN}✓ $1${NC}"
}

# Error message
error() {
    echo -e "${RED}✗ $1${NC}"
    exit 1
}

# Warning message
warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

echo ""
echo -e "${GREEN}╔═══════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                      PREFLIGHT CHECKS                             ║${NC}"
echo -e "${GREEN}║          Running all CI checks locally before commit              ║${NC}"
echo -e "${GREEN}╔═══════════════════════════════════════════════════════════════════╗${NC}"

# ==============================================================================
# RUST CHECKS
# ==============================================================================

section "RUST: Format Check (cargo fmt)"
cargo fmt -- --check || error "Rust formatting check failed. Run: cargo fmt"
success "Rust formatting is correct"

section "RUST: Clippy Lints (cargo clippy)"
cargo clippy --all-targets --all-features -- -D warnings || error "Clippy found warnings/errors"
success "Clippy checks passed"

section "RUST: Dependency Checks (cargo deny)"
cargo deny check || error "Dependency policy violations found"
success "Dependency checks passed"

section "RUST: Unit & Integration Tests (cargo test)"

# Setup test database before running tests
echo "Setting up test database..."
bash "$(dirname "$0")/setup-test-db.sh" || error "Test database setup failed"
success "Test database ready"

# Set DATABASE_URL for integration tests to point to test database
export DATABASE_URL="postgres://postgres:password@localhost:5432/home_inventory_test"
echo "Set DATABASE_URL for integration tests: $DATABASE_URL"

# Run all tests including ignored ones (integration tests)
cargo test -- --include-ignored || error "Tests failed"
success "All tests passed (including integration tests)"

section "RUST: Code Coverage (cargo tarpaulin)"
if ! command -v cargo-tarpaulin &> /dev/null; then
    error "cargo-tarpaulin is REQUIRED but not installed. Install with: cargo install cargo-tarpaulin"
fi

COVERAGE_OUTPUT=$(cargo tarpaulin --out Stdout --skip-clean --exclude-files 'target/*' 2>&1)
echo "$COVERAGE_OUTPUT"

COVERAGE=$(echo "$COVERAGE_OUTPUT" | grep -oP '\d+\.\d+(?=% coverage)' | tail -1 || echo "0")

if (( $(echo "$COVERAGE < $MIN_COVERAGE" | bc -l) )); then
    error "Coverage ${COVERAGE}% is below minimum ${MIN_COVERAGE}%"
fi
success "Coverage ${COVERAGE}% meets minimum ${MIN_COVERAGE}%"

section "RUST: MSRV Compatibility (1.88.0)"
if ! rustup toolchain list | grep -q "1.88.0"; then
    warning "Rust 1.88.0 not installed, skipping MSRV check"
    warning "Install with: rustup toolchain install 1.88.0"
else
    cargo +1.88.0 check --all-targets --all-features || error "MSRV 1.88.0 compatibility check failed"
    success "MSRV 1.88.0 compatibility verified"
fi

# ==============================================================================
# FRONTEND CHECKS
# ==============================================================================

if [ -d "frontend" ]; then
    section "FRONTEND: TypeScript Compilation (tsc)"
    (cd frontend && npm run typecheck) || error "TypeScript compilation failed"
    success "TypeScript compilation passed"

    section "FRONTEND: ESLint"
    (cd frontend && npm run lint) || error "ESLint found errors/warnings"
    success "ESLint checks passed"

    section "FRONTEND: Prettier Format Check"
    (cd frontend && npm run format:check) || error "Prettier formatting check failed. Run: npm run format"
    success "Prettier formatting is correct"

    section "FRONTEND: Unit Tests (every time zone)"
    (cd frontend && npm test) || error "Frontend unit tests failed"
    success "Frontend unit tests passed"

    section "FRONTEND: Build"
    (cd frontend && npm run build) || error "Frontend build failed"
    success "Frontend build passed"
else
    warning "frontend/ directory not found, skipping frontend checks"
fi

# ==============================================================================
# CONTAINER CHECKS
# ==============================================================================

section "CONTAINER: Docker Multi-Stage Build"
docker build -t home-registry:preflight . || error "Docker build failed"
success "Docker build passed"

section "CONTAINER: Trivy Security Scan"
if ! command -v trivy &> /dev/null; then
    error "Trivy is REQUIRED but not installed. Install from: https://github.com/aquasecurity/trivy"
fi

trivy image --severity HIGH,CRITICAL --exit-code 1 home-registry:preflight || error "Trivy found HIGH/CRITICAL vulnerabilities"
success "Trivy security scan passed"

# ==============================================================================
# SUPPLY CHAIN CHECKS
# ==============================================================================

section "SUPPLY CHAIN: Cargo Audit"
if ! command -v cargo-audit &> /dev/null; then
    warning "cargo-audit not installed, skipping audit"
    warning "Install with: cargo install cargo-audit"
else
    cargo audit || error "Cargo audit found vulnerabilities"
    success "Cargo audit passed"
fi

section "SUPPLY CHAIN: NPM Audit"
if [ -d "frontend" ]; then
    (cd frontend && npm audit --production --audit-level=high) || error "NPM audit found HIGH/CRITICAL vulnerabilities in production dependencies"
    success "NPM audit passed"
fi

section "SUPPLY CHAIN: SBOM Generation"
if ! command -v syft &> /dev/null; then
    warning "Syft not installed, skipping SBOM generation"
    warning "Install from: https://github.com/anchore/syft"
else
    syft . -o json --file sbom.json > /dev/null || error "SBOM generation failed"
    success "SBOM generated successfully"
fi

# ==============================================================================
# SUMMARY
# ==============================================================================

echo ""
echo -e "${GREEN}╔═══════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                    ALL CHECKS PASSED ✓                            ║${NC}"
echo -e "${GREEN}║              Your code is ready for CI/CD pipeline                ║${NC}"
echo -e "${GREEN}╚═══════════════════════════════════════════════════════════════════╝${NC}"
echo ""
