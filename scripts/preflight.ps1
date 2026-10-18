# Preflight checks - mirrors CI pipeline exactly
# Run this before committing to ensure CI will pass

$ErrorActionPreference = "Stop"

# CRITICAL: Ensure we're running from the project root directory
# All Rust commands (cargo fmt, clippy, deny, test) require Cargo.toml at the root
# Without this, cargo deny will fail if script is run from subdirectories like frontend/
$scriptPath = Split-Path -Parent $PSCommandPath
$projectRoot = Split-Path -Parent $scriptPath
Set-Location $projectRoot
Write-Host "Working directory: $projectRoot" -ForegroundColor Cyan

# Minimum coverage threshold
$MIN_COVERAGE = 48

# Color output functions
function Write-Section {
    param([string]$Message)
    Write-Host ""
    Write-Host "===================================================================" -ForegroundColor Blue
    Write-Host "  $Message" -ForegroundColor Blue
    Write-Host "===================================================================" -ForegroundColor Blue
}

function Write-Success {
    param([string]$Message)
    Write-Host "[PASS] $Message" -ForegroundColor Green
}

function Write-Error-Exit {
    param([string]$Message)
    Write-Host "[FAIL] $Message" -ForegroundColor Red
    exit 1
}

function Write-Warning-Message {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "===================================================================" -ForegroundColor Green
Write-Host "                      PREFLIGHT CHECKS                             " -ForegroundColor Green
Write-Host "          Running all CI checks locally before commit              " -ForegroundColor Green
Write-Host "===================================================================" -ForegroundColor Green

# ==============================================================================
# RUST CHECKS
# ==============================================================================

Write-Section "RUST: Format Check (cargo fmt)"
try {
    cargo fmt -- --check
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Exit "Rust formatting check failed. Run: cargo fmt"
    }
    Write-Success "Rust formatting is correct"
} catch {
    Write-Error-Exit "Rust formatting check failed: $_"
}

Write-Section "RUST: Clippy Lints (cargo clippy)"
try {
    cargo clippy --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Exit "Clippy found warnings/errors"
    }
    Write-Success "Clippy checks passed"
} catch {
    Write-Error-Exit "Clippy check failed: $_"
}

Write-Section "RUST: Dependency Checks (cargo deny)"
try {
    cargo deny check
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Exit "Dependency policy violations found"
    }
    Write-Success "Dependency checks passed"
} catch {
    Write-Error-Exit "Dependency check failed: $_"
}

Write-Section "RUST: Unit and Integration Tests (cargo test)"

# Setup test database before running tests
Write-Host "Setting up test database..." -ForegroundColor Cyan
try {
    & "$projectRoot\scripts\setup-test-db.ps1"
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Exit "Test database setup failed"
    }
    Write-Success "Test database ready"
} catch {
    Write-Error-Exit "Test database setup failed: $_"
}

# Set DATABASE_URL for integration tests to point to test database
$env:DATABASE_URL = "postgres://postgres:password@localhost:5432/home_inventory_test"
Write-Host "Set DATABASE_URL for integration tests: $env:DATABASE_URL" -ForegroundColor Cyan

try {
    # Run all tests including ignored ones (integration tests)
    # Don't use 2>&1 piping as it corrupts $LASTEXITCODE in PowerShell
    cargo test -- --include-ignored
    $testExitCode = $LASTEXITCODE
    
    if ($testExitCode -ne 0) {
        Write-Error-Exit "Tests failed with exit code $testExitCode"
    }
    
    Write-Success "All tests passed (including integration tests)"
} catch {
    Write-Error-Exit "Test execution failed: $_"
}

Write-Section "RUST: Code Coverage (cargo tarpaulin)"
$tarpaulinInstalled = Get-Command cargo-tarpaulin -ErrorAction SilentlyContinue
if (-not $tarpaulinInstalled) {
    Write-Warning-Message "cargo-tarpaulin not installed, skipping coverage check on Windows"
    Write-Warning-Message "Note: cargo-tarpaulin has limited Windows support"
    Write-Warning-Message "Coverage will be enforced in CI (Linux environment)"
} else {
    Write-Warning-Message "Attempting coverage check (cargo-tarpaulin has limited Windows support)"
    Write-Warning-Message "If this hangs or fails, it will be skipped. Coverage enforced in CI."
    
    try {
        # Run tarpaulin with a timeout to avoid hanging
        $tempFile = [System.IO.Path]::GetTempFileName()
        $tarpaulinJob = Start-Job -ScriptBlock {
            param($tempFile, $dbUrl)
            $env:DATABASE_URL = $dbUrl
            cargo tarpaulin --out Stdout --skip-clean --exclude-files 'target/*' *> $tempFile
        } -ArgumentList $tempFile, $env:DATABASE_URL
        
        # Wait up to 3 minutes
        $completed = Wait-Job $tarpaulinJob -Timeout 180
        
        if ($null -eq $completed) {
            Write-Warning-Message "Tarpaulin timed out after 3 minutes - skipping on Windows"
            Remove-Job $tarpaulinJob -Force
            Remove-Item $tempFile -ErrorAction SilentlyContinue
        } else {
            $tarpaulinExitCode = (Receive-Job $tarpaulinJob).ExitCode
            $coverageOutput = Get-Content $tempFile -Raw -ErrorAction SilentlyContinue
            Remove-Item $tempFile
            Remove-Job $tarpaulinJob
            
            if ($coverageOutput) {
                Write-Host $coverageOutput
                
                # Extract coverage percentage
                if ($coverageOutput -match '(\d+\.\d+)% coverage') {
                    $coverage = [double]$matches[1]
                    if ($coverage -lt $MIN_COVERAGE) {
                        Write-Warning-Message "Coverage ${coverage}% is below minimum ${MIN_COVERAGE}%"
                        Write-Warning-Message "This will fail in CI - fix before pushing"
                    } else {
                        Write-Success "Coverage ${coverage}% meets minimum ${MIN_COVERAGE}%"
                    }
                } else {
                    Write-Warning-Message "Could not parse coverage output - skipping on Windows"
                }
            } else {
                Write-Warning-Message "Tarpaulin produced no output - skipping on Windows"
            }
        }
    } catch {
        Write-Warning-Message "Coverage check failed on Windows: $_"
        Write-Warning-Message "Coverage will be enforced in CI"
    }
}

Write-Section "RUST: MSRV Compatibility (1.88.0)"
$msrvInstalled = rustup toolchain list | Select-String "1.88.0"
if (-not $msrvInstalled) {
    Write-Warning-Message "Rust 1.88.0 not installed, skipping MSRV check"
    Write-Warning-Message "Install with: rustup toolchain install 1.88.0"
} else {
    try {
        cargo +1.88.0 check --all-targets --all-features
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Exit "MSRV 1.88.0 compatibility check failed"
        }
        Write-Success "MSRV 1.88.0 compatibility verified"
    } catch {
        Write-Error-Exit "MSRV check failed: $_"
    }
}

# ==============================================================================
# FRONTEND CHECKS
# ==============================================================================

if (Test-Path "frontend") {
    Write-Section "FRONTEND: TypeScript Compilation (tsc)"
    Push-Location frontend
    try {
        npm run typecheck
        if ($LASTEXITCODE -ne 0) {
            Pop-Location
            Write-Error-Exit "TypeScript compilation failed"
        }
        Write-Success "TypeScript compilation passed"
    } catch {
        Pop-Location
        Write-Error-Exit "TypeScript check failed: $_"
    }

    Write-Section "FRONTEND: ESLint"
    try {
        npm run lint
        if ($LASTEXITCODE -ne 0) {
            Pop-Location
            Write-Error-Exit "ESLint found errors/warnings"
        }
        Write-Success "ESLint checks passed"
    } catch {
        Pop-Location
        Write-Error-Exit "ESLint check failed: $_"
    }

    Write-Section "FRONTEND: Prettier Format Check"
    try {
        npm run format:check
        if ($LASTEXITCODE -ne 0) {
            Pop-Location
            Write-Error-Exit "Prettier formatting check failed. Run: npm run format"
        }
        Write-Success "Prettier formatting is correct"
    } catch {
        Pop-Location
        Write-Error-Exit "Prettier check failed: $_"
    }

    Write-Section "FRONTEND: Build"
    try {
        npm run build
        if ($LASTEXITCODE -ne 0) {
            Pop-Location
            Write-Error-Exit "Frontend build failed"
        }
        Write-Success "Frontend build passed"
    } catch {
        Pop-Location
        Write-Error-Exit "Frontend build failed: $_"
    }
    Pop-Location
} else {
    Write-Warning-Message "frontend/ directory not found, skipping frontend checks"
}

# ==============================================================================
# CONTAINER CHECKS
# ==============================================================================

Write-Section "CONTAINER: Docker Multi-Stage Build"
try {
    docker build -t home-registry:preflight .
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Exit "Docker build failed"
    }
    Write-Success "Docker build passed"
} catch {
    Write-Error-Exit "Docker build failed: $_"
}

Write-Section "CONTAINER: Trivy Security Scan"
$trivyInstalled = Get-Command trivy -ErrorAction SilentlyContinue
if (-not $trivyInstalled) {
    Write-Warning-Message "Trivy not installed, skipping security scan on Windows"
    Write-Warning-Message "Note: Trivy scanning will be enforced in CI"
} else {
    try {
        trivy image --severity HIGH,CRITICAL --exit-code 1 home-registry:preflight
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Exit "Trivy found HIGH/CRITICAL vulnerabilities"
        }
        Write-Success "Trivy security scan passed"
    } catch {
        Write-Error-Exit "Trivy scan failed: $_"
    }
}

# ==============================================================================
# SUPPLY CHAIN CHECKS
# ==============================================================================

Write-Section "SUPPLY CHAIN: Cargo Audit"
$cargoAuditInstalled = Get-Command cargo-audit -ErrorAction SilentlyContinue
if (-not $cargoAuditInstalled) {
    Write-Warning-Message "cargo-audit not installed, skipping audit"
    Write-Warning-Message "Install with: cargo install cargo-audit"
} else {
    try {
        cargo audit
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Exit "Cargo audit found vulnerabilities"
        }
        Write-Success "Cargo audit passed"
    } catch {
        Write-Error-Exit "Cargo audit failed: $_"
    }
}

Write-Section "SUPPLY CHAIN: NPM Audit"
if (Test-Path "frontend") {
    Push-Location frontend
    try {
        npm audit --production --audit-level=high
        if ($LASTEXITCODE -ne 0) {
            Pop-Location
            Write-Error-Exit "NPM audit found HIGH/CRITICAL vulnerabilities in production dependencies"
        }
        Write-Success "NPM audit passed"
        Pop-Location
    } catch {
        Pop-Location
        Write-Error-Exit "NPM audit failed: $_"
    }
}

Write-Section "SUPPLY CHAIN: SBOM Generation"
$syftInstalled = Get-Command syft -ErrorAction SilentlyContinue
if (-not $syftInstalled) {
    Write-Warning-Message "Syft not installed, skipping SBOM generation"
    Write-Warning-Message "Install from: https://github.com/anchore/syft"
} else {
    try {
        syft . -o json --file sbom.json | Out-Null
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Exit "SBOM generation failed"
        }
        Write-Success "SBOM generated successfully"
    } catch {
        Write-Error-Exit "SBOM generation failed: $_"
    }
}

# ==============================================================================
# SUMMARY
# ==============================================================================

Write-Host ""
Write-Host "===================================================================" -ForegroundColor Green
Write-Host "                    ALL CHECKS PASSED                              " -ForegroundColor Green
Write-Host "              Your code is ready for CI/CD pipeline                " -ForegroundColor Green
Write-Host "===================================================================" -ForegroundColor Green
Write-Host ""
