# GHCR Beta Pre-Release Specification

**Project:** Home Registry  
**Version:** v0.1.0-beta.1  
**Created:** 2026-02-15  
**Status:** Specification Complete

---

## Executive Summary

This specification outlines the complete setup for publishing Home Registry to GitHub Container Registry (GHCR) as a beta pre-release. The implementation will enable automated multi-architecture Docker image builds, semantic versioning with pre-release tags, and streamlined deployment workflows for end users.

**Target Outcome:**
- Users can deploy Home Registry using pre-built images from GHCR
- Multi-architecture support (linux/amd64, linux/arm64)
- Automated releases with proper versioning (v0.1.0-beta.1, v0.1.0-beta.2, etc.)
- Security-hardened CI/CD pipeline with attestation and SBOM generation

---

## 1. Current State Analysis

### 1.1 Existing Infrastructure

#### ✅ What's Already in Place

**Dockerfile (`Dockerfile`):**
- Multi-stage build (frontend → backend → runtime)
- Security-hardened Alpine-based image
- Non-root user (`appuser`)
- Health checks included
- Static linking for portability
- Version label: `0.1.0`

**Docker Compose (`docker-compose.yml`):**
- PostgreSQL 17 database service
- Application service with `build: .` (local build)
- Health checks and dependencies configured
- Named volumes for persistence

**CI/CD Workflows (`.github/workflows/`):**
- `ci.yml`: Rust checks, frontend build, Docker build (no push)
- `security.yml`: CodeQL, Trivy scanning, SBOM generation
- `weekly-audit.yml`: Scheduled security audits

**Project Metadata:**
- Version: `0.1.0` (Cargo.toml, package.json, Dockerfile)
- License: MIT
- Rust MSRV: 1.88
- Repository field configured in Cargo.toml

#### ❌ What's Missing

1. **GHCR Publishing Workflow**
   - No GitHub Actions workflow for building and pushing to GHCR
   - No multi-architecture build configuration (only amd64 currently)
   - No automated release creation workflow

2. **Version Management**
   - No semantic versioning strategy for pre-releases
   - No CHANGELOG.md or release notes automation
   - Version hardcoded in multiple files (needs sync strategy)

3. **Deployment Documentation**
   - README.md only covers local builds
   - No instructions for pulling from GHCR
   - No docker-compose.yml variant for pre-built images

4. **Release Process**
   - No release checklist or runbook
   - No automated tag-based releases
   - No GitHub Release creation automation

### 1.2 Architecture Compatibility

**Current Target:** linux/amd64 (implicit)  
**Proposed Targets:** linux/amd64, linux/arm64

**Compatibility Matrix:**

| Platform | Status | Notes |
|----------|--------|-------|
| linux/amd64 | ✅ Working | Current implicit target |
| linux/arm64 | 🔨 Requires multi-arch build | Raspberry Pi, Apple Silicon |
| linux/arm/v7 | ⏸️ Future | ARMv7 (older Pi models) - high build time |

**Decision:** Start with amd64 + arm64. ARMv7 can be added later based on demand.

---

## 2. Research Findings

### 2.1 GitHub Container Registry Best Practices

**Source 1: GitHub Docs - Publishing Docker Images**
- Use `packages: write` permission for GITHUB_TOKEN
- Tag convention: `ghcr.io/OWNER/IMAGE_NAME:TAG`
- Use `docker/login-action@v3` for authentication
- Enable improved container support in repository settings

**Source 2: Docker Best Practices for CI/CD**
- Use `docker/build-push-action@v6` for builds
- Enable BuildKit cache with `cache-from` and `cache-to`
- Multi-platform builds require `docker/setup-buildx-action`
- Use `docker/metadata-action` for automatic tagging

**Source 3: GitHub Actions Security Best Practices**
- Pin actions to specific SHA commits (not tags)
- Use minimal permissions (GITHUB_TOKEN with explicit scopes)
- Enable SLSA attestation (`provenance: true`, `sbom: true`)
- Run Trivy scan before pushing to registry

**Source 4: Semantic Versioning for Pre-Releases (semver.org)**
- Pre-release format: `MAJOR.MINOR.PATCH-IDENTIFIER.NUMBER`
- Examples: `0.1.0-beta.1`, `0.1.0-beta.2`, `1.0.0-rc.1`
- Pre-release precedence: `0.1.0-alpha < 0.1.0-beta < 0.1.0-rc < 0.1.0`
- Git tags should match: `v0.1.0-beta.1`

**Source 5: Docker Multi-Architecture Builds**
- Use BuildKit's `--platform` flag
- QEMU emulation for cross-compilation via `docker/setup-qemu-action`
- Rust cross-compilation: use `cross-rs/cross` for native speed
- Alpine multi-arch base images available (node:20-alpine, rust:1-alpine)

**Source 6: Container Supply Chain Security**
- Generate and sign SBOM (Software Bill of Materials)
- Use Sigstore cosign for image signing
- Provenance attestation (SLSA Level 3)
- Scan for vulnerabilities before publishing (Trivy, Grype)
- Pin all dependencies by SHA or exact version

---

## 3. Implementation Plan

### 3.1 GitHub Actions Workflow: Release to GHCR

**File:** `.github/workflows/release.yml`

**Trigger Strategy:**
- Manual dispatch with version input (initial implementation)
- Automatic on git tag push `v*.*.*-beta.*` (future enhancement)

**Workflow Jobs:**

#### Job 1: `validate` (Pre-Checks)
- Run preflight checks (`scripts/preflight.ps1`)
- Verify version consistency across files
- Check no uncommitted changes
- Validate tag format (if triggered by tag)

#### Job 2: `build-and-push` (Multi-Arch Build)
- **Matrix Strategy:** Build separately for amd64 and arm64 (faster)
- Use `docker/setup-qemu-action@v3` for emulation
- Use `docker/setup-buildx-action@v3` for multi-platform
- Cache from GHCR to speed up builds
- Push manifests for both architectures
- Generate SBOM and provenance attestation

#### Job 3: `security-scan` (Post-Build Security)
- Pull built images (both architectures)
- Run Trivy scan (fail on HIGH/CRITICAL)
- Upload results to GitHub Security tab

#### Job 4: `create-release` (GitHub Release)
- Create GitHub Release with pre-release flag
- Generate release notes from commits since last tag
- Attach SBOM and security scan report
- Include docker-compose.yml example

**Detailed Workflow Code:**

```yaml
name: Release to GHCR

on:
  workflow_dispatch:
    inputs:
      version:
        description: 'Release version (e.g., 0.1.0-beta.1)'
        required: true
        type: string
      create_release:
        description: 'Create GitHub Release'
        required: true
        type: boolean
        default: true
  # Future: Automatic trigger on tag push
  # push:
  #   tags:
  #     - 'v*.*.*-beta.*'
  #     - 'v*.*.*-rc.*'
  #     - 'v*.*.*'

permissions:
  contents: write  # For creating releases
  packages: write  # For pushing to GHCR
  security-events: write  # For uploading Trivy results
  id-token: write  # For SLSA attestation
  attestations: write  # For GitHub attestations

env:
  REGISTRY: ghcr.io
  # IMAGE_NAME: ${{ github.repository }}  # Format: owner/repo
  IMAGE_NAME: home-registry  # Can customize if needed

concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false  # Never cancel in-flight releases

jobs:
  validate:
    name: Pre-Release Validation
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.version.outputs.version }}
      image_tag: ${{ steps.version.outputs.image_tag }}
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
        with:
          fetch-depth: 0  # Full history for release notes

      - name: Determine version
        id: version
        run: |
          if [ "${{ github.event_name }}" == "workflow_dispatch" ]; then
            VERSION="${{ inputs.version }}"
          else
            # Extract from git tag (format: v0.1.0-beta.1)
            VERSION="${GITHUB_REF#refs/tags/v}"
          fi
          echo "version=$VERSION" >> $GITHUB_OUTPUT
          echo "image_tag=v$VERSION" >> $GITHUB_OUTPUT
          echo "📦 Release version: $VERSION"

      - name: Validate version format
        run: |
          VERSION="${{ steps.version.outputs.version }}"
          if ! echo "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z]+\.[0-9]+)?$'; then
            echo "❌ Invalid version format: $VERSION"
            echo "Expected: X.Y.Z or X.Y.Z-beta.N"
            exit 1
          fi
          echo "✅ Version format valid"

      - name: Check version consistency
        run: |
          VERSION="${{ steps.version.outputs.version }}"
          CARGO_VERSION=$(grep '^version = ' Cargo.toml | head -1 | cut -d'"' -f2)
          
          # Extract base version (strip pre-release suffix)
          BASE_VERSION=$(echo "$VERSION" | cut -d'-' -f1)
          
          if [ "$CARGO_VERSION" != "$BASE_VERSION" ]; then
            echo "⚠️  Warning: Cargo.toml version ($CARGO_VERSION) differs from release ($BASE_VERSION)"
            echo "This is expected for pre-releases. Continuing..."
          fi
          
          echo "✅ Version check complete"

      - name: Run preflight checks
        run: |
          # Install dependencies for preflight
          sudo apt-get update
          sudo apt-get install -y postgresql-client
          
          # Run subset of checks (no database required)
          echo "Running static checks..."
          cargo fmt --check
          cargo clippy --all-targets --all-features -- -D warnings
          
          cd frontend
          npm ci
          npm run lint
          npm run format:check
          cd ..
          
          echo "✅ Preflight checks passed"

  build-and-push:
    name: Build & Push (${{ matrix.platform }})
    needs: validate
    runs-on: ubuntu-latest
    timeout-minutes: 60
    strategy:
      fail-fast: false
      matrix:
        platform:
          - linux/amd64
          - linux/arm64
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2

      - name: Set up QEMU
        uses: docker/setup-qemu-action@2f7da1b92e1a7fd96bc9b7f1b1bcbdf0f7fe2a23 # v3.3.0

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@b5ca514318bd6ebac0fb2aedd5d36ec1b5c232a2 # v3.10.0

      - name: Log in to GitHub Container Registry
        uses: docker/login-action@7ca345011ac4304463197321049ff8fd128ff2df # v3.4.0
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@70b2cdc6480c1a8b86edf1777157f8f437de2166 # v5.7.0
        with:
          images: ${{ env.REGISTRY }}/${{ github.repository_owner }}/${{ env.IMAGE_NAME }}
          tags: |
            type=raw,value=${{ needs.validate.outputs.image_tag }}
            type=raw,value=latest,enable=${{ !contains(needs.validate.outputs.version, '-') }}
            type=raw,value=beta,enable=${{ contains(needs.validate.outputs.version, '-beta') }}
          flavor: |
            latest=false

      - name: Build and push by digest
        id: build
        uses: docker/build-push-action@263435318d21b8e681c14492fe198d362a7d2c83 # v6.18.0
        with:
          context: .
          platforms: ${{ matrix.platform }}
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha,scope=build-${{ matrix.platform }}
          cache-to: type=gha,mode=max,scope=build-${{ matrix.platform }}
          provenance: true
          sbom: true

      - name: Export digest
        run: |
          mkdir -p /tmp/digests
          digest="${{ steps.build.outputs.digest }}"
          touch "/tmp/digests/${digest#sha256:}"

      - name: Upload digest
        uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2
        with:
          name: digests-${{ matrix.platform }}
          path: /tmp/digests/*
          retention-days: 1

  merge-manifests:
    name: Merge Multi-Arch Manifests
    needs: [validate, build-and-push]
    runs-on: ubuntu-latest
    steps:
      - name: Download digests
        uses: actions/download-artifact@fa0a91b85d4f404e444e00e005971372dc801d16 # v4.1.8
        with:
          path: /tmp/digests
          pattern: digests-*
          merge-multiple: true

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@b5ca514318bd6ebac0fb2aedd5d36ec1b5c232a2 # v3.10.0

      - name: Log in to GitHub Container Registry
        uses: docker/login-action@7ca345011ac4304463197321049ff8fd128ff2df # v3.4.0
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Create manifest list and push
        working-directory: /tmp/digests
        run: |
          IMAGE_BASE="${{ env.REGISTRY }}/${{ github.repository_owner }}/${{ env.IMAGE_NAME }}"
          VERSION="${{ needs.validate.outputs.image_tag }}"
          
          docker buildx imagetools create \
            --tag "${IMAGE_BASE}:${VERSION}" \
            $(printf "${IMAGE_BASE}@sha256:%s " *)
          
          # Tag as beta if pre-release
          if [[ "${{ needs.validate.outputs.version }}" == *"-beta"* ]]; then
            docker buildx imagetools create \
              --tag "${IMAGE_BASE}:beta" \
              $(printf "${IMAGE_BASE}@sha256:%s " *)
          fi
          
          # Tag as latest only for stable releases
          if [[ "${{ needs.validate.outputs.version }}" != *"-"* ]]; then
            docker buildx imagetools create \
              --tag "${IMAGE_BASE}:latest" \
              $(printf "${IMAGE_BASE}@sha256:%s " *)
          fi

      - name: Inspect image
        run: |
          IMAGE="${{ env.REGISTRY }}/${{ github.repository_owner }}/${{ env.IMAGE_NAME }}:${{ needs.validate.outputs.image_tag }}"
          docker buildx imagetools inspect "$IMAGE"

  security-scan:
    name: Security Scan
    needs: [validate, merge-manifests]
    runs-on: ubuntu-latest
    strategy:
      matrix:
        platform: [amd64, arm64]
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2

      - name: Set up Docker
        uses: docker/setup-buildx-action@b5ca514318bd6ebac0fb2aedd5d36ec1b5c232a2 # v3.10.0

      - name: Log in to GitHub Container Registry
        uses: docker/login-action@7ca345011ac4304463197321049ff8fd128ff2df # v3.4.0
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Pull image for scanning
        run: |
          IMAGE="${{ env.REGISTRY }}/${{ github.repository_owner }}/${{ env.IMAGE_NAME }}:${{ needs.validate.outputs.image_tag }}"
          docker pull --platform linux/${{ matrix.platform }} "$IMAGE"

      - name: Run Trivy vulnerability scanner
        uses: aquasecurity/trivy-action@76071ef0d7ec797419534a183b498b4d6366cf37 # 0.31.0
        with:
          image-ref: ${{ env.REGISTRY }}/${{ github.repository_owner }}/${{ env.IMAGE_NAME }}:${{ needs.validate.outputs.image_tag }}
          format: sarif
          output: trivy-results-${{ matrix.platform }}.sarif
          severity: CRITICAL,HIGH
          exit-code: '1'  # Fail on vulnerabilities

      - name: Upload Trivy results
        uses: github/codeql-action/upload-sarif@ff0a06e83cb2de871e5a09832bc6a81e7276941f # v3.28.18
        if: always()
        with:
          sarif_file: trivy-results-${{ matrix.platform }}.sarif
          category: trivy-${{ matrix.platform }}

  create-release:
    name: Create GitHub Release
    needs: [validate, security-scan]
    runs-on: ubuntu-latest
    if: ${{ inputs.create_release == true || github.event_name == 'push' }}
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
        with:
          fetch-depth: 0  # Full history for release notes

      - name: Generate release notes
        id: notes
        run: |
          VERSION="${{ needs.validate.outputs.version }}"
          PREV_TAG=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "")
          
          if [ -z "$PREV_TAG" ]; then
            COMMITS=$(git log --pretty=format:"- %s (%h)" HEAD)
          else
            COMMITS=$(git log --pretty=format:"- %s (%h)" ${PREV_TAG}..HEAD)
          fi
          
          cat > release-notes.md <<EOF
          ## Home Registry v${VERSION}
          
          **⚠️ Beta Pre-Release**: This is a pre-release version for testing purposes.
          
          ### 📦 Installation
          
          \`\`\`bash
          # Pull and run with Docker Compose
          curl -sSL https://raw.githubusercontent.com/${{ github.repository }}/main/docker-compose.prod.yml -o docker-compose.yml
          docker compose up -d
          \`\`\`
          
          Or use the image directly:
          \`\`\`bash
          docker pull ${{ env.REGISTRY }}/${{ github.repository_owner }}/${{ env.IMAGE_NAME }}:${{ needs.validate.outputs.image_tag }}
          \`\`\`
          
          ### 🏗️ Multi-Architecture Support
          - ✅ linux/amd64 (x86_64)
          - ✅ linux/arm64 (Raspberry Pi 4/5, Apple Silicon)
          
          ### 📝 Changes
          ${COMMITS}
          
          ### 🔒 Security
          - All images scanned with Trivy (no HIGH/CRITICAL vulnerabilities)
          - SBOM and provenance attestation included
          - Supply chain security verified
          
          ### 🐛 Known Issues
          - First-time setup requires manual admin account creation
          - Recovery codes must be saved immediately (not retrievable later)
          
          ### 📚 Documentation
          - [Installation Guide](https://github.com/${{ github.repository }}#quick-start)
          - [Configuration Reference](https://github.com/${{ github.repository }}#environment-variables)
          - [Security Best Practices](https://github.com/${{ github.repository }}#security-considerations)
          EOF
          
          cat release-notes.md

      - name: Create docker-compose.prod.yml
        run: |
          cat > docker-compose.prod.yml <<'EOF'
          services:
            db:
              image: postgres:17
              environment:
                POSTGRES_USER: postgres
                POSTGRES_PASSWORD: password  # ⚠️ Change in production!
                POSTGRES_DB: home_inventory
              ports:
                - "5432:5432"
              volumes:
                - pgdata:/var/lib/postgresql/data
              healthcheck:
                test: ["CMD-SHELL", "pg_isready -U postgres"]
                interval: 5s
                timeout: 5s
                retries: 5
              restart: unless-stopped

            app:
              image: ${{ env.REGISTRY }}/${{ github.repository_owner }}/${{ env.IMAGE_NAME }}:${{ needs.validate.outputs.image_tag }}
              depends_on:
                db:
                  condition: service_healthy
              environment:
                DATABASE_URL: postgres://postgres:password@db:5432/home_inventory
                PORT: 8210
                RUST_LOG: info
                # JWT_SECRET: "your-secure-secret-here"  # Auto-generated if not set
                # JWT_TOKEN_LIFETIME_HOURS: 24
                RATE_LIMIT_RPS: 100
                RATE_LIMIT_BURST: 200
              ports:
                - "8210:8210"
              volumes:
                - appdata:/app/data
                - backups:/app/backups
              command: ["./home-registry"]
              restart: unless-stopped

          volumes:
            pgdata:
            appdata:
            backups:
          EOF

      - name: Create GitHub Release
        uses: softprops/action-gh-release@da05a1223c0ebad7fb81e472f6650cc0f27efed0 # v2.2.2
        with:
          tag_name: v${{ needs.validate.outputs.version }}
          name: Home Registry v${{ needs.validate.outputs.version }}
          body_path: release-notes.md
          draft: false
          prerelease: ${{ contains(needs.validate.outputs.version, '-') }}
          files: |
            docker-compose.prod.yml
          token: ${{ secrets.GITHUB_TOKEN }}
          generate_release_notes: false

      - name: Announce release
        run: |
          echo "🎉 Release v${{ needs.validate.outputs.version }} published!"
          echo "📦 Image: ${{ env.REGISTRY }}/${{ github.repository_owner }}/${{ env.IMAGE_NAME }}:${{ needs.validate.outputs.image_tag }}"
          echo "🔗 Release URL: https://github.com/${{ github.repository }}/releases/tag/v${{ needs.validate.outputs.version }}"
```

### 3.2 Docker Compose Modifications

**Create New File:** `docker-compose.prod.yml`

This file will:
- Use `image:` instead of `build:`
- Pull from GHCR
- Reference specific version tag
- Include example for environment variable overrides

**File Content:**

```yaml
# docker-compose.prod.yml
# Production deployment using pre-built image from GHCR

services:
  db:
    image: postgres:17
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:-password}  # Override via env var
      POSTGRES_DB: home_inventory
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5
    restart: unless-stopped

  app:
    image: ghcr.io/${GITHUB_REPOSITORY_OWNER:-yourusername}/home-registry:${VERSION:-beta}
    pull_policy: always  # Always check for updates
    depends_on:
      db:
        condition: service_healthy
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD:-password}@db:5432/home_inventory
      PORT: 8210
      RUST_LOG: ${RUST_LOG:-info}
      # JWT_SECRET: "${JWT_SECRET}"  # Optional - auto-generated if not set
      # JWT_TOKEN_LIFETIME_HOURS: 24
      RATE_LIMIT_RPS: 100
      RATE_LIMIT_BURST: 200
    ports:
      - "8210:8210"
    volumes:
      - appdata:/app/data
      - backups:/app/backups
    command: ["./home-registry"]
    restart: unless-stopped
    healthcheck:
      test: ["CMD-SHELL", "curl -f http://localhost:8210/health || exit 1"]
      interval: 30s
      timeout: 10s
      start_period: 10s
      retries: 3

volumes:
  pgdata:
  appdata:
  backups:
```

**Usage:**

```bash
# Use default beta tag
docker compose -f docker-compose.prod.yml up -d

# Use specific version
VERSION=v0.1.0-beta.1 docker compose -f docker-compose.prod.yml up -d

# Override owner (for forks)
GITHUB_REPOSITORY_OWNER=myusername VERSION=v0.1.0-beta.1 docker compose -f docker-compose.prod.yml up -d
```

**Keep Existing `docker-compose.yml`:**
- Rename to `docker-compose.dev.yml` (optional)
- Keep for local development builds
- Document distinction in README

### 3.3 README.md Updates

**Sections to Add/Modify:**

#### A. Quick Start Section (Update)

```markdown
## Quick Start

### Option 1: Pre-Built Image (Recommended for Production)

```bash
# Download production compose file
curl -sSL https://raw.githubusercontent.com/yourusername/home-registry/main/docker-compose.prod.yml -o docker-compose.yml

# Start services
docker compose up -d

# Access at http://localhost:8210
```

### Option 2: Local Build (Development)

```bash
# Clone repository
git clone https://github.com/yourusername/home-registry.git
cd home-registry

# Start with local build
docker compose -f docker-compose.dev.yml up -d
```

### Multi-Architecture Support

Pre-built images support:
- ✅ **linux/amd64** (Intel/AMD x86_64)
- ✅ **linux/arm64** (Raspberry Pi 4/5, Apple Silicon, AWS Graviton)

Docker automatically pulls the correct architecture for your system.
```

#### B. Installation Section (New)

```markdown
## Installation

### Pull from GitHub Container Registry

```bash
# Latest beta
docker pull ghcr.io/yourusername/home-registry:beta

# Specific version
docker pull ghcr.io/yourusername/home-registry:v0.1.0-beta.1

# Latest stable (when available)
docker pull ghcr.io/yourusername/home-registry:latest
```

### Available Tags

| Tag | Description | Stability |
|-----|-------------|-----------|
| `beta` | Latest beta pre-release | Testing |
| `v0.1.0-beta.1` | Specific beta version | Fixed |
| `latest` | Latest stable release | Production |
| `v1.0.0` | Specific stable version | Fixed |

### Verification

Verify image authenticity:

```bash
# Check SBOM (Software Bill of Materials)
docker sbom ghcr.io/yourusername/home-registry:v0.1.0-beta.1

# Inspect image metadata
docker inspect ghcr.io/yourusername/home-registry:v0.1.0-beta.1
```
```

#### C. Update Development Section

```markdown
## Development

### Local Development (No Docker)

For fastest iteration during development:

**Prerequisites:**
- Rust 1.85+
- Node.js 20+
- PostgreSQL 17+

[... existing content ...]

### Local Docker Build

For testing Docker builds before release:

```bash
# Build and run locally
docker compose -f docker-compose.dev.yml up -d

# Or build manually
docker build -t home-registry:local .
docker run -d -p 8210:8210 \
  -e DATABASE_URL=postgres://postgres:password@db:5432/home_inventory \
  home-registry:local
```
```

### 3.4 Versioning Strategy

#### Semantic Versioning Schema

**Format:** `MAJOR.MINOR.PATCH[-IDENTIFIER.NUMBER]`

**Examples:**
- `0.1.0-beta.1` - First beta release
- `0.1.0-beta.2` - Second beta with fixes
- `0.1.0-rc.1` - Release candidate
- `0.1.0` - First stable release
- `1.0.0` - Major stable release

**Pre-Release Hierarchy:**
1. `alpha` - Internal testing, unstable
2. `beta` - Public testing, feature-complete
3. `rc` (release candidate) - Production-ready, final testing
4. `stable` - No suffix, production release

#### Version Management

**Files Requiring Version Updates:**
- `Cargo.toml` (line 3): `version = "0.1.0"`
- `frontend/package.json` (line 4): `"version": "0.1.0"`
- `Dockerfile` (line 99): `LABEL org.opencontainers.image.version="0.1.0"`
- Git tag: `v0.1.0-beta.1`

**Update Process:**

1. **For Beta Releases:** Keep base version stable, increment beta number
   - Files stay at `0.1.0`
   - Tag increments: `v0.1.0-beta.1` → `v0.1.0-beta.2`

2. **For Stable Releases:** Update all files + tag
   - Update Cargo.toml, package.json, Dockerfile
   - Tag: `v0.1.0` (no suffix)

3. **Automation Script** (create `scripts/bump-version.sh`):

```bash
#!/bin/bash
# scripts/bump-version.sh - Bump version across all files

set -e

if [ $# -ne 1 ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 0.2.0"
  exit 1
fi

NEW_VERSION="$1"

# Validate format
if ! echo "$NEW_VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "Error: Invalid version format. Expected: X.Y.Z"
  exit 1
fi

echo "Bumping version to $NEW_VERSION..."

# Update Cargo.toml
sed -i "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml

# Update frontend/package.json
sed -i "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" frontend/package.json

# Update Dockerfile
sed -i "s/org.opencontainers.image.version=\".*\"/org.opencontainers.image.version=\"$NEW_VERSION\"/" Dockerfile

echo "✅ Version bumped to $NEW_VERSION in all files"
echo "Next steps:"
echo "  1. Review changes: git diff"
echo "  2. Commit: git commit -am 'chore: bump version to $NEW_VERSION'"
echo "  3. Tag: git tag v$NEW_VERSION-beta.1"
echo "  4. Push: git push && git push --tags"
```

### 3.5 Security Considerations

#### Token Permissions

**GITHUB_TOKEN Permissions Required:**
```yaml
permissions:
  contents: write        # Create releases
  packages: write        # Push to GHCR
  security-events: write # Upload security scans
  id-token: write        # SLSA attestation
  attestations: write    # GitHub attestations
```

**Why These Permissions:**
- `packages: write` - Push images to ghcr.io
- `contents: write` - Create GitHub Releases and tags
- `security-events: write` - Upload Trivy scan results to Security tab
- `id-token: write` - Generate SLSA provenance attestation
- `attestations: write` - Attach SBOM and other attestations

**Security Best Practices:**
1. ✅ Use `secrets.GITHUB_TOKEN` (automatic, scoped to repository)
2. ✅ Never use Personal Access Token (PAT) unless required
3. ✅ Pin all GitHub Actions to specific SHA commits
4. ✅ Use minimal permissions (principle of least privilege)
5. ✅ Enable "Improved container support" in repository settings

#### Image Signing (Future Enhancement)

**For Production Releases (v1.0.0+):**
- Use Sigstore Cosign for image signing
- Store signatures in GHCR alongside images
- Users verify with: `cosign verify ghcr.io/owner/image:tag`

**Example addition to workflow:**
```yaml
- name: Install Cosign
  uses: sigstore/cosign-installer@v3

- name: Sign image
  run: |
    cosign sign --yes \
      ghcr.io/${{ github.repository_owner }}/home-registry:${{ needs.validate.outputs.image_tag }}
```

### 3.6 Release Process Documentation

**Create:** `.github/docs/RELEASE_PROCESS.md`

```markdown
# Release Process

## Beta Pre-Release Checklist

### Prerequisites
- [ ] All preflight checks passing locally
- [ ] All CI checks passing on main branch
- [ ] Security audit complete (no HIGH/CRITICAL vulnerabilities)
- [ ] Documentation updated (README, API docs)
- [ ] CHANGELOG.md updated with changes

### Steps

1. **Determine Next Version**
   ```bash
   # Current: v0.1.0-beta.1
   # Next:    v0.1.0-beta.2
   
   # Or for stable release:
   # From: v0.1.0-beta.N
   # To:   v0.1.0
   ```

2. **Update Version (If Stable Release)**
   ```bash
   # Only for stable releases (no beta/rc suffix)
   ./scripts/bump-version.sh 0.1.0
   git commit -am "chore: bump version to 0.1.0"
   git push
   ```

3. **Trigger Release Workflow**
   - Go to: Actions → Release to GHCR → Run workflow
   - Select branch: `main`
   - Enter version: `0.1.0-beta.2` (include identifier)
   - Check "Create GitHub Release"
   - Click "Run workflow"

4. **Monitor Workflow**
   - Watch for validation errors
   - Check security scan results
   - Verify multi-arch build completes

5. **Verify Release**
   ```bash
   # Pull and test both architectures
   docker pull ghcr.io/yourusername/home-registry:v0.1.0-beta.2
   
   # Test on amd64
   docker run --rm --platform linux/amd64 \
     ghcr.io/yourusername/home-registry:v0.1.0-beta.2 \
     ./home-registry --help
   
   # Test on arm64 (if available)
   docker run --rm --platform linux/arm64 \
     ghcr.io/yourusername/home-registry:v0.1.0-beta.2 \
     ./home-registry --help
   ```

6. **Update Documentation**
   - Update README with latest version
   - Add release announcement (if significant)
   - Notify users (GitHub Discussions, Discord, etc.)

7. **Post-Release Testing**
   - Deploy to staging environment
   - Run integration tests
   - Verify database migrations
   - Test backup/restore functionality

### Emergency Rollback

If critical bug discovered:

```bash
# Option 1: Re-tag previous version as latest
docker pull ghcr.io/yourusername/home-registry:v0.1.0-beta.1
docker tag ghcr.io/yourusername/home-registry:v0.1.0-beta.1 \
           ghcr.io/yourusername/home-registry:beta
docker push ghcr.io/yourusername/home-registry:beta

# Option 2: Release hotfix
./scripts/bump-version.sh 0.1.0
# Fix bug, commit
git tag v0.1.0-beta.3
# Trigger release workflow
```

## Stable Release Checklist

When transitioning from beta to stable (e.g., v0.1.0-beta.N → v0.1.0):

- [ ] All known bugs fixed
- [ ] Security audit complete
- [ ] Performance benchmarks meet targets
- [ ] Documentation complete
- [ ] Migration guide available (if needed)
- [ ] Community feedback addressed
- [ ] Test coverage ≥80%
- [ ] No HIGH/CRITICAL vulnerabilities
- [ ] Backup/restore tested
- [ ] Multi-user scenarios tested
- [ ] Load testing complete
- [ ] Update base version in all files (bump-version.sh)
- [ ] Create stable release (no beta suffix in workflow)
- [ ] Announce on all channels

## Release Schedule

**Beta Phase (v0.1.0-beta.1 → v0.1.0):**
- Release frequency: Weekly or as needed
- Focus: Bug fixes, usability improvements
- Breaking changes: Allowed with migration guide

**Stable Phase (v0.1.0+):**
- Release frequency: Monthly for minor, quarterly for patch
- Focus: Stability, security, performance
- Breaking changes: Only in major versions
```

### 3.7 CHANGELOG.md Template

**Create:** `CHANGELOG.md`

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Multi-architecture Docker images (amd64, arm64)
- Automated GHCR publishing workflow
- Security attestation (SBOM, provenance)

### Changed
- Split docker-compose.yml into dev and prod variants

### Fixed
- (List bug fixes here)

### Security
- Added Trivy vulnerability scanning in release pipeline
- Enabled SLSA provenance attestation

## [0.1.0-beta.1] - 2026-02-15

### Added
- Initial beta release
- User authentication with JWT
- Inventory management (CRUD operations)
- Categories, tags, and custom fields
- Multi-user support with sharing
- Organizers (locations/containers)
- Backup/restore functionality
- Progressive Web App (PWA) support
- Mobile-responsive design

### Security
- Argon2id password hashing
- Rate limiting (100 RPS)
- Non-root container user
- Static-linked Alpine image
- HTTPS-ready deployment

### Known Issues
- Recovery codes not retrievable after initial setup
- No email notification system yet
- Limited search filters

---

## Release Links

- [0.1.0-beta.1](https://github.com/yourusername/home-registry/releases/tag/v0.1.0-beta.1)

[Unreleased]: https://github.com/yourusername/home-registry/compare/v0.1.0-beta.1...HEAD
[0.1.0-beta.1]: https://github.com/yourusername/home-registry/releases/tag/v0.1.0-beta.1
```

---

## 4. Testing Recommendations Before Release

### 4.1 Pre-Release Testing Checklist

**Local Build Testing:**
```bash
# Test multi-arch build locally
docker buildx build --platform linux/amd64,linux/arm64 -t home-registry:test .

# Test amd64 image
docker run --rm --platform linux/amd64 home-registry:test ./home-registry --help

# Test arm64 image (requires QEMU or ARM machine)
docker run --rm --platform linux/arm64 home-registry:test ./home-registry --help
```

**Workflow Testing:**
```bash
# Test workflow locally with act (GitHub Actions simulator)
gh act workflow_dispatch -W .github/workflows/release.yml \
  -s GITHUB_TOKEN=$GITHUB_TOKEN \
  --input version=0.1.0-beta.1 \
  --input create_release=false
```

**Security Testing:**
```bash
# Run Trivy scan locally
trivy image home-registry:test --severity HIGH,CRITICAL

# Check for secrets in image
trivy image home-registry:test --scanners secret

# Verify SBOM generation
docker sbom home-registry:test
```

**Integration Testing:**
```bash
# Test complete deployment with prod compose file
VERSION=v0.1.0-beta.1 docker compose -f docker-compose.prod.yml up -d

# Run health checks
curl http://localhost:8210/health

# Test database migrations
docker compose -f docker-compose.prod.yml logs app | grep "migration"

# Test backup/restore
curl -X POST http://localhost:8210/api/backup \
  -H "Authorization: Bearer $TOKEN" \
  -o backup.sql

# Cleanup
docker compose -f docker-compose.prod.yml down -v
```

### 4.2 Post-Release Verification

**After first beta release:**

1. **Pull Image from GHCR:**
   ```bash
   docker pull ghcr.io/yourusername/home-registry:v0.1.0-beta.1
   ```

2. **Verify Multi-Arch:**
   ```bash
   docker manifest inspect ghcr.io/yourusername/home-registry:v0.1.0-beta.1
   # Should show linux/amd64 and linux/arm64 manifests
   ```

3. **Test Deployment:**
   ```bash
   # Use production compose file
   curl -sSL https://raw.githubusercontent.com/yourusername/home-registry/main/docker-compose.prod.yml -o compose-test.yml
   VERSION=v0.1.0-beta.1 docker compose -f compose-test.yml up -d
   ```

4. **Check Security Tab:**
   - Go to repository → Security → Dependabot alerts
   - Verify Trivy scan results uploaded
   - Review SBOM in Insights → Dependency graph

5. **Test Recovery:**
   ```bash
   # Simulate container restart
   docker compose -f compose-test.yml restart app
   
   # Verify data persistence
   curl http://localhost:8210/api/inventories -H "Authorization: Bearer $TOKEN"
   ```

---

## 5. Migration Path for Existing Users

### 5.1 From Local Build to GHCR Image

**Current Setup (Local Build):**
```yaml
services:
  app:
    build: .
```

**New Setup (GHCR Image):**
```yaml
services:
  app:
    image: ghcr.io/yourusername/home-registry:beta
```

**Migration Steps:**

1. **Backup Data:**
   ```bash
   # Export current data
   docker exec home-registry-app-1 pg_dump $DATABASE_URL > backup.sql
   
   # Or use built-in backup endpoint
   curl -X POST http://localhost:8210/api/backup \
     -H "Authorization: Bearer $TOKEN" \
     -o backup-$(date +%Y%m%d).sql
   ```

2. **Stop Current Containers:**
   ```bash
   docker compose down
   # Keep volumes: do NOT use -v flag
   ```

3. **Update Compose File:**
   ```bash
   # Download new prod compose file
   mv docker-compose.yml docker-compose.yml.backup
   curl -sSL https://raw.githubusercontent.com/yourusername/home-registry/main/docker-compose.prod.yml -o docker-compose.yml
   
   # Or manually update image: field
   ```

4. **Start with GHCR Image:**
   ```bash
   docker compose up -d
   ```

5. **Verify Data Intact:**
   ```bash
   docker compose logs app
   curl http://localhost:8210/health
   # Login and check inventories
   ```

**Note:** Named volumes persist between compose configurations, so data is retained.

---

## 6. Risks and Mitigations

### 6.1 Identified Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Multi-arch build fails | Medium | High | Test locally first; fallback to amd64-only |
| Security scan finds vulnerabilities | Medium | High | Fail workflow on HIGH/CRITICAL; fix before release |
| Image pulls slow on arm64 | Low | Medium | Optimize Dockerfile; use smaller base images |
| Version inconsistency | Medium | Medium | Automated version bump script; CI validation |
| GITHUB_TOKEN permission denied | Low | High | Document required permissions; test in staging repo |
| Migration breaks existing deployments | Low | Critical | Provide clear migration guide; test upgrade path |

### 6.2 Mitigation Strategies

**Build Failures:**
- Implement retry logic in workflow
- Test multi-arch build in CI before release workflow
- Fallback to amd64-only if arm64 fails (non-blocking)

**Security Vulnerabilities:**
- Run security scans before pushing to GHCR
- Fail workflow on HIGH/CRITICAL (except known false positives)
- Maintain vulnerability exceptions file (trivy.yaml)

**Performance Issues:**
- Monitor build times; optimize if >30min
- Use GitHub Actions cache aggressively
- Consider self-hosted runners for faster builds

**User Migration:**
- Provide step-by-step migration guide
- Test upgrade path with real data
- Offer rollback instructions

---

## 7. Future Enhancements

### 7.1 Post-Beta Improvements

**Automated Releases on Tag Push:**
```yaml
on:
  push:
    tags:
      - 'v*.*.*-beta.*'
      - 'v*.*.*-rc.*'
      - 'v*.*.*'
```

**Image Signing with Cosign:**
- Sign all production images
- Publish signatures to GHCR
- Document verification process for users

**Release Notes Automation:**
- Use `github/release-drafter` action
- Auto-generate from PR titles
- Categorize by labels (feature, bugfix, security)

**Automatic Version Bumping:**
- Use `semantic-release` tool
- Analyze commits for version bumps
- Update all files automatically

**Multi-Registry Publishing:**
- Publish to Docker Hub as fallback
- Use `docker.io/username/home-registry` as alias
- Sync tags between registries

**ARM/v7 Support:**
- Add linux/arm/v7 for older Raspberry Pi models
- Requires additional build time (~45min)
- Make optional (only on stable releases)

### 7.2 Monitoring and Analytics

**GHCR Analytics:**
- Track download counts per version
- Monitor which architectures used most
- Identify popular release channels (beta vs stable)

**GitHub Releases:**
- Track release page views
- Monitor issue reports per release
- Correlate bugs with specific versions

**Security Posture:**
- Weekly Trivy scans of published images
- Automated vulnerability patching
- Notify users of security updates

---

## 8. Implementation Summary

### 8.1 Files to Create

| File | Purpose | Priority |
|------|---------|----------|
| `.github/workflows/release.yml` | GHCR publishing workflow | **Critical** |
| `docker-compose.prod.yml` | Production deployment config | **Critical** |
| `scripts/bump-version.sh` | Version management script | High |
| `.github/docs/RELEASE_PROCESS.md` | Release documentation | High |
| `CHANGELOG.md` | Version history tracking | High |

### 8.2 Files to Modify

| File | Changes | Priority |
|------|---------|----------|
| `README.md` | Add GHCR installation instructions | **Critical** |
| `docker-compose.yml` | Rename to `docker-compose.dev.yml` | Medium |
| `Cargo.toml` | Update repository URL (if needed) | Low |

### 8.3 Configuration Changes

**Repository Settings:**
1. Enable "Improved container support" (Settings → Code and automation → Packages)
2. Make packages public (Settings → Packages → home-registry → Danger Zone → Change visibility)
3. Link package to repository (Settings → Packages → home-registry → Add repository)

**Branch Protection:**
- Require status checks before merge (CI, security scans)
- Require up-to-date branches
- Enforce signed commits (optional but recommended)

### 8.4 Timeline Estimate

| Phase | Tasks | Estimated Time |
|-------|-------|----------------|
| **Phase 1: Setup** | Create workflow, compose files, scripts | 4-6 hours |
| **Phase 2: Testing** | Local testing, workflow validation | 2-3 hours |
| **Phase 3: Documentation** | README updates, release guide | 2-3 hours |
| **Phase 4: First Release** | Trigger workflow, verify, test deployment | 1-2 hours |
| **Phase 5: Iteration** | Bug fixes, refinements based on feedback | Ongoing |

**Total Initial Setup:** 9-14 hours

---

## 9. Success Criteria

### 9.1 Definition of Done

- [ ] Workflow successfully builds and pushes multi-arch images to GHCR
- [ ] Both amd64 and arm64 images pass security scans
- [ ] GitHub Release created with proper pre-release flag
- [ ] docker-compose.prod.yml pulls and runs GHCR image successfully
- [ ] README.md updated with clear installation instructions
- [ ] Release process documented
- [ ] First beta release (v0.1.0-beta.1) published
- [ ] Verified deployment on fresh system using GHCR image
- [ ] No HIGH/CRITICAL vulnerabilities in images
- [ ] SBOM and provenance attestation attached to images

### 9.2 Metrics for Success

**Technical Metrics:**
- Build time <20 minutes (per architecture)
- Image size <150MB (compressed)
- Zero HIGH/CRITICAL vulnerabilities
- All CI checks passing

**User Experience Metrics:**
- Installation time <5 minutes (from download to running)
- Clear error messages if issues occur
- Documentation covers 90% of common questions

**Adoption Metrics:**
- 10+ successful deployments within first week
- <5% rollback rate
- Positive feedback from beta testers

---

## 10. Appendices

### Appendix A: Reference Documentation

**GitHub Documentation:**
- [Publishing Docker images](https://docs.github.com/en/actions/publishing-packages/publishing-docker-images)
- [Working with the Container registry](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)
- [Managing GitHub Actions permissions](https://docs.github.com/en/actions/security-guides/automatic-token-authentication)

**Docker Documentation:**
- [Multi-platform images](https://docs.docker.com/build/building/multi-platform/)
- [Dockerfile best practices](https://docs.docker.com/develop/dev-best-practices/)
- [Docker Compose specification](https://docs.docker.com/compose/compose-file/)

**Semantic Versioning:**
- [Semantic Versioning 2.0.0](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/)

**Security:**
- [Trivy documentation](https://aquasecurity.github.io/trivy/)
- [Sigstore Cosign](https://docs.sigstore.dev/cosign/overview/)
- [SLSA Framework](https://slsa.dev/)

### Appendix B: Troubleshooting Guide

**Common Issues:**

1. **Build fails on arm64:**
   - Check QEMU installation
   - Verify Alpine base images support arm64
   - Review Rust compilation flags

2. **Permission denied pushing to GHCR:**
   - Verify `packages: write` permission in workflow
   - Check organization package visibility settings
   - Ensure repository is linked to package

3. **Image pull fails for users:**
   - Verify package is public
   - Check image name format (ghcr.io/owner/name:tag)
   - Confirm tag exists in GHCR

4. **Security scan fails:**
   - Review Trivy output for specific CVEs
   - Check if vulnerabilities are in base image
   - Consider exception list for false positives

5. **Workflow timeout:**
   - Increase timeout value (default: 60 minutes)
   - Optimize build cache usage
   - Consider splitting architectures into separate workflows

### Appendix C: Example Commands

**Testing Multi-Arch Locally:**
```bash
# Create builder
docker buildx create --name multiarch --driver docker-container --bootstrap

# Build for multiple platforms
docker buildx build --platform linux/amd64,linux/arm64 \
  --tag home-registry:multiarch \
  --load .  # Note: --load only works with single platform

# Build and push to registry (for testing)
docker buildx build --platform linux/amd64,linux/arm64 \
  --tag localhost:5000/home-registry:test \
  --push .
```

**Manual Release Process (Without Workflow):**
```bash
# 1. Tag release
git tag v0.1.0-beta.1
git push origin v0.1.0-beta.1

# 2. Build and push manually
docker buildx build --platform linux/amd64,linux/arm64 \
  --tag ghcr.io/yourusername/home-registry:v0.1.0-beta.1 \
  --tag ghcr.io/yourusername/home-registry:beta \
  --push .

# 3. Create release
gh release create v0.1.0-beta.1 \
  --title "Home Registry v0.1.0-beta.1" \
  --notes-file release-notes.md \
  --prerelease \
  docker-compose.prod.yml
```

**Rollback to Previous Version:**
```bash
# Pull previous version
docker pull ghcr.io/yourusername/home-registry:v0.1.0-beta.1

# Update compose file
VERSION=v0.1.0-beta.1 docker compose up -d

# Or manually update image tag in docker-compose.yml
```

---

## Conclusion

This specification provides a complete roadmap for publishing Home Registry to GitHub Container Registry as a beta pre-release. The implementation focuses on:

1. **Security** - Multi-layer scanning, attestation, and best practices
2. **Portability** - Multi-architecture support for wide compatibility
3. **Automation** - CI/CD workflow for consistent releases
4. **User Experience** - Simple one-command deployment
5. **Maintainability** - Clear versioning and release process

**Next Steps:**
1. Review and approve this specification
2. Implement Phase 1 (workflow and compose files)
3. Test locally and in staging environment
4. Execute first beta release (v0.1.0-beta.1)
5. Gather feedback and iterate

**Contact:** For questions or clarifications, open an issue or discussion in the repository.

---

*Specification version: 1.0*  
*Last updated: 2026-02-15*
