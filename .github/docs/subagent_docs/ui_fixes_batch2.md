# UI Fixes Batch 2 — Specification

**Date:** 2026-02-16  
**Scope:** 4 frontend issues in `frontend/src/`  
**Status:** Research Complete — Ready for Implementation

---

## Table of Contents

1. [Issue 1: Copy All Toast Not Appearing on SetupPage](#issue-1-copy-all-toast-not-appearing-on-setuppage)
2. [Issue 2: Remove Print Button from SetupPage Step 3](#issue-2-remove-print-button-from-setuppage-step-3)
3. [Issue 3: Rework Instructions Modal to Quick-Start Guide](#issue-3-rework-instructions-modal-to-quick-start-guide)
4. [Issue 4: Instructions Modal Close/Checkbox Persistence](#issue-4-instructions-modal-closecheckbox-persistence)
5. [Implementation Steps](#implementation-steps)
6. [Risks and Mitigations](#risks-and-mitigations)

---

## Issue 1: Copy All Toast Not Appearing on SetupPage

### Current State

- **File:** `frontend/src/pages/SetupPage.tsx`
- `useApp()` is imported (line 5) and `showToast` is destructured (line 10)
- The `copyCodes` function (approx. line 215) correctly calls `showToast('Codes copied to clipboard!', 'success')` on success and `showToast('Failed to copy codes...', 'error')` on failure
- The clipboard API call itself is correct

### Root Cause

**The `<Toast />` component is NOT rendered in the auth pages branch of `AppContent`.**

In `frontend/src/App.tsx`, the `AppContent` component has two rendering branches:

1. **Auth pages branch** (lines 93–143): When pathname is `/setup`, `/login`, `/register`, or `/recover`, it returns ONLY `<Routes>` with the page components. **No `<Toast />` component.**

2. **Main app branch** (lines 147–215): Returns `<Sidebar>`, `<InstructionsModal>`, `<main>` with routes, AND `<Toast />` (line 215).

When `SetupPage` calls `showToast()`, the toast message is added to `AppContext.toasts` state array correctly. However, since the `<Toast />` component is not mounted in the auth pages branch, there is nothing to render those toasts visually.

**The same bug affects:**
- `RegisterPage` (`frontend/src/pages/RegisterPage.tsx`) — `copyCodes` calls `showToast` (approx. line 207) but will also not show toasts for the same reason.

### Solution

Add `<Toast />` to the auth pages rendering branch in `AppContent` (`frontend/src/App.tsx`).

### Exact Code Change

In `frontend/src/App.tsx`, locate the auth pages branch (approx lines 93–143):

```tsx
// Auth pages (no sidebar)
if (
  location.pathname === '/setup' ||
  location.pathname === '/login' ||
  location.pathname === '/register' ||
  location.pathname === '/recover'
) {
  return (
    <Routes>
      {/* ...routes... */}
    </Routes>
  );
}
```

Change to:

```tsx
// Auth pages (no sidebar)
if (
  location.pathname === '/setup' ||
  location.pathname === '/login' ||
  location.pathname === '/register' ||
  location.pathname === '/recover'
) {
  return (
    <>
      <Routes>
        {/* ...routes... */}
      </Routes>
      <Toast />
    </>
  );
}
```

This wraps the return in a Fragment and adds `<Toast />` after the routes. The `Toast` component is already imported at line 4 of App.tsx.

---

## Issue 2: Remove Print Button from SetupPage Step 3

### Current State

**SetupPage** (`frontend/src/pages/SetupPage.tsx`):
- **Print button JSX** (approx. line 373): `<button type="button" className="btn-secondary" onClick={printCodes}>🖨️ Print</button>`
- **`printCodes` function** (lines 155–206): Opens a new window, writes HTML with escaped code content, calls `window.print()`

**RegisterPage** (`frontend/src/pages/RegisterPage.tsx`):
- Has an identical Print button (approx. line 384) and `printCodes` function (lines 164–206)
- User request specifically says SetupPage, but RegisterPage has the same button pattern

### Code to Remove from SetupPage

1. **Remove the `printCodes` function** (lines 155–206 in SetupPage.tsx):

```tsx
const printCodes = () => {
  if (!recoveryCodes) { return; }
  const printWindow = window.open('', '', 'width=800,height=600');
  // ... entire function body ...
  printWindow.print();
  printWindow.close();
};
```

2. **Remove the Print button JSX** (approx. line 373):

```tsx
<button type="button" className="btn-secondary" onClick={printCodes}>
  🖨️ Print
</button>
```

3. **Remove the `escapeHtml` import** if it's only used by `printCodes`:
   - Line 4: `import { escapeHtml } from '@/utils/security';`
   - Check if `escapeHtml` is used elsewhere in SetupPage → It is NOT used elsewhere, so remove the import.

### RegisterPage Decision

The user mentioned only "step 3 of the setup wizard" (SetupPage). However for consistency, the same removal should be applied to RegisterPage's recovery codes display. **Recommendation:** Remove from both pages. But strictly per the user request, only SetupPage is required. The user should be asked or it should be noted as a recommended additional change. **For this spec: remove from SetupPage only, flag RegisterPage as follow-up.**

---

## Issue 3: Rework Instructions Modal to Quick-Start Guide

### Current State

**File:** `frontend/src/components/InstructionsModal.tsx`

Current content has **7 feature-description sections:**
1. Inventories — detailed explanation of what they are
2. Items — details about recording item properties  
3. Organizers — custom fields explanation
4. Warranty Notifications — auto-notification feature description
5. Sharing — sharing with household members
6. Reports — generate value reports
7. Backups — backup/restore explanation

Plus a "Pro tip" section at the bottom.

**Problem:** Too wordy, feature-focused, not action-oriented. User wants a concise quick-start guide with an order of operations.

### New Content Design: Quick-Start Guide

Replace all 7 sections + pro tip with **3 numbered steps:**

**Header:** Keep the welcome icon and "Welcome to Home Registry!" title.  
**Subtitle:** Change from "Here's a quick guide to help you get started" → "Get up and running in 3 simple steps"

**Step 1: Create Your First Inventory**
- Icon: `fas fa-boxes-stacked`
- Number badge: "1"
- Title: "Create Your First Inventory"
- Body: "Head to the Inventories page and click **Add Inventory**. Name it after a room or category — like \"Kitchen\" or \"Electronics.\""

**Step 2: Set Up Organizers**
- Icon: `fas fa-tags`
- Number badge: "2"
- Title: "Set Up Organizers"
- Body: "Before adding items, go to your inventory's **Organizers** tab. Add custom fields you want to track — like \"Serial Number,\" \"Condition,\" or \"Location.\""

**Step 3: Add Items**
- Icon: `fas fa-cube`
- Number badge: "3"
- Title: "Add Items"
- Body: "Click **Add Item** inside your inventory. Fill in the details — name, price, warranty date, and any custom organizer fields you created."

**Pro tip (kept, simplified):**
"You can always edit, reorganize, or add more inventories and organizers later. Start small and build from there!"

### New JSX Structure

```tsx
<div className="modal-body instructions-modal-body">
  <div className="instructions-steps">
    <div className="instructions-step">
      <div className="step-number-badge">1</div>
      <div className="instructions-step-icon">
        <i className="fas fa-boxes-stacked"></i>
      </div>
      <div className="instructions-text">
        <h3>Create Your First Inventory</h3>
        <p>
          Head to the Inventories page and click <strong>Add Inventory</strong>. 
          Name it after a room or category — like &quot;Kitchen&quot; or &quot;Electronics.&quot;
        </p>
      </div>
    </div>

    <div className="instructions-step-connector">
      <i className="fas fa-arrow-down"></i>
    </div>

    <div className="instructions-step">
      <div className="step-number-badge">2</div>
      <div className="instructions-step-icon">
        <i className="fas fa-tags"></i>
      </div>
      <div className="instructions-text">
        <h3>Set Up Organizers</h3>
        <p>
          Before adding items, go to your inventory&apos;s <strong>Organizers</strong> tab. 
          Add custom fields you want to track — like &quot;Serial Number,&quot; &quot;Condition,&quot; or &quot;Location.&quot;
        </p>
      </div>
    </div>

    <div className="instructions-step-connector">
      <i className="fas fa-arrow-down"></i>
    </div>

    <div className="instructions-step">
      <div className="step-number-badge">3</div>
      <div className="instructions-step-icon">
        <i className="fas fa-cube"></i>
      </div>
      <div className="instructions-text">
        <h3>Add Items</h3>
        <p>
          Click <strong>Add Item</strong> inside your inventory. Fill in the details — 
          name, price, warranty date, and any custom organizer fields you created.
        </p>
      </div>
    </div>
  </div>

  <div className="instructions-tip">
    <i className="fas fa-lightbulb"></i>
    <p>
      <strong>Tip:</strong> You can always edit, reorganize, or add more inventories 
      and organizers later. Start small and build from there!
    </p>
  </div>
</div>
```

### CSS Changes

Add new styles to `frontend/src/styles/instructions.css`:

```css
.instructions-steps {
  display: flex;
  flex-direction: column;
  gap: 0;
}

.instructions-step {
  display: flex;
  gap: 1rem;
  padding: 1rem 0;
  position: relative;
  align-items: flex-start;
}

.step-number-badge {
  width: 28px;
  height: 28px;
  min-width: 28px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--accent-color), var(--accent-dark));
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 700;
  font-size: 0.85rem;
}

.instructions-step-icon {
  width: 40px;
  height: 40px;
  min-width: 40px;
  border-radius: var(--radius-md);
  background: var(--bg-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--accent-color);
  font-size: 1rem;
}

.instructions-step-connector {
  display: flex;
  justify-content: center;
  padding-left: 14px; /* center under number badge */
  color: var(--text-tertiary);
  font-size: 0.75rem;
}
```

Remove old `.instructions-section` styles (or keep for backwards compat, but no longer used).

---

## Issue 4: Instructions Modal Close/Checkbox Persistence

### Current State

**File:** `frontend/src/components/InstructionsModal.tsx`

Current behavior:
1. Modal visibility: Shows when `settings?.settings_json.instructionsAcknowledged !== true`
2. Checkbox: "I have read and understand these instructions"
3. Button: "Get Started" — disabled until checkbox is checked
4. On click: Saves `instructionsAcknowledged: true` to user settings via API (`updateSettings`)
5. **No close button exists** — user must check + click to dismiss
6. Modal overlay has `cursor: default` in CSS (prevents dismiss on overlay click)
7. Once acknowledged, **never shows again** — `instructionsAcknowledged: true` persists in DB

**Settings storage:** `UserSettings.settings_json` is a `Record<string, unknown>` JSONB field stored server-side. The value `instructionsAcknowledged` is a boolean within this JSON.

### Desired Behavior

1. **Close button (X):** User can close the modal temporarily
2. **Session-based dismissal:** After closing with X, modal stays hidden for the rest of the session
3. **Re-appearance on next login:** Modal pops up again after their next login (page reload / new session)
4. **Permanent dismissal checkbox:** "I have read this and don't want to see it again"
5. **When checkbox is checked + confirmed:** Saves `instructionsAcknowledged: true` to backend → never shows again

### Architecture Design

**Two-tier dismissal system:**

- **Tier 1 (Session):** `sessionStorage` key `home_registry_instructions_dismissed = 'true'`. This is cleared on tab/browser close. Set when user clicks the X close button.
- **Tier 2 (Permanent):** `settings_json.instructionsAcknowledged = true` in the database via API. Set when user checks the checkbox and clicks confirm.

**Visibility logic:**
```
if (settings_json.instructionsAcknowledged === true) → HIDE (permanent)
else if (sessionStorage.instructionsSessionDismissed === 'true') → HIDE (session only)
else → SHOW
```

### Implementation Detail

#### Component Changes (`InstructionsModal.tsx`)

1. **Add close button** to the modal header:
```tsx
<button
  className="modal-close-btn"
  onClick={handleSessionDismiss}
  aria-label="Close"
  title="Close (will appear again next login)"
>
  <i className="fas fa-times"></i>
</button>
```

2. **Add `sessionDismissed` state:**
```tsx
const [sessionDismissed, setSessionDismissed] = useState(() => {
  return sessionStorage.getItem('home_registry_instructions_dismissed') === 'true';
});
```

3. **Session dismiss handler:**
```tsx
const handleSessionDismiss = () => {
  sessionStorage.setItem('home_registry_instructions_dismissed', 'true');
  setSessionDismissed(true);
};
```

4. **Update visibility check:**
```tsx
// Current:
if (!settings || isAcknowledged) { return null; }

// New:
if (!settings || isAcknowledged || sessionDismissed) { return null; }
```

5. **Update checkbox label:**
```tsx
// Current: "I have read and understand these instructions"
// New: "I have read this and don't want to see it again"
```

6. **Update scroll lock effect** to include `sessionDismissed`:
```tsx
useEffect(() => {
  if (!settings || isAcknowledged || sessionDismissed) {
    return undefined;
  }
  document.body.style.overflow = 'hidden';
  return () => { document.body.style.overflow = ''; };
}, [settings, isAcknowledged, sessionDismissed]);
```

7. **`handleConfirm` stays the same** — saves `instructionsAcknowledged: true` to API.

#### Session Storage Key

- Key: `home_registry_instructions_dismissed`
- Value: `'true'`
- Scope: Per-tab session (cleared when tab/browser is closed)
- Not set on login — only set when user explicitly clicks close

#### Login Flow

No backend changes needed. On login:
1. User logs in → `AuthContext.login()` fetches settings from API
2. Settings loaded → `instructionsAcknowledged` is `false` (or absent)
3. `sessionStorage` is empty (new session) → `sessionDismissed` is `false`
4. Modal shows
5. User closes with X → `sessionStorage` set → modal hidden rest of session
6. User logs out and back in → `sessionStorage` still exists (same tab) → modal stays hidden
7. User closes browser tab and reopens → `sessionStorage` cleared → modal shows again

**Edge case:** If user logs out and back in within the same tab, `sessionStorage` persists. This is acceptable behavior — the alternative is clearing `sessionStorage` on logout, which would be more aggressive. **Recommendation:** Clear the session dismissal on logout for true "pop up after next login" behavior.

#### Logout Hook (Optional Enhancement)

In `AuthContext.tsx`, add to the `logout` function:
```tsx
const logout = useCallback(() => {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(USER_KEY);
  sessionStorage.removeItem('home_registry_instructions_dismissed');
  setToken(null);
  setUser(null);
  setSettings(null);
}, []);
```

This ensures the modal re-appears even if the user logs out and back in within the same tab session.

### CSS for Close Button

The modal already has styling infrastructure. Add a close button style:

```css
.instructions-modal-header {
  position: relative;  /* already has other styles, just ensure this is set */
}

.instructions-close-btn {
  position: absolute;
  top: 1rem;
  right: 1rem;
  background: none;
  border: none;
  font-size: 1.25rem;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 0.25rem 0.5rem;
  border-radius: var(--radius-sm);
  transition: color 0.2s, background 0.2s;
}

.instructions-close-btn:hover {
  color: var(--text-primary);
  background: var(--bg-tertiary);
}
```

---

## Implementation Steps

### Step 1: Fix Toast on Auth Pages (Issue 1)

**File:** `frontend/src/App.tsx`

1. In the auth pages conditional branch (approx. line 93–143), wrap the return in a `<>` Fragment
2. Add `<Toast />` after `</Routes>` inside the Fragment
3. `Toast` is already imported at line 4

### Step 2: Remove Print Button from SetupPage (Issue 2)

**File:** `frontend/src/pages/SetupPage.tsx`

1. Remove the `printCodes` function (lines 155–206)
2. Remove the Print button from the JSX (the `<button>` with `🖨️ Print`)
3. Remove the `import { escapeHtml } from '@/utils/security';` import (line 4) — only used by `printCodes`

### Step 3: Rework Instructions Modal Content (Issue 3)

**File:** `frontend/src/components/InstructionsModal.tsx`

1. Replace the subtitle text in the header
2. Replace all 7 `instructions-section` divs with the 3-step quick-start layout  
3. Simplify the pro tip text
4. Keep the footer (checkbox + button) the same structure (checkbox text changes in Issue 4)

**File:** `frontend/src/styles/instructions.css`

1. Add `.instructions-steps`, `.instructions-step`, `.step-number-badge`, `.instructions-step-icon`, `.instructions-step-connector` styles
2. Keep existing `.instructions-section` styles (harmless if unused, or remove for cleanliness)

### Step 4: Add Close Button + Checkbox Persistence (Issue 4)

**File:** `frontend/src/components/InstructionsModal.tsx`

1. Add `sessionDismissed` state initialized from `sessionStorage`
2. Add `handleSessionDismiss` function
3. Update visibility checks (`return null` conditions)
4. Update `useEffect` for scroll lock
5. Add close button to modal header
6. Update checkbox label text to "I have read this and don't want to see it again"

**File:** `frontend/src/styles/instructions.css`

1. Add `.instructions-close-btn` styles
2. Ensure `.instructions-modal-header` has `position: relative`

**File:** `frontend/src/context/AuthContext.tsx`

1. Add `sessionStorage.removeItem('home_registry_instructions_dismissed')` to the `logout` function

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| `<Toast />` in auth branch renders outside main layout, may appear behind auth card | Low | Toast uses fixed positioning — will render correctly regardless of DOM position |
| `sessionStorage` cleared on incognito mode / private window close — good, but may confuse users in regular mode if they never close the tab | Low | Clearing on logout handles the main scenario; browser tab close handles the rest |
| Removing `printCodes` from SetupPage but not RegisterPage creates inconsistency | Medium | Note in spec: recommend removing from RegisterPage as follow-up; user specifically requested SetupPage only |
| Quick-start guide is very short — may not cover enough for some users | Low | The "Tip" section encourages exploration; the app itself is self-explanatory |
| `sessionStorage` key collision if user opens multiple tabs | None | `sessionStorage` is per-tab origin, no collision possible |
| `escapeHtml` import removal from SetupPage — must verify no other usages | Low | Grep confirms `escapeHtml` is only used in `printCodes` within SetupPage |
| Instructions modal close button accessibility | Low | Adding `aria-label` and keyboard support (button element is inherently keyboard-accessible) |

---

## Files to Modify

| File | Issues |
|------|--------|
| `frontend/src/App.tsx` | Issue 1 |
| `frontend/src/pages/SetupPage.tsx` | Issue 1 (already works, just needs Toast visible), Issue 2 |
| `frontend/src/components/InstructionsModal.tsx` | Issue 3, Issue 4 |
| `frontend/src/styles/instructions.css` | Issue 3, Issue 4 |
| `frontend/src/context/AuthContext.tsx` | Issue 4 (logout cleanup) |

---

## Reference: Working Toast Example

`RecoveryCodesSection.tsx` (used in Settings page, within main app layout where `<Toast />` is rendered):
```tsx
const handleCopyAll = async () => {
  if (!codes) return;
  try {
    await navigator.clipboard.writeText(codes.join('\n'));
    showToast('Codes copied to clipboard!', 'success');
  } catch {
    showToast('Failed to copy codes', 'error');
  }
};
```

This works because `RecoveryCodesSection` is used within the authenticated layout where `<Toast />` is mounted. The SetupPage/RegisterPage code is identical — the only difference is the missing `<Toast />` renderer.
