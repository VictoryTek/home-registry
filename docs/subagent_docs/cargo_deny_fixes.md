# Cargo-Deny Fixes Specification

**Document Created:** February 13, 2026  
**Author:** Research Subagent  
**Status:** Ready for Implementation

---

## Executive Summary

This specification addresses four critical issues identified in the Home Registry project's cargo-deny checks:

1. **License Rejection**: `actix-governor v0.8.0` has GPL-3.0-or-later license (explicitly denied by project policy)
2. **Security Vulnerability**: `time v0.3.41` (RUSTSEC-2026-0009) - DoS via stack exhaustion
3. **Yanked Crate**: `slab v0.4.10` - needs update to v0.4.12
4. **Configuration Warnings**: Unused license allowances and advisory ignores in deny.toml

---

## Current State Analysis

### Dependency Tree Analysis

#### Issue 1: actix-governor v0.8.0 (GPL-3.0-or-later)

**Current Usage:**
- Direct dependency declared in Cargo.toml line 28: `actix-governor = "=0.8.0"`
- Used in `src/main.rs` lines 11, 92-99, 144
- Provides GCRA (Generic Cell Rate Algorithm) rate limiting for `/api/*` routes
- Configuration: 50 req/sec (default), 100 burst size (default), configurable via env vars

**License Issue:**
- actix-governor v0.8.0 is GPL-3.0-or-later
- actix-governor v0.10.0 (latest) is also GPL-3.0-or-later
- Project policy (LICENSE-POLICY.md) explicitly denies GPL-3.0-or-later
- GitHub Actions workflow (security.yml line 55) denies GPL-3.0 licenses

**Dependency Chain:**
```
home-registry
└── actix-governor v0.8.0 (GPL-3.0-or-later) ❌
    ├── actix-web v4.9.0
    ├── actix-http v3.11.0
    ├── futures v0.3.31
    └── governor v0.8.1 (MIT OR Apache-2.0) ✓
```

**Impact:** High - Blocks compliance scan, violates project license policy

---

#### Issue 2: time v0.3.41 (RUSTSEC-2026-0009)

**Security Advisory:**
- CVE: RUSTSEC-2026-0009
- Severity: HIGH (DoS via stack exhaustion)
- Affected: time < 0.3.47
- Current version: 0.3.41 → Updated to 0.3.44 (still vulnerable)
- Required version: ≥ 0.3.47

**Dependency Chain:**
```
home-registry
├── actix-web v4.9.0
│   └── time v0.3.41 ❌
├── jsonwebtoken v9.3.0
│   └── simple_asn1 v0.6.3
│       └── time v0.3.41 ❌
└── (via cookie v0.16.2 → actix-web)
    └── time v0.3.41 ❌
```

**Why cargo update didn't reach v0.3.47:**
- `cargo update slab time` updated time from v0.3.41 → v0.3.44
- Dependencies (actix-web, simple_asn1, cookie) have version constraints preventing v0.3.47
- Requires updating parent dependencies to unlock v0.3.47 compatibility

**Impact:** High - Security vulnerability, exploitable DoS attack vector

---

#### Issue 3: slab v0.4.10 (Yanked)

**Status:** ✅ **RESOLVED** via `cargo update`
- Updated from v0.4.10 → v0.4.12
- Used by: futures-util v0.3.31, h2 v0.3.26 (transitive dependencies)
- No further action required

**Dependency Chain:**
```
home-registry
├── actix-web v4.9.0
│   ├── futures-util v0.3.31
│   │   └── slab v0.4.12 ✓
│   └── h2 v0.3.26
│       └── slab v0.4.12 ✓
└── tokio-postgres v0.7.12
    └── futures-util v0.3.31
        └── slab v0.4.12 ✓
```

**Impact:** Low - Already resolved, prevent future yanked dependency issues

---

#### Issue 4: deny.toml Configuration Warnings

**Warnings:**
1. `unused-allowed-license = "warn"` - Some allowed licenses are not encountered in current dependencies
2. Advisory ignores may flag non-existent advisories

**Current deny.toml Configuration:**
- **Line 44-56**: Allowed licenses include many that may not be in use (MPL-2.0, LGPL-2.1, etc.)
- **Line 31-35**: Advisory ignore for RUSTSEC-2026-0003 (cmov CVSS 4.0 compatibility issue)

**Impact:** Low - Cosmetic warnings, but could mask real issues if not cleaned up

---

## Research: Rate Limiting Alternatives

### Alternative 1: actix-extensible-rate-limit v0.4.0 ⭐ **RECOMMENDED**

**License:** MIT OR Apache-2.0 ✓  
**Repository:** https://github.com/jacob-pro/actix-extensible-rate-limit  
**Documentation:** https://docs.rs/actix-extensible-rate-limit/0.4.0

**Features:**
- In-memory backend via dashmap (default)
- Optional Redis backend for distributed systems
- Simple API similar to actix-governor
- Active maintenance (last update: recent)
- MIT/Apache-2.0 dual license (project-compliant)

**Example Usage:**
```rust
use actix_extensible_rate_limit::{backend::memory::MemoryBackend, RateLimiter};

let backend = MemoryBackend::new(100, 50); // burst, replenish rate
let limiter = RateLimiter::builder(backend, |req: &ServiceRequest| {
    Some(req.peer_addr()?.ip().to_string()) // Key by IP
})
.build();

App::new()
    .service(api_scope.wrap(limiter))
```

**Pros:**
- ✅ Permissive license (MIT OR Apache-2.0)
- ✅ Similar API to actix-governor
- ✅ In-memory backend (no Redis required for basic use)
- ✅ Well-documented

**Cons:**
- ⚠️ Different configuration API (requires code changes)
- ⚠️ Less stars/popularity than actix-governor

---

### Alternative 2: actix-limitation v0.5.1

**License:** MIT OR Apache-2.0 ✓  
**Repository:** https://github.com/actix/actix-extras (official actix-extras)  
**Documentation:** https://docs.rs/actix-limitation/0.5.1

**Features:**
- Fixed window counter algorithm
- Redis-backed by default (requires Redis server)
- Part of official actix-extras ecosystem
- Maintained by Actix team

**Example Usage:**
```rust
use actix_limitation::{Limiter, RateLimiter};

let limiter = Limiter::builder("redis://127.0.0.1")
    .cookie_name("rate-limit")
    .limit(100)
    .period(Duration::from_secs(60))
    .build()
    .unwrap();

App::new()
    .wrap(RateLimiter::default())
    .app_data(web::Data::new(limiter))
```

**Pros:**
- ✅ Official actix-extras project
- ✅ Permissive license (MIT OR Apache-2.0)
- ✅ Well-maintained by Actix team

**Cons:**
- ❌ Requires Redis server (adds infrastructure dependency)
- ⚠️ Fixed window algorithm (less sophisticated than GCRA)
- ⚠️ More complex setup

---

### Alternative 3: actix-ratelimit v0.3.1

**License:** MIT ✓  
**Repository:** https://github.com/TerminalWitchcraft/actix-ratelimit  
**Documentation:** https://docs.rs/actix-ratelimit/0.3.1

**Features:**
- Multiple backends: memory (dashmap), Redis, Memcached
- Flexible rate limiting framework
- MIT license

**Pros:**
- ✅ Permissive license (MIT)
- ✅ Multiple backend options
- ✅ In-memory backend available

**Cons:**
- ⚠️ Less popular than actix-governor
- ⚠️ Different API requiring code changes

---

### Alternative 4: Custom Middleware (Not Recommended)

**Pros:**
- ✅ Full control over licensing
- ✅ Tailored to exact requirements

**Cons:**
- ❌ Significant development time
- ❌ Security risk (rate limiting is security-critical)
- ❌ Maintenance burden
- ❌ Needs thorough testing and review

---

## Proposed Solutions

### Solution 1: Replace actix-governor with actix-extensible-rate-limit ⭐ **RECOMMENDED**

**Rationale:**
- Resolves GPL-3.0-or-later license issue immediately
- Minimal code changes required (similar API)
- No additional infrastructure dependencies (in-memory backend)
- Permissive MIT OR Apache-2.0 license
- Active maintenance and good documentation

**Changes Required:**
1. Update `Cargo.toml` line 28:
   ```toml
   - actix-governor = "=0.8.0"  # Rate limiting middleware
   + actix-extensible-rate-limit = "=0.4.0"  # Rate limiting middleware
   ```

2. Update `src/main.rs` imports (line 11):
   ```rust
   - use actix_governor::{Governor, GovernorConfigBuilder};
   + use actix_extensible_rate_limit::{backend::memory::MemoryBackend, RateLimiter};
   ```

3. Update rate limiter configuration (lines 92-99):
   ```rust
   - let governor_conf = GovernorConfigBuilder::default()
   -     .requests_per_second(requests_per_second)
   -     .burst_size(burst_size)
   -     .finish()
   -     .expect("Failed to build rate limiter configuration");
   
   + let backend = MemoryBackend::builder()
   +     .burst_size(burst_size as u64)
   +     .interval(Duration::from_millis(1000 / requests_per_second))
   +     .build();
   + 
   + let limiter = RateLimiter::builder(backend, |req: &ServiceRequest| {
   +     Some(req.peer_addr()?.ip().to_string())
   + })
   + .add_headers()
   + .build();
   ```

4. Update middleware application (line 144):
   ```rust
   - .wrap(Governor::new(&governor_conf))
   + .wrap(limiter)
   ```

**Risk Level:** Low
- Well-tested library with similar functionality
- API migration is straightforward
- No infrastructure changes required

---

### Solution 2: Update actix-web and Related Dependencies

**Rationale:**
- Resolves `time v0.3.41` security vulnerability
- Addresses yanked `slab v0.4.10` (already partially resolved)
- Brings in latest security patches and performance improvements
- May enable upgrading to `time v0.3.47+`

**Changes Required:**

Update `Cargo.toml` dependencies (lines 22-40):

```toml
# Current versions
actix-web = "=4.9.0"
actix-files = "=0.6.6"
actix-cors = "=0.7.0"
# actix-governor = "=0.8.0"  # To be replaced in Solution 1
tokio = { version = "=1.42.0", features = ["full"] }

# Updated versions
actix-web = "=4.12.1"       # 4.9.0 → 4.12.1
actix-files = "=0.6.10"     # 0.6.6 → 0.6.10
actix-cors = "=0.7.1"       # 0.7.0 → 0.7.1
# actix-extensible-rate-limit = "=0.4.0"  # New dependency from Solution 1
tokio = { version = "=1.42.0", features = ["full"] }  # Already latest
```

**Verification Steps:**
1. Run `cargo update --aggressive` after Cargo.toml changes
2. Verify `time` crate updates to v0.3.47 or later:
   ```powershell
   cargo tree -i time | Select-String "time v"
   ```
3. Run full test suite: `cargo test`
4. Run cargo deny check: `cargo deny check`

**Risk Level:** Medium
- Actix-web has good backward compatibility track record
- Minor version updates (4.9.0 → 4.12.1) should be safe
- Requires thorough testing to ensure no breaking changes

---

### Solution 3: Clean up deny.toml Configuration

**Rationale:**
- Remove unused license allowances that trigger warnings
- Document advisory ignores with clear justification
- Improve configuration maintainability

**Changes Required:**

Update `.github/deny.toml`:

1. **Review and prune license allowances (lines 44-56):**
   - Keep only licenses actually used by current dependencies
   - Run `cargo deny check licenses --show-stats` to identify used licenses

2. **Document advisory ignores (lines 31-35):**
   ```toml
   ignore = [
       # RUSTSEC-2026-0003: cmov advisory uses CVSS 4.0 which cargo-deny doesn't support yet
       # TODO: Remove this when cargo-deny supports CVSS 4.0
       # Last reviewed: 2026-02-13
       "RUSTSEC-2026-0003",
   ]
   ```

3. **Update unused-allowed-license setting:**
   ```toml
   # Change from "warn" to "deny" after cleanup
   unused-allowed-license = "deny"
   ```

**Risk Level:** Low
- Configuration-only changes
- No code impact
- Improves future maintainability

---

## Implementation Plan

### Phase 1: Replace actix-governor (Solution 1) - Priority: HIGH

**Steps:**
1. ✅ Research and identify actix-extensible-rate-limit as replacement
2. 🔲 Update Cargo.toml dependency declaration
3. 🔲 Refactor src/main.rs to use new rate limiting library
   - Update imports
   - Replace configuration builder code
   - Update middleware application
4. 🔲 Test rate limiting functionality:
   - Unit tests for configuration
   - Integration tests for rate limit enforcement
   - Manual testing of 429 responses with rate limit headers
5. 🔲 Update documentation:
   - Comment updates in src/main.rs
   - README.md if rate limiting is mentioned
   - .github/docs if architectural docs exist

**Acceptance Criteria:**
- ✅ cargo deny check licenses passes (no GPL-3.0-or-later)
- ✅ Rate limiting works identically to previous implementation
- ✅ 429 responses include appropriate headers (Retry-After, etc.)
- ✅ All tests pass (cargo test)
- ✅ Manual testing confirms rate limits enforce correctly

**Time Estimate:** 2-3 hours

---

### Phase 2: Update Actix Dependencies (Solution 2) - Priority: HIGH

**Steps:**
1. 🔲 Update Cargo.toml with new actix versions
2. 🔲 Run `cargo update --aggressive`
3. 🔲 Verify time crate upgraded to v0.3.47+:
   ```powershell
   cargo tree -i time
   ```
4. 🔲 Run full test suite: `cargo test`
5. 🔲 Run integration tests: `cargo test --test integration_test`
6. 🔲 Manual smoke testing:
   - Start server: `cargo run`
   - Test API endpoints
   - Test rate limiting
   - Test authentication flows
7. 🔲 Verify cargo deny check passes:
   ```powershell
   cargo deny check advisories
   ```

**Rollback Plan:**
If issues arise, revert Cargo.toml changes and run `cargo update` to restore lock file.

**Acceptance Criteria:**
- ✅ time crate is v0.3.47 or later
- ✅ slab crate is v0.4.12 (already done)
- ✅ cargo deny check advisories passes (no RUSTSEC-2026-0009)
- ✅ All tests pass
- ✅ Manual testing confirms no regressions

**Time Estimate:** 1-2 hours

---

### Phase 3: Clean up deny.toml (Solution 3) - Priority: MEDIUM

**Steps:**
1. 🔲 Run license statistics:
   ```powershell
   cargo deny check licenses --show-stats
   ```
2. 🔲 Identify unused license allowances
3. 🔲 Update deny.toml:
   - Remove unused licenses from allow list
   - Add comments documenting why each license is allowed
4. 🔲 Document advisory ignores with review dates
5. 🔲 Change `unused-allowed-license = "warn"` to `"deny"`
6. 🔲 Verify cargo deny check passes:
   ```powershell
   cargo deny check
   ```

**Acceptance Criteria:**
- ✅ No warnings about unused-allowed-license
- ✅ All advisory ignores have documentation and review dates
- ✅ cargo deny check passes cleanly

**Time Estimate:** 30 minutes - 1 hour

---

## Verification & Testing

### Pre-Implementation Checks

```powershell
# Check current deny status
cargo deny check

# Check dependency tree for problematic crates
cargo tree -i actix-governor
cargo tree -i time
cargo tree -i slab

# Check available updates
cargo outdated
```

### Post-Implementation Checks

```powershell
# Full cargo deny check
cargo deny check advisories
cargo deny check licenses
cargo deny check bans
cargo deny check sources

# Build and test
cargo build --release
cargo test --all-features
cargo test --test integration_test

# Check for any remaining issues
cargo audit
cargo outdated
```

### Manual Testing Checklist

- [ ] Server starts successfully: `cargo run`
- [ ] Health endpoint responds: `curl http://localhost:8210/health`
- [ ] Rate limiting enforces correctly:
  ```powershell
  # Generate rapid requests to trigger rate limit
  for ($i=0; $i -lt 150; $i++) { curl http://localhost:8210/api/items }
  # Should see 429 Too Many Requests after burst limit
  ```
- [ ] Rate limit headers present in 429 response:
  - Retry-After
  - X-RateLimit-Limit (if supported)
  - X-RateLimit-Remaining (if supported)
- [ ] API endpoints functional:
  - GET /api/items
  - POST /api/items
  - Authentication endpoints
- [ ] Static file serving works: `curl http://localhost:8210/`
- [ ] Frontend loads correctly

---

## Potential Risks & Mitigations

### Risk 1: Breaking Changes in actix-extensible-rate-limit API

**Probability:** Low  
**Impact:** Medium  
**Mitigation:**
- Thorough testing of rate limiting behavior
- Review actix-extensible-rate-limit examples and documentation
- Have rollback plan ready (keep old Cargo.toml in git history)
- Implement comprehensive integration tests for rate limiting

### Risk 2: actix-web 4.12.1 Introduces Breaking Changes

**Probability:** Low (minor version bump)  
**Impact:** High (affects entire API)  
**Mitigation:**
- Review actix-web CHANGELOG between 4.9.0 and 4.12.1
- Run full test suite after update
- Test all endpoints manually
- Have rollback plan ready
- Update in non-production environment first

### Risk 3: time v0.3.47 Not Reachable Due to Version Constraints

**Probability:** Medium  
**Impact:** High (security vulnerability remains)  
**Mitigation:**
- If `cargo update` doesn't reach v0.3.47, investigate specific constraints:
  ```powershell
  cargo tree -i time -e normal --format "{p} -> {r}"
  ```
- Consider updating other dependencies (jsonwebtoken, cookie) if they constrain time
- As last resort, use `[patch]` section in Cargo.toml to force time v0.3.47
  ```toml
  [patch.crates-io]
  time = "0.3.47"
  ```

### Risk 4: Rate Limiting Behavior Changes

**Probability:** Medium  
**Impact:** Medium  
**Mitigation:**
- Document current rate limiting behavior before changes
- Test with various request patterns:
  - Slow steady requests (under limit)
  - Burst requests (hitting burst limit)
  - Sustained high rate (above limit)
- Monitor logs for rate limit violations
- Configure environment variables for rate limits to allow tuning

### Risk 5: License Compliance False Positives After Cleanup

**Probability:** Low  
**Impact:** Low  
**Mitigation:**
- Keep comprehensive comments in deny.toml
- Run `cargo deny check licenses` multiple times during development
- Document rationale for each allowed license
- Review deny.toml configuration in PR reviews

---

## Dependencies & Prerequisites

### Tools Required
- Rust toolchain (current version is sufficient)
- cargo-deny (install if not present): `cargo install cargo-deny --locked`
- cargo-outdated (optional): `cargo install cargo-outdated`
- cargo-audit (optional): `cargo install cargo-audit`

### Knowledge Required
- Actix-web middleware system
- Rust async/await patterns
- Rate limiting algorithms (GCRA vs Fixed Window)
- Cargo dependency management
- License compliance basics

### Environment Setup
- PostgreSQL database (for integration tests)
- Environment variables configured (.env file):
  - DATABASE_URL
  - RATE_LIMIT_RPS (optional, default 50)
  - RATE_LIMIT_BURST (optional, default 100)

---

## Expected Outcomes

### After Phase 1 (actix-governor Replacement)
- ✅ GPL-3.0-or-later license violation resolved
- ✅ cargo deny check licenses passes
- ✅ Rate limiting functionality preserved
- ✅ GitHub Actions security workflow passes

### After Phase 2 (Dependency Updates)
- ✅ RUSTSEC-2026-0009 security vulnerability resolved
- ✅ time crate updated to v0.3.47 or later
- ✅ slab crate on latest stable version (v0.4.12)
- ✅ All dependencies at recommended versions
- ✅ cargo deny check advisories passes

### After Phase 3 (deny.toml Cleanup)
- ✅ No configuration warnings
- ✅ All license allowances documented and justified
- ✅ Advisory ignores documented with review dates
- ✅ Improved maintainability for future dependency audits

### Overall Project Benefits
- ✅ Full compliance with project license policy
- ✅ Resolved critical security vulnerabilities
- ✅ Updated dependencies with latest security patches
- ✅ Cleaner, more maintainable configuration
- ✅ CI/CD pipeline passes all security checks

---

## Alternative Approaches Considered

### Approach A: Keep actix-governor, Add GPL-3.0-or-later to Allow List

**Rejected Reason:**
- Violates project license policy (LICENSE-POLICY.md)
- GPL-3.0-or-later is strong copyleft, incompatible with proprietary use
- GitHub Actions explicitly denies GPL-3.0 licenses
- Would require policy change and legal review

### Approach B: Use actix-limitation with Redis

**Rejected Reason:**
- Adds infrastructure dependency (Redis server)
- Increased operational complexity
- Fixed window algorithm less sophisticated than GCRA
- Home Registry is a single-instance home application, doesn't need distributed rate limiting

### Approach C: Implement Custom Rate Limiting Middleware

**Rejected Reason:**
- Significant development time (estimated 8-16 hours)
- Security-critical component requires extensive testing
- Ongoing maintenance burden
- Risk of implementation bugs
- Existing MIT/Apache-2.0 libraries available that solve the problem

### Approach D: Use cargo [patch] to Force time v0.3.47

**Considered for Fallback:**
- Only use if normal dependency updates fail to reach v0.3.47
- Has risks: may introduce incompatibilities
- Preferred approach: update parent dependencies first

---

## References & Research Sources

### Source 1: Cargo-Deny Documentation
**URL:** https://embarkstudios.github.io/cargo-deny/  
**Relevance:** Configuration best practices, license checking, advisory detection

**Key Points:**
- How to configure license allowlists
- Advisory database structure
- Handling yanked crates
- unused-allowed-license warning explanation

---

### Source 2: actix-extensible-rate-limit Repository
**URL:** https://github.com/jacob-pro/actix-extensible-rate-limit  
**Crates.io:** https://crates.io/crates/actix-extensible-rate-limit  
**Documentation:** https://docs.rs/actix-extensible-rate-limit/0.4.0

**Key Points:**
- MIT OR Apache-2.0 license (project-compliant)
- In-memory backend via dashmap
- API examples for migration from actix-governor
- Active maintenance status

---

### Source 3: Actix-Web Official Extras (actix-limitation)
**URL:** https://github.com/actix/actix-extras  
**Crates.io:** https://crates.io/crates/actix-limitation  
**Documentation:** https://docs.rs/actix-limitation/0.5.1

**Key Points:**
- Official Actix project
- MIT OR Apache-2.0 license
- Requires Redis for storage
- Fixed window counter algorithm

---

### Source 4: RustSec Advisory Database
**URL:** https://rustsec.org/advisories/RUSTSEC-2026-0009.html  
**Advisory:** RUSTSEC-2026-0009 (time crate DoS vulnerability)

**Key Points:**
- Affects time < 0.3.47
- DoS via stack exhaustion
- Severity: HIGH
- Fix: Upgrade to time >= 0.3.47

---

### Source 5: Actix-Web Migration Guides
**URL:** https://actix.rs/docs/whats-new  
**Documentation:** https://docs.rs/actix-web/4.12.1/actix_web/

**Key Points:**
- Changelog between 4.9.0 and 4.12.1
- Breaking changes documentation
- Migration guides for middleware changes

---

### Source 6: Crates.io - Latest Version Information
**Searched Packages:**
- actix-web: 4.12.1
- actix-files: 0.6.10
- actix-cors: 0.7.1
- actix-governor: 0.10.0 (still GPL-3.0-or-later)
- actix-extensible-rate-limit: 0.4.0
- actix-limitation: 0.5.1
- actix-ratelimit: 0.3.1
- slab: 0.4.12
- time: 0.3.47

---

### Source 7: Home Registry License Policy
**File:** LICENSE-POLICY.md

**Key Points:**
- Explicitly denies GPL-3.0-or-later (line 37)
- Allowed licenses: MIT, Apache-2.0, BSD-*, ISC, Zlib, MPL-2.0, etc.
- Weekly license compliance checks required
- Documentation required for exceptions

---

### Source 8: Home Registry Security Workflow
**File:** .github/workflows/security.yml

**Key Points:**
- dependency-review-action denies GPL-3.0 and AGPL-3.0 (line 55)
- Runs on every PR and weekly schedule
- Includes Trivy container scanning
- Generates SBOM (Software Bill of Materials)

---

## Document Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-02-13 | Research Subagent | Initial specification created |

---

## Next Steps

1. **Implementation Subagent** should read this specification and implement Phase 1 and Phase 2 sequentially
2. After implementation, **Review Subagent** should verify:
   - License compliance (cargo deny check licenses)
   - Security advisories resolved (cargo deny check advisories)
   - Rate limiting functionality preserved
   - All tests passing
3. If refinement needed, address specific issues identified in review
4. Phase 3 (cleanup) can be implemented separately as lower priority

---

**Specification Status:** ✅ **COMPLETE - READY FOR IMPLEMENTATION**

**Critical Issues Summary:**
- 🔴 GPL-3.0-or-later license violation (actix-governor)
- 🔴 Security vulnerability RUSTSEC-2026-0009 (time crate)
- 🟢 Yanked crate (slab) - ALREADY RESOLVED
- 🟡 Configuration warnings (low priority)

**Recommended Implementation Order:**
1. Phase 1: Replace actix-governor (4-6 hours)
2. Phase 2: Update Actix dependencies (2-3 hours)
3. Phase 3: Clean up deny.toml (1 hour)

**Total Estimated Time:** 7-10 hours (including testing and documentation)
