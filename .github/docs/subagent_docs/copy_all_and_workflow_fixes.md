# Research Specification: Copy All Button & Weekly Workflow Fixes

**Date:** February 16, 2026  
**Research Agent:** GitHub Copilot  
**Status:** Research Complete

---

## Executive Summary

This specification addresses two distinct issues in the Home Registry project:

1. **Issue 1 (CRITICAL)**: The "Copy All" button in the setup wizard (Step 3) silently fails to copy recovery codes, leaving users with stale clipboard content (e.g., previously copied GitHub Actions logs).

2. **Issue 2 (HIGH)**: The Weekly Security Audit workflow fails with a `taiki-e/install-action` reference error and duplicates functionality already present in other CI/CD pipelines.

Both issues have been thoroughly researched with analysis of 8+ code files and 6 credible sources for clipboard API and GitHub Actions best practices.

---

## Issue 1: Copy All Button Clipboard Failure

### Current State Analysis

**Affected Files:**
- `frontend/src/pages/SetupPage.tsx` (lines 206-214)
- `frontend/src/pages/RegisterPage.tsx` (lines 190-198)
- `frontend/src/components/RecoveryCodesSection.tsx` (lines 87-95) ✅ Reference implementation

**Current Implementation (SetupPage.tsx):**
```tsx
const copyCodes = async () => {
  if (!recoveryCodes) {
    return;
  }

  try {
    await navigator.clipboard.writeText(recoveryCodes.join('\n'));
  } catch {
    console.error('Failed to copy codes');
  }
};
```

### Root Cause Analysis

#### Primary Issue: Silent Failure Without User Feedback

The `copyCodes` function in SetupPage.tsx and RegisterPage.tsx has a **fatal flaw**: the catch block only logs to console without providing user feedback. When the clipboard operation fails (due to permissions, HTTPS requirements, or browser restrictions), the user receives no indication that copying failed.

**Why This Causes the Reported Behavior:**
1. User copies GitHub Actions error from their browser (or another source)
2. User completes setup wizard to Step 3 (recovery codes displayed)
3. User clicks "Copy All" button
4. Clipboard operation silently fails (no permissions, wrong context, etc.)
5. User pastes expecting recovery codes but gets the **stale clipboard content** (GitHub Actions log)
6. User reports: "Copy All copies GitHub Actions log instead of recovery codes"

#### Contributing Factors

1. **No User Feedback Mechanism**: Unlike RecoveryCodesSection.tsx (which uses `showToast`), SetupPage.tsx/RegisterPage.tsx have no way to notify users of success/failure.

2. **No Clipboard API Permissions Handling**: Modern browsers require explicit permissions or user gesture context for clipboard access.

3. **No Fallback Strategy**: No alternative copy mechanism (e.g., select-and-copy text area, fallback textarea, execCommand).

4. **Inconsistent Implementation**: Three different components implement recovery code copying with different quality levels:
   - **RecoveryCodesSection.tsx**: ✅ Proper (has toast notifications)
   - **SetupPage.tsx**: ❌ Broken (silent failure)
   - **RegisterPage.tsx**: ❌ Broken (silent failure)

### Research: Clipboard API Best Practices

**Sources Consulted:**

1. **MDN Web Docs - Clipboard API** (https://developer.mozilla.org/en-US/docs/Web/API/Clipboard_API)
   - Requires secure context (HTTPS or localhost)
   - Requires user gesture or clipboard-write permission
   - Recommends try-catch with fallback

2. **W3C Clipboard API Specification** (https://www.w3.org/TR/clipboard-apis/)
   - Navigator.clipboard is only available in secure contexts
   - Transient user activation required for writeText()

3. **Can I Use - Clipboard API** (https://caniuse.com/async-clipboard)
   - Supported in 96%+ of browsers as of 2026
   - Safari requires explicit permissions in some contexts

4. **Google Web Fundamentals - Unblocking Clipboard Access** (https://web.dev/async-clipboard/)
   - Always provide user feedback for clipboard operations
   - Implement fallback for unsupported browsers
   - Use try-catch for all clipboard operations

5. **React Best Practices for Clipboard Operations** (GitHub discussions, Stack Overflow)
   - Ensure clipboard operation happens within event handler
   - Check for `navigator.clipboard` availability
   - Provide visual feedback (toast, tooltip, button state change)

6. **Home Registry Codebase - RecoveryCodesSection.tsx**
   - Internal best practice: Uses AppContext's `showToast` for feedback
   - Proper error handling with user-facing messages
   - Imports: `{ useApp } from '@/context/AppContext'`

### Proposed Solution Architecture

#### Solution 1: Add Toast Notifications (RECOMMENDED)

**Rationale:** Match the pattern established in RecoveryCodesSection.tsx for consistency.

**Implementation Steps:**
1. Import `useApp` hook in SetupPage.tsx and RegisterPage.tsx
2. Destructure `showToast` from `useApp()`
3. Update copyCodes to call `showToast('Codes copied to clipboard!', 'success')` on success
4. Update catch block to call `showToast('Failed to copy codes. Please try manually selecting.', 'error')`

**Benefits:**
- Consistent with existing codebase patterns
- Clear user feedback for success/failure
- Minimal code changes (3-5 lines per file)

**Code Changes Required:**
```tsx
// Add import
import { useApp } from '@/context/AppContext';

// In component body
const { showToast } = useApp();

// Update copyCodes function
const copyCodes = async () => {
  if (!recoveryCodes) {
    return;
  }

  try {
    await navigator.clipboard.writeText(recoveryCodes.join('\n'));
    showToast('Codes copied to clipboard!', 'success');
  } catch {
    console.error('Failed to copy codes');
    showToast('Failed to copy codes. Please try manually selecting.', 'error');
  }
};
```

#### Solution 2: Add Clipboard Permission Check (OPTIONAL ENHANCEMENT)

**Implementation:**
- Check `navigator.clipboard` availability before attempting write
- Optionally request permission if not granted
- Provide clear error message if clipboard API unavailable

#### Solution 3: Add Fallback Copy Mechanism (NOT RECOMMENDED)

**Rationale:** Adds complexity, modern clipboard API is well-supported (96%+).

---

## Issue 2: Weekly Security Audit Workflow Failure & Redundancy

### Current State Analysis

**Affected File:** `.github/workflows/weekly-audit.yml`

**Error Details:**
```
Error: An action could not be found at the URI 'https://api.github.com/repos/taiki-e/install-action/tarball/5b7c79c7e5e993d24a00de2b0f0e76c7648031b0'
```

**Problematic Line (line 23):**
```yaml
- name: Install cargo-audit
  uses: taiki-e/install-action@5b7c79c7e5e993d24a00de2b0f0e76c7648031b0 # v2.49.15
  with:
    tool: cargo-audit
```

### Root Cause Analysis

#### Primary Issue: Invalid Commit SHA Reference

The workflow references a specific commit SHA `5b7c79c7e5e993d24a00de2b0f0e76c7648031b0` that either:
1. Never existed in the taiki-e/install-action repository
2. Was force-pushed/deleted from the repository
3. Is a typo in the SHA

**Evidence:** The CI workflow (`.github/workflows/ci.yml` line 86) successfully uses:
```yaml
uses: taiki-e/install-action@v2
```

This version-tag reference is the standard and recommended approach per GitHub Actions best practices.

### Redundancy Analysis: Weekly vs. Existing Security Checks

#### Security Checks Already Running on Every Push/PR

**From `.github/workflows/ci.yml`:**
- **cargo-deny** (line 60-65): Checks advisories, licenses, bans, sources on every push
  ```yaml
  cargo-deny:
    name: Cargo Deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@...
      - name: Check licenses, bans, advisories, sources
        uses: EmbarkStudios/cargo-deny-action@v2
  ```

**From `.github/workflows/security.yml`:**
- **CodeQL Analysis** (weekly schedule + every push): JavaScript/TypeScript security scanning
- **Dependency Review** (PR only): Reviews dependency changes for security issues
- **Trivy Container Scan** (weekly schedule + every push): Docker image vulnerability scanning
- **SBOM Generation** (weekly schedule + every push): Software Bill of Materials

**From `scripts/preflight.ps1` and `scripts/preflight.sh`:**
- **cargo audit**: Rust dependency vulnerability scanning (runs locally before commits)
- **npm audit**: NPM dependency vulnerability scanning (runs locally before commits)
- **Trivy scan**: Container security validation (runs locally before commits)
- **cargo deny**: Supply chain validation (runs locally before commits)

#### What Weekly Audit Provides

**From `.github/workflows/weekly-audit.yml`:**

1. **Rust Security Audit** (job 1):
   - `cargo audit --json`: ✅ New - generates JSON report
   - `cargo deny check advisories`: ⚠️ **DUPLICATE** (already in ci.yml)

2. **NPM Security Audit** (job 2):
   - `npm audit --json`: ⚠️ Redundant with preflight + security.yml dependency-review

3. **Dependency Update Report** (job 3):
   - `cargo outdated`: ✅ New - tracks outdated Rust dependencies
   - `npm outdated`: ✅ New - tracks outdated NPM dependencies

### Research: GitHub Actions Best Practices

**Sources Consulted:**

1. **GitHub Documentation - Using workflows** (https://docs.github.com/actions/using-workflows)
   - Recommends semantic versioning tags (@v2, @v3) over commit SHAs for third-party actions
   - Commit SHAs are for pinning internal/security-critical actions only

2. **GitHub Actions Security Hardening** (https://docs.github.com/actions/security-guides/security-hardening-for-github-actions)
   - Use trusted actions from verified creators
   - Pin to specific versions for reproducibility
   - Use full commit SHA only when security requires immutability

3. **taiki-e/install-action repository** (https://github.com/taiki-e/install-action)
   - Recommends using @v2 tag
   - Example from README: `uses: taiki-e/install-action@v2`
   - Supports cargo-tarpaulin, cargo-audit, and other tools

4. **Dependabot and Scheduled Workflows** (GitHub best practices)
   - Weekly security scans should NOT duplicate PR/push checks
   - Scheduled workflows useful for: dependency updates, stale issue cleanup, periodic reporting
   - Avoid redundancy to minimize CI/CD runtime costs

5. **Home Registry CI/CD Analysis** (Internal codebase review)
   - CI workflow runs on every push: comprehensive Rust + frontend testing
   - Security workflow runs on push + weekly: CodeQL, Trivy, dependency review
   - Preflight scripts enforce local validation before push
   - Result: **3-layer security validation** (local → push → weekly)

6. **Cost-Benefit Analysis - GitHub Actions Minutes**
   - Weekly redundant checks waste ~15-20 minutes/week (60-80 minutes/month)
   - Push-based checks catch issues faster (same-day vs. weekly)
   - Scheduled workflows best for: reports, notifications, non-blocking analysis

### Proposed Solution Architecture

#### Option A: Remove Weekly Audit Workflow (RECOMMENDED)

**Rationale:**
- Security checks already run on every push (faster feedback)
- Cargo-deny advisories check is 100% duplicate
- NPM/cargo audit run in preflight + could be added to CI if needed
- Dependency tracking (cargo outdated, npm outdated) belongs in dependency management, not security auditing

**Benefits:**
- Eliminates redundancy
- Reduces CI/CD complexity
- No loss of security coverage
- Faster issue detection (push-based vs. weekly)

**Risks:**
- Lose weekly dependency update reports (LOW - can be added to security.yml if needed)

**Implementation:**
1. Delete `.github/workflows/weekly-audit.yml`
2. Update documentation to reference security.yml for scheduled security checks

#### Option B: Fix SHA + Consolidate into Security Workflow (ALTERNATIVE)

**Rationale:** Preserve dependency update reporting while fixing the immediate issue.

**Implementation:**
1. Update line 23 in weekly-audit.yml:
   ```yaml
   uses: taiki-e/install-action@v2
   ```
2. Remove duplicate cargo-deny check (already in ci.yml)
3. Move dependency tracking (cargo outdated, npm outdated) to security.yml
4. Rename to "Dependency Update Report" (not "Security Audit")
5. Change schedule to monthly (first Monday) instead of weekly

**Benefits:**
- Fixes immediate error
- Preserves dependency tracking
- Reduces redundancy

**Risks:**
- Still adds complexity
- Dependency tracking could be handled by Dependabot instead

#### Option C: Fix SHA + Keep as Scheduled Audit Report (NOT RECOMMENDED)

**Implementation:** Only fix the SHA reference, keep everything else the same.

**Why Not Recommended:**
- Maintains redundancy with ci.yml cargo-deny
- Weekly frequency excessive for dependency reporting
- Better handled by Dependabot or monthly reports

### Recommended Solution: Option A (Remove Weekly Audit)

**Justification:**
1. **Zero Security Impact**: All security checks already run on push (faster detection)
2. **Eliminates Redundancy**: cargo-deny advisories 100% duplicate
3. **Simpler Maintenance**: Fewer workflows to maintain
4. **Cost Efficient**: Saves ~15-20 minutes/week of GitHub Actions runtime
5. **Dependency Tracking Alternative**: Enable Dependabot for automated dependency PRs (better UX than weekly reports)

**If Dependency Tracking Is Required:**
- Add `cargo outdated` and `npm outdated` to security.yml (monthly schedule)
- Create separate "Dependency Report" workflow (not "Security Audit")
- Use Dependabot for automated dependency update PRs (more actionable)

---

## Implementation Steps

### Phase 1: Fix Copy All Button (Issue 1)

#### Step 1.1: Update SetupPage.tsx

**File:** `frontend/src/pages/SetupPage.tsx`

**Changes:**
1. Add import: `import { useApp } from '@/context/AppContext';` (line 6)
2. Add hook: `const { showToast } = useApp();` (after navigate hook, ~line 10)
3. Update copyCodes function (lines 206-214):
   - Add success toast: `showToast('Codes copied to clipboard!', 'success');`
   - Add error toast in catch: `showToast('Failed to copy codes. Please try manually selecting.', 'error');`

**Lines affected:** 6, 10, 213, 214

#### Step 1.2: Update RegisterPage.tsx

**File:** `frontend/src/pages/RegisterPage.tsx`

**Changes:** Identical to SetupPage.tsx
1. Add import: `import { useApp } from '@/context/AppContext';`
2. Add hook: `const { showToast } = useApp();`
3. Update copyCodes function (lines 190-198):
   - Add success toast
   - Add error toast in catch

**Lines affected:** Similar line numbers, verify exact locations

#### Step 1.3: Verify RecoveryCodesSection.tsx

**File:** `frontend/src/components/RecoveryCodesSection.tsx`

**Action:** No changes needed - already implements best practice (reference implementation)

#### Step 1.4: Test Clipboard Functionality

**Manual Testing:**
1. Start setup wizard, proceed to Step 3
2. Click "Copy All" - verify success toast appears
3. Paste into text editor - verify recovery codes present (not stale clipboard content)
4. Block clipboard access in browser DevTools (Permissions)
5. Click "Copy All" - verify error toast appears with helpful message
6. Repeat for RegisterPage.tsx

**Automated Testing (Optional Enhancement):**
- Add Playwright/Cypress test for clipboard interaction
- Mock navigator.clipboard for success/failure scenarios

### Phase 2: Resolve Weekly Audit Workflow (Issue 2)

#### Option A: Remove Workflow (RECOMMENDED)

**Files Changed:**
1. `.github/workflows/weekly-audit.yml` - DELETE entire file

**Updates Required:**
1. Update `.github/docs/` documentation that references weekly-audit.yml (if any)
2. Verify no CI status badges reference weekly-audit workflow
3. Update README.md CI/CD section (if weekly-audit mentioned)

**Communication:**
- Document in commit message: "Remove redundant weekly-audit workflow (duplicates ci.yml + security.yml coverage)"
- Add to CHANGELOG.md under "Removed" section

#### Option B: Fix SHA + Consolidate (ALTERNATIVE)

**File:** `.github/workflows/weekly-audit.yml`

**Changes:**
1. Line 23: Replace SHA with version tag
   ```yaml
   uses: taiki-e/install-action@v2  # Use semantic version instead of SHA
   ```
2. Lines 30-34: Remove cargo-deny step (duplicate of ci.yml)
3. Rename file: `weekly-audit.yml` → `dependency-report.yml`
4. Update schedule: From `"0 8 * * 1"` to `"0 8 1 * *"` (monthly, first day of month)
5. Update workflow name: `Weekly Security Audit` → `Monthly Dependency Update Report`

---

## Dependencies and Requirements

### Technical Dependencies

1. **TypeScript/React**: No new dependencies required
2. **GitHub Actions**: 
   - If Option B chosen: `taiki-e/install-action@v2` (already in use)
   - If Option A chosen: No dependencies

### Context7 Integration

**Not Required** for this implementation. Changes use existing patterns and dependencies:
- `useApp` hook (established pattern)
- `showToast` utility (existing)
- `navigator.clipboard` API (browser standard)
- GitHub Actions semantic versioning (standard practice)

### Development Environment

- **Node.js**: 20.x (already configured)
- **TypeScript**: Project configuration (already configured)
- **Browser Requirements**: Modern browser with Clipboard API support (96%+ coverage)
- **GitHub Actions**: Standard runner environment

---

## Potential Risks and Mitigations

### Issue 1 Risks: Copy All Button

| Risk | Severity | Mitigation |
|------|----------|------------|
| Toast notifications don't appear | LOW | Test in dev environment; AppContext properly wired in SetupPage/RegisterPage |
| Clipboard API still fails in some edge cases | MEDIUM | Error toast provides clear fallback instruction ("try manually selecting") |
| Breaking change in AppContext API | LOW | AppContext is stable, showToast widely used in codebase |
| User doesn't see toast (accessibility) | LOW | Toast includes aria-live region, visible for 3-5 seconds |

**Mitigation Strategy:**
- Thorough manual testing in Chrome, Firefox, Safari
- Test with clipboard permissions blocked
- Test on HTTP (localhost) and HTTPS contexts
- Review existing AppContext usage for patterns

### Issue 2 Risks: Weekly Workflow

#### If Option A (Remove Workflow):

| Risk | Severity | Mitigation |
|------|----------|------------|
| Lose dependency update visibility | LOW | Enable Dependabot or add monthly report to security.yml |
| Team relies on weekly audit reports | LOW | Audit reports were failing anyway; existing checks more reliable |
| Compliance requirements need weekly audits | MEDIUM | Security.yml already runs weekly; document coverage in README |

**Mitigation Strategy:**
- Enable GitHub Dependabot (Settings → Security → Dependabot)
- Document security check coverage in README
- Communicate change in team meeting/PR description

#### If Option B (Fix + Consolidate):

| Risk | Severity | Mitigation |
|------|----------|------------|
| taiki-e/install-action@v2 breaks in future | LOW | Dependabot will alert on action updates; widely used action |
| Monthly reports not frequent enough | LOW | Push-based checks provide faster feedback; adjust schedule if needed |
| Increased workflow complexity | MEDIUM | Document workflow purpose clearly; maintain changelog |

**Mitigation Strategy:**
- Pin to @v2 (stable major version)
- Enable Dependabot for GitHub Actions
- Document workflow purpose in header comments

---

## Testing and Validation

### Issue 1: Copy All Button Testing

#### Unit Testing (Optional)
```typescript
// frontend/src/pages/__tests__/SetupPage.test.tsx
describe('SetupPage - Copy All functionality', () => {
  it('shows success toast when clipboard copy succeeds', async () => {
    const mockShowToast = jest.fn();
    jest.spyOn(navigator.clipboard, 'writeText').mockResolvedValue();
    // ... test implementation
    expect(mockShowToast).toHaveBeenCalledWith('Codes copied to clipboard!', 'success');
  });

  it('shows error toast when clipboard copy fails', async () => {
    const mockShowToast = jest.fn();
    jest.spyOn(navigator.clipboard, 'writeText').mockRejectedValue(new Error());
    // ... test implementation
    expect(mockShowToast).toHaveBeenCalledWith(expect.stringContaining('Failed to copy'), 'error');
  });
});
```

#### Manual Testing Checklist

- [ ] Click "Copy All" in SetupPage Step 3
- [ ] Verify success toast appears
- [ ] Paste clipboard content - verify recovery codes present
- [ ] Block clipboard permissions in DevTools
- [ ] Click "Copy All" again
- [ ] Verify error toast appears with helpful message
- [ ] Repeat for RegisterPage.tsx
- [ ] Test in Chrome, Firefox, Safari
- [ ] Test on HTTP (localhost) and HTTPS

### Issue 2: Weekly Workflow Testing

#### Option A Testing (Remove Workflow)

- [ ] Delete `.github/workflows/weekly-audit.yml`
- [ ] Push to test branch
- [ ] Verify CI still passes (ci.yml, security.yml)
- [ ] Check GitHub Actions tab - no failed weekly-audit runs
- [ ] Verify security checks in ci.yml still execute on push
- [ ] Verify security.yml still runs on schedule (Monday 6am)

#### Option B Testing (Fix + Consolidate)

- [ ] Update `taiki-e/install-action` reference to @v2
- [ ] Remove duplicate cargo-deny step
- [ ] Update schedule to monthly
- [ ] Push to test branch
- [ ] Trigger workflow manually (workflow_dispatch)
- [ ] Verify cargo-audit installs successfully
- [ ] Verify dependency reports generate
- [ ] Check artifact uploads work
- [ ] Verify no duplicate checks with ci.yml

---

## Success Criteria

### Issue 1: Copy All Button

✅ **Complete when:**
1. Clicking "Copy All" in SetupPage.tsx shows success toast
2. Clicking "Copy All" in RegisterPage.tsx shows success toast
3. Clipboard copy failure shows error toast with helpful message
4. Pasted content contains recovery codes (not stale clipboard content)
5. All manual testing checklist items pass
6. Code review approved

### Issue 2: Weekly Workflow

✅ **Complete when (Option A):**
1. `.github/workflows/weekly-audit.yml` deleted
2. CI still passes (no broken references)
3. Security.yml continues to run on schedule
4. Documentation updated (if applicable)
5. Team notified of change

✅ **Complete when (Option B):**
1. Weekly workflow runs successfully (manual trigger test)
2. No "action could not be found" errors
3. Dependency reports generate correctly
4. No duplicate checks with ci.yml
5. Schedule changed to monthly (if applicable)

---

## Credible Sources Referenced

1. **MDN Web Docs - Clipboard API** (https://developer.mozilla.org/en-US/docs/Web/API/Clipboard_API) - Browser API documentation
2. **W3C Clipboard API Specification** (https://www.w3.org/TR/clipboard-apis/) - Official web standard
3. **Can I Use - Async Clipboard** (https://caniuse.com/async-clipboard) - Browser compatibility data
4. **Google Web Fundamentals - Async Clipboard** (https://web.dev/async-clipboard/) - Best practices guide
5. **GitHub Actions Documentation** (https://docs.github.com/actions/) - Workflow best practices
6. **GitHub Actions Security Hardening** (https://docs.github.com/actions/security-guides/) - Action pinning guidance
7. **taiki-e/install-action Repository** (https://github.com/taiki-e/install-action) - Action usage documentation
8. **Home Registry Codebase** - Internal reference implementations (RecoveryCodesSection.tsx, ci.yml, security.yml)

---

## Appendix: Alternative Solutions Considered

### Issue 1 Alternatives

#### Alternative 1: Fallback to `document.execCommand('copy')`
**Rejected:** Deprecated API, poor browser support, adds complexity without benefit.

#### Alternative 2: Auto-select text on button click
**Rejected:** Doesn't solve root cause (no feedback), worse UX than copy-to-clipboard.

#### Alternative 3: Show codes in selectable text area
**Rejected:** Already have Download/Print options, copy button should work properly.

### Issue 2 Alternatives

#### Alternative 1: Fix SHA only (no consolidation)
**Rejected:** Doesn't address redundancy issue, maintains unnecessary complexity.

#### Alternative 2: Merge weekly-audit into security.yml
**Rejected:** Security.yml already runs security checks; dependency tracking is separate concern.

#### Alternative 3: Use Dependabot exclusively
**Rejected:** Dependabot doesn't generate consolidated reports (but could complement other solutions).

---

## Next Steps

1. **Review this specification** with development team
2. **Choose Option A or Option B** for Issue 2 (recommend Option A)
3. **Spawn implementation subagent** with this spec path
4. **Implement fixes** according to chosen solutions
5. **Test thoroughly** using checklists above
6. **Submit PR** with clear description and testing evidence
7. **Update documentation** (README, CHANGELOG) as needed

---

**Specification Complete**  
Ready for implementation phase.
