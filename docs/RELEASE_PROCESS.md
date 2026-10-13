# Release Process

This document outlines the complete release process for Home Registry, including beta pre-releases and stable releases.

## Beta Pre-Release Checklist

### Prerequisites

- [ ] All preflight checks passing locally (`scripts/preflight.ps1`)
- [ ] All CI checks passing on main branch
- [ ] Security audit complete (no HIGH/CRITICAL vulnerabilities)
- [ ] Documentation updated (README, API docs)
- [ ] CHANGELOG.md updated with changes
- [ ] All issues in milestone closed or moved

### Release Steps

#### 1. Determine Next Version

```bash
# Current: v0.1.0-beta.1
# Next:    v0.1.0-beta.2

# Or for stable release:
# From: v0.1.0-beta.N
# To:   v0.1.0
```

#### 2. Update Version (If Stable Release)

For stable releases only (no beta/rc suffix), update version in all files:

```powershell
# Windows PowerShell
.\scripts\version-bump.ps1 0.1.0

# Linux/macOS/PowerShell Core
pwsh ./scripts/version-bump.ps1 0.1.0
```

Review and commit changes:

```bash
git diff
git commit -am "chore: bump version to 0.1.0"
git push
```

**Note:** For beta releases, keep base version (0.1.0) and only increment beta number in tag.

#### 3. Update CHANGELOG.md

Add release notes:

```markdown
## [0.1.0-beta.2] - 2026-02-XX

### Added
- New feature descriptions

### Fixed
- Bug fix descriptions

### Security
- Security improvements
```

Commit:

```bash
git add CHANGELOG.md
git commit -m "docs: update changelog for v0.1.0-beta.2"
git push
```

#### 4. Trigger Release Workflow

**Option A: Manual Trigger (Recommended for Beta)**

1. Go to: [Actions → Release to GHCR](https://github.com/VictoryTek/home-registry/actions/workflows/release.yml)
2. Click "Run workflow"
3. Select branch: `main`
4. Enter version: `0.1.0-beta.2` (include identifier, no 'v' prefix)
5. Check "Create GitHub Release"
6. Click "Run workflow"

**Option B: Git Tag (Future Enhancement)**

```bash
# Tag the release
git tag v0.1.0-beta.2
git push origin v0.1.0-beta.2

# Workflow will trigger automatically
```

#### 5. Monitor Workflow

Watch the workflow execution:

- ✅ **Validate** - Checks version format and runs preflight
- ✅ **Build & Push** - Multi-arch builds for amd64 and arm64
- ✅ **Merge Manifests** - Creates multi-arch manifest
- ✅ **Security Scan** - Trivy vulnerability scanning
- ✅ **Create Release** - GitHub Release creation

**Estimated Duration:** 20-30 minutes for complete workflow

#### 6. Verify Release

After workflow completes successfully:

```bash
# Pull the image
docker pull ghcr.io/victorytek/home-registry:v0.1.0-beta.2

# Test both architectures
docker run --rm --platform linux/amd64 \
  ghcr.io/victorytek/home-registry:v0.1.0-beta.2 \
  ./home-registry --version

docker run --rm --platform linux/arm64 \
  ghcr.io/victorytek/home-registry:v0.1.0-beta.2 \
  ./home-registry --version

# Verify multi-arch manifest
docker manifest inspect ghcr.io/victorytek/home-registry:v0.1.0-beta.2
```

#### 7. Test Deployment

Deploy to a test environment:

```bash
# Create test directory
mkdir test-deployment
cd test-deployment

# Download production compose file
curl -sSL https://raw.githubusercontent.com/VictoryTek/home-registry/main/docker-compose.prod.yml -o docker-compose.yml

# Deploy with specific version
VERSION=v0.1.0-beta.2 GITHUB_REPOSITORY_OWNER=victorytek docker compose up -d

# Test the deployment
curl http://localhost:8210/health
curl http://localhost:8210/api/setup/status

# Run integration tests (if available)
# ...

# Cleanup
docker compose down -v
cd ..
rm -rf test-deployment
```

#### 8. Announce Release

Update documentation and notify users:

- [ ] Update README.md if installation instructions changed
- [ ] Post announcement in GitHub Discussions
- [ ] Update project website (if applicable)
- [ ] Notify beta testers via email/Discord/Slack
- [ ] Post on social media (optional)

## Stable Release Checklist

When transitioning from beta to stable (e.g., v0.1.0-beta.N → v0.1.0):

### Additional Prerequisites

- [ ] All known bugs fixed
- [ ] Security audit complete (third-party if possible)
- [ ] Performance benchmarks meet targets
- [ ] Documentation complete and reviewed
- [ ] Migration guide available (if breaking changes)
- [ ] Community feedback addressed
- [ ] Test coverage ≥80%
- [ ] No HIGH/CRITICAL vulnerabilities
- [ ] Backup/restore tested thoroughly
- [ ] Multi-user scenarios tested
- [ ] Load testing complete
- [ ] Upgrade path tested (beta → stable)

### Stable Release Process

1. **Update Base Version**

   ```powershell
   .\scripts\version-bump.ps1 0.1.0
   git commit -am "chore: bump version to 0.1.0"
   git push
   ```

2. **Update CHANGELOG.md**

   Move items from `[Unreleased]` to `[0.1.0]` section:

   ```markdown
   ## [0.1.0] - 2026-03-15

   Stable release based on beta.5.

   ### All Changes Since Last Stable
   ...
   ```

3. **Create Release**

   Use release workflow with version `0.1.0` (no beta suffix):
   
   - Image will be tagged with both `v0.1.0` and `latest`
   - Release will NOT be marked as pre-release

4. **Post-Release**

   - [ ] Verify `latest` tag points to new release
   - [ ] Update documentation to reference stable version
   - [ ] Archive beta releases (optional)
   - [ ] Plan next release cycle

## Emergency Rollback

If a critical bug is discovered after release:

### Option 1: Re-tag Previous Version

```bash
# Pull previous working version
docker pull ghcr.io/victorytek/home-registry:v0.1.0-beta.1

# Manually re-tag as latest beta
docker tag ghcr.io/victorytek/home-registry:v0.1.0-beta.1 \
           ghcr.io/victorytek/home-registry:beta

# Push updated tag (requires manual GHCR login)
docker push ghcr.io/victorytek/home-registry:beta
```

### Option 2: Release Hotfix

```bash
# Create hotfix from previous tag
git checkout v0.1.0-beta.2
git checkout -b hotfix/beta.3

# Apply fixes
git cherry-pick <commit-hash>
# or manually fix

# Test thoroughly
./scripts/preflight.ps1

# Commit and push
git commit -am "fix: critical bug in X"
git push origin hotfix/beta.3

# Trigger release for v0.1.0-beta.3
# ... use workflow as normal
```

## Version Numbering Strategy

### Pre-Release Versions

- **alpha**: `0.1.0-alpha.1`, `0.1.0-alpha.2`, ...
  - Internal testing
  - Unstable, breaking changes allowed
  - Not published to GHCR (optional)

- **beta**: `0.1.0-beta.1`, `0.1.0-beta.2`, ...
  - Public testing
  - Feature-complete for version
  - Breaking changes allowed with migration guide
  - Tagged as `beta` in GHCR

- **rc**: `0.1.0-rc.1`, `0.1.0-rc.2`, ...
  - Release candidate
  - Production-ready, final testing
  - No new features, only bug fixes
  - No breaking changes

### Stable Versions

- **stable**: `0.1.0`, `1.0.0`, ...
  - Production release
  - No suffix
  - Tagged as `latest` in GHCR
  - Follows semantic versioning strictly

### Semantic Versioning Rules

- **MAJOR**: Breaking changes (e.g., 1.0.0 → 2.0.0)
- **MINOR**: New features, backwards compatible (e.g., 1.0.0 → 1.1.0)
- **PATCH**: Bug fixes, backwards compatible (e.g., 1.0.0 → 1.0.1)

## Release Schedule

### Beta Phase (Current)

- **Frequency**: Weekly or as needed
- **Focus**: Bug fixes, usability improvements, new features
- **Breaking Changes**: Allowed with migration guide
- **Support**: Community support only

### Stable Phase (Future)

- **Minor Releases**: Monthly
- **Patch Releases**: As needed (security/critical bugs)
- **Major Releases**: Quarterly or biannually
- **Breaking Changes**: Only in major versions
- **Support**: 
  - Latest major: Full support
  - Previous major: Security patches for 6 months
  - Older: Community support only

## Troubleshooting

### Workflow Fails at Validation

**Issue**: Preflight checks fail

**Solution**:
1. Run preflight locally: `./scripts/preflight.ps1`
2. Fix issues locally
3. Commit fixes
4. Re-trigger workflow

### Workflow Fails at Build

**Issue**: Multi-arch build fails

**Solution**:
1. Check build logs for specific errors
2. Test locally: `docker buildx build --platform linux/amd64,linux/arm64 .`
3. Fix Dockerfile or dependencies
4. Commit and re-trigger

### Security Scan Fails

**Issue**: Trivy finds HIGH/CRITICAL vulnerabilities

**Solution**:
1. Review Trivy output in workflow logs
2. Update dependencies in Cargo.toml or package.json
3. If false positive, add to trivy exception list
4. Commit and re-trigger

### Wrong Image Tagged

**Issue**: Accidentally released wrong version

**Solution**:
1. Do NOT delete GitHub Release (breaks links)
2. Create new hotfix release with correct version
3. Update release notes to mention correction
4. Mark incorrect release as "superseded" in description

## Support

For questions or issues with the release process:

1. Check [GitHub Actions logs](https://github.com/VictoryTek/home-registry/actions)
2. Review [CONTRIBUTING.md](../CONTRIBUTING.md) (if exists)
3. Open an issue with `release` label
4. Contact maintainers via GitHub Discussions

---

**Document Version**: 1.0  
**Last Updated**: 2026-02-15
