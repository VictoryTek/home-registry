# Post-Login Instructions Popup — Feature Specification

**Date**: 2025-02-15  
**Feature**: Post-login instructions popup / welcome modal  
**Status**: Specification Complete  

---

## 1. Executive Summary

Add a modal popup that appears after a user logs in (or on first authenticated page load) explaining how the Home Registry app works. The modal includes a checkbox "I have read and understand these instructions." Once confirmed, the preference is persisted in the database via the existing `settings_json` JSONB column in `user_settings`, and the popup never shows again for that user.

---

## 2. Current Architecture Analysis

### 2.1 Frontend Stack
- **Framework**: React 18.3 with TypeScript 5.6
- **Router**: react-router-dom 6.30
- **Styling**: Custom CSS with CSS variables (no UI library like MUI)
- **Build**: Vite 6.4
- **State Management**: React Context API (`AuthContext`, `AppContext`)

### 2.2 Auth Flow (Frontend)
1. User submits credentials on `LoginPage` → calls `useAuth().login()`
2. `AuthContext.login()` calls `authApi.login()` → stores token + user in localStorage
3. `AuthContext.login()` immediately fetches `authApi.getSettings(token)` → sets `settings` state
4. `LoginPage.handleSubmit()` calls `navigate('/')` on success → redirects to `InventoriesPage`
5. `App.tsx` wraps all authenticated routes in `<ProtectedRoute>` which checks `isAuthenticated`
6. On page load (existing session), `AuthContext.initAuth()` restores token, verifies profile, and fetches settings

### 2.3 User Settings System (Already Exists)
- **Database table**: `user_settings` with a `settings_json JSONB` column (migration 015)
- **Backend model**: `UserSettings` struct with `settings_json: serde_json::Value` field
- **Backend endpoints**:
  - `GET /api/auth/settings` → `get_user_settings` (auto-creates if missing via `get_or_create_user_settings`)
  - `PUT /api/auth/settings` → `update_user_settings` (partial update, merges fields)
- **Frontend API**: `authApi.getSettings(token)` and `authApi.updateSettings(token, data)`
- **Frontend state**: `AuthContext` exposes `settings`, `refreshSettings()`, and `updateSettings()`
- **Existing usage**: `settings_json` already stores `dismissedWarranties` for warranty notification dismissals — proving the JSONB pattern works for arbitrary per-user flags

### 2.4 Existing Modal Component
- Reusable `Modal` component (`frontend/src/components/Modal.tsx`) using React Portals
- Props: `isOpen`, `onClose`, `title`, `subtitle`, `children`, `footer`, `maxWidth`
- Full CSS in `frontend/src/styles/modal.css` with dark mode support, animations, responsive design
- Also available: `ConfirmModal` for simple confirm/cancel dialogs

### 2.5 Component Patterns
- Components are exported via barrel file `frontend/src/components/index.ts`
- CSS uses BEM-like naming with CSS custom properties from `variables.css`
- Icons use Font Awesome (`<i className="fas fa-...">`)

---

## 3. Design Decisions

### 3.1 No Database Migration Required
The existing `settings_json JSONB` column in `user_settings` can store the `instructionsAcknowledged` flag. This follows the exact same pattern used for `dismissedWarranties`. **No new migration is needed.**

### 3.2 No Backend Changes Required
The existing `PUT /api/auth/settings` endpoint already supports partial updates to `settings_json`. The frontend can update `settings_json.instructionsAcknowledged = true` using the existing `updateSettings()` function in `AuthContext`. **No backend code changes are needed.**

### 3.3 Frontend-Only Implementation
All changes are confined to the frontend:
1. New `InstructionsModal` component
2. Integration into `App.tsx` (shown after authentication, before main content interaction)
3. New CSS styles for the instructions modal
4. No new types needed (`settings_json` is already `Record<string, unknown>`)

### 3.4 When to Show the Modal
- Show the modal **after authentication is confirmed and settings are loaded**
- Check `settings?.settings_json?.instructionsAcknowledged !== true`
- Show on the **main app layout** (inside `AppContent`, after `ProtectedRoute` verification passes), NOT inside individual pages
- The modal overlays the app — the user can see the app behind the blurred backdrop
- The modal is **not dismissable by clicking outside or pressing Escape** — the user MUST check the checkbox and click the confirm button

### 3.5 UI/UX Design
- Use the existing `Modal` component as a base, but create a dedicated `InstructionsModal` with custom behavior (no close-on-overlay-click)
- Wider modal (`maxWidth: 650px`) to accommodate instruction text comfortably
- Scrollable content area for the instructions
- Checkbox at the bottom with label "I have read and understand these instructions"
- Confirm button ("Get Started") only enabled when checkbox is checked
- Friendly, welcoming tone in the instructions text
- Icons for each section to make it visually appealing

---

## 4. Implementation Specification

### 4.1 New File: `frontend/src/components/InstructionsModal.tsx`

```tsx
import { useState } from 'react';
import { createPortal } from 'react-dom';
import { useAuth } from '@/context/AuthContext';

export function InstructionsModal() {
  const { settings, updateSettings } = useAuth();
  const [isChecked, setIsChecked] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Don't show if settings not loaded yet or already acknowledged
  const isAcknowledged = settings?.settings_json?.instructionsAcknowledged === true;
  
  if (!settings || isAcknowledged) {
    return null;
  }

  const handleConfirm = async () => {
    if (!isChecked) return;
    
    setIsSaving(true);
    const success = await updateSettings({
      settings_json: {
        ...settings.settings_json,
        instructionsAcknowledged: true,
      },
    });
    
    if (!success) {
      // If save fails, still allow user to proceed (localStorage fallback could be added)
      console.error('Failed to save instructions acknowledgment');
    }
    setIsSaving(false);
  };

  const modalContent = (
    <div className="modal-overlay active instructions-modal-overlay">
      <div 
        className="modal-content instructions-modal" 
        style={{ maxWidth: '650px' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-header instructions-modal-header">
          <div className="instructions-welcome-icon">
            <i className="fas fa-home"></i>
          </div>
          <h2 className="modal-title">Welcome to Home Registry!</h2>
          <p className="modal-subtitle">Here's a quick guide to help you get started</p>
        </div>
        
        <div className="modal-body instructions-modal-body">
          <div className="instructions-section">
            <div className="instructions-icon">
              <i className="fas fa-boxes-stacked"></i>
            </div>
            <div className="instructions-text">
              <h3>Inventories</h3>
              <p>
                Think of inventories as containers for your belongings. You might create one 
                for each room in your home (Kitchen, Garage, Bedroom) or for different 
                categories (Electronics, Tools, Documents). Create as many as you need!
              </p>
            </div>
          </div>

          <div className="instructions-section">
            <div className="instructions-icon">
              <i className="fas fa-cube"></i>
            </div>
            <div className="instructions-text">
              <h3>Items</h3>
              <p>
                Inside each inventory, you can add items — the actual things you own. 
                For each item, you can record details like its name, description, purchase 
                date, price, warranty expiration, and more. The more details you add, the 
                more useful your registry becomes!
              </p>
            </div>
          </div>

          <div className="instructions-section">
            <div className="instructions-icon">
              <i className="fas fa-tags"></i>
            </div>
            <div className="instructions-text">
              <h3>Organizers</h3>
              <p>
                Organizers let you create custom ways to categorize and filter your items 
                within an inventory. For example, you could create an organizer called 
                "Condition" with options like "New," "Good," or "Needs Repair." Each 
                inventory can have its own set of organizers.
              </p>
            </div>
          </div>

          <div className="instructions-section">
            <div className="instructions-icon">
              <i className="fas fa-bell"></i>
            </div>
            <div className="instructions-text">
              <h3>Warranty Notifications</h3>
              <p>
                When you add warranty expiration dates to your items, Home Registry will 
                automatically notify you when warranties are about to expire or have already 
                expired. Never miss a warranty claim again!
              </p>
            </div>
          </div>

          <div className="instructions-section">
            <div className="instructions-icon">
              <i className="fas fa-share-nodes"></i>
            </div>
            <div className="instructions-text">
              <h3>Sharing</h3>
              <p>
                You can share your inventories with other users in your household. Choose 
                whether they can just view your inventory or also make changes to it. 
                Great for families managing a home together!
              </p>
            </div>
          </div>

          <div className="instructions-section">
            <div className="instructions-icon">
              <i className="fas fa-chart-bar"></i>
            </div>
            <div className="instructions-text">
              <h3>Reports</h3>
              <p>
                Generate reports to see the total value of your belongings, breakdowns 
                by category, and other useful statistics. Perfect for insurance purposes 
                or simply keeping track of what you own.
              </p>
            </div>
          </div>

          <div className="instructions-section">
            <div className="instructions-icon">
              <i className="fas fa-database"></i>
            </div>
            <div className="instructions-text">
              <h3>Backups</h3>
              <p>
                Regularly back up your data to keep it safe. You can create backups from 
                the Settings page and restore them at any time. Your data is important — 
                protect it!
              </p>
            </div>
          </div>

          <div className="instructions-tip">
            <i className="fas fa-lightbulb"></i>
            <p>
              <strong>Pro tip:</strong> Start by creating your first inventory (like "Living Room" 
              or "Kitchen"), then add a few items to get familiar with how everything works. 
              You can always edit or reorganize later!
            </p>
          </div>
        </div>

        <div className="modal-footer instructions-modal-footer">
          <label className="instructions-checkbox-label">
            <input
              type="checkbox"
              checked={isChecked}
              onChange={(e) => setIsChecked(e.target.checked)}
              className="instructions-checkbox"
            />
            <span>I have read and understand these instructions</span>
          </label>
          <button
            className="btn btn-primary instructions-confirm-btn"
            disabled={!isChecked || isSaving}
            onClick={handleConfirm}
          >
            {isSaving ? (
              <>
                <span className="btn-spinner"></span>
                Saving...
              </>
            ) : (
              <>
                <i className="fas fa-rocket"></i>
                Get Started
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );

  return createPortal(modalContent, document.body);
}
```

### 4.2 New File: `frontend/src/styles/instructions.css`

```css
/* Instructions Modal Styles */

.instructions-modal-overlay {
  /* Override: prevent closing by clicking outside */
  cursor: default;
}

.instructions-modal {
  max-height: 85vh;
}

.instructions-modal-header {
  text-align: center;
  padding: 2rem 2rem 1.5rem;
}

.instructions-welcome-icon {
  width: 64px;
  height: 64px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--accent-color), var(--accent-dark));
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0 auto 1rem;
  font-size: 1.75rem;
  color: white;
  box-shadow: 0 4px 12px rgba(249, 115, 22, 0.3);
}

.instructions-modal-body {
  padding: 1rem 2rem 1.5rem;
  max-height: 50vh;
  overflow-y: auto;
}

.instructions-section {
  display: flex;
  gap: 1rem;
  padding: 1rem 0;
  border-bottom: 1px solid var(--border-light);
}

.instructions-section:last-of-type {
  border-bottom: none;
}

.instructions-icon {
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

.instructions-text h3 {
  font-size: 0.95rem;
  font-weight: 700;
  color: var(--text-primary);
  margin-bottom: 0.25rem;
}

.instructions-text p {
  font-size: 0.85rem;
  color: var(--text-secondary);
  line-height: 1.5;
  margin: 0;
}

.instructions-tip {
  display: flex;
  gap: 0.75rem;
  padding: 1rem;
  margin-top: 1rem;
  background: linear-gradient(135deg, rgba(249, 115, 22, 0.08), rgba(249, 115, 22, 0.03));
  border: 1px solid rgba(249, 115, 22, 0.15);
  border-radius: var(--radius-md);
  align-items: flex-start;
}

.instructions-tip > i {
  color: var(--accent-color);
  font-size: 1.1rem;
  margin-top: 0.1rem;
}

.instructions-tip p {
  font-size: 0.85rem;
  color: var(--text-secondary);
  line-height: 1.5;
  margin: 0;
}

.instructions-modal-footer {
  flex-direction: column;
  align-items: stretch;
  gap: 1rem;
  padding: 1.5rem 2rem 2rem;
  border-top: 1px solid var(--border-light);
}

.instructions-checkbox-label {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  cursor: pointer;
  font-size: 0.9rem;
  color: var(--text-primary);
  font-weight: 500;
}

.instructions-checkbox {
  width: 18px;
  height: 18px;
  min-width: 18px;
  accent-color: var(--accent-color);
  cursor: pointer;
}

.instructions-confirm-btn {
  width: 100%;
  padding: 0.875rem 1.5rem;
  font-size: 1rem;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
}

.instructions-confirm-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Responsive */
@media (max-width: 768px) {
  .instructions-modal {
    max-height: 95vh;
    width: 95%;
  }

  .instructions-modal-body {
    max-height: 45vh;
    padding: 1rem;
  }

  .instructions-section {
    gap: 0.75rem;
  }

  .instructions-modal-header {
    padding: 1.5rem 1.5rem 1rem;
  }

  .instructions-modal-footer {
    padding: 1rem 1.5rem 1.5rem;
  }
}
```

### 4.3 Modify: `frontend/src/components/index.ts`

Add the new export:

```typescript
export { InstructionsModal } from './InstructionsModal';
```

### 4.4 Modify: `frontend/src/styles/index.css`

Add the new CSS import:

```css
@import './instructions.css';
```

### 4.5 Modify: `frontend/src/App.tsx`

Import and render `InstructionsModal` inside `AppContent`, after the sidebar and before `<main>`, so it appears when the user is authenticated:

```tsx
// Add import
import { Sidebar, Toast, InstructionsModal } from '@/components';

// In the authenticated layout return (the else branch with Sidebar):
return (
  <>
    <Sidebar currentPage={getCurrentPage()} onNavigate={handleNavigate} />
    <InstructionsModal />
    <main className="main-content">
      {/* ... routes ... */}
    </main>
    <Toast />
  </>
);
```

The `InstructionsModal` component internally checks `settings?.settings_json?.instructionsAcknowledged` and renders nothing if already acknowledged or if settings aren't loaded yet. This means:
- It automatically shows after login (settings are fetched during login flow)
- It automatically shows on page reload if not yet acknowledged
- It never shows once acknowledged
- It does not block the login/setup/register pages (those render in a separate branch of `AppContent`)

---

## 5. Data Flow

### 5.1 First Login (Instructions Not Yet Acknowledged)

```
User logs in
  → AuthContext.login() stores token, fetches settings
  → settings.settings_json = {} (no instructionsAcknowledged key)
  → Navigate to "/" 
  → AppContent renders authenticated layout
  → InstructionsModal checks: settings_json.instructionsAcknowledged !== true → shows modal
  → User reads instructions, checks checkbox, clicks "Get Started"
  → InstructionsModal calls updateSettings({ settings_json: { ...existing, instructionsAcknowledged: true } })
  → PUT /api/auth/settings merges into JSONB
  → AuthContext.settings updated → InstructionsModal re-renders → isAcknowledged = true → returns null
```

### 5.2 Subsequent Logins

```
User logs in (or refreshes page)
  → settings fetched → settings_json.instructionsAcknowledged === true
  → InstructionsModal returns null (not shown)
```

### 5.3 Settings Reset Scenario

If an admin or the user themselves clears `settings_json`, the modal will reappear — this is correct behavior since the preference has been lost.

---

## 6. Files to Create

| File | Purpose |
|------|---------|
| `frontend/src/components/InstructionsModal.tsx` | New modal component |
| `frontend/src/styles/instructions.css` | Styles for the instructions modal |

## 7. Files to Modify

| File | Change |
|------|--------|
| `frontend/src/components/index.ts` | Add `InstructionsModal` export |
| `frontend/src/styles/index.css` | Add `@import './instructions.css'` |
| `frontend/src/App.tsx` | Import `InstructionsModal`, render in authenticated layout |

## 8. Files NOT Modified (No Changes Needed)

| Layer | Reason |
|-------|--------|
| Backend Rust code | Existing `PUT /api/auth/settings` handles `settings_json` updates |
| Database migrations | `settings_json JSONB` column already exists |
| TypeScript types | `settings_json: Record<string, unknown>` already flexible |
| API service | `authApi.updateSettings()` already supports partial `settings_json` |
| AuthContext | `updateSettings()` already merges and persists settings |

---

## 9. Instructions Content (Final Copy)

The modal displays these sections:

1. **Inventories** — Containers for belongings (rooms, categories)
2. **Items** — Individual things you own, with details (price, warranty, etc.)
3. **Organizers** — Custom categorization/filtering per inventory
4. **Warranty Notifications** — Automatic alerts for expiring warranties
5. **Sharing** — Share inventories with household members (view or edit)
6. **Reports** — Value totals, category breakdowns, insurance-ready stats
7. **Backups** — Create and restore data backups from Settings
8. **Pro Tip** — Start with one inventory, add a few items, learn as you go

---

## 10. Testing Plan

### 10.1 Manual Testing
1. Log in with a user that has NOT acknowledged instructions → modal appears
2. Verify checkbox is unchecked and "Get Started" button is disabled
3. Check the checkbox → button becomes enabled
4. Click "Get Started" → modal disappears, user sees dashboard
5. Refresh page → modal does NOT reappear
6. Log out and log back in → modal does NOT reappear
7. Test with a NEW user → modal DOES appear
8. Verify dark mode styling works correctly
9. Verify mobile/responsive layout
10. Test: uncheck checkbox after checking → button disables again

### 10.2 Edge Cases
- Settings fetch fails → modal doesn't show (settings is null), user can still use app
- Settings update fails → error logged, modal stays open, user can retry
- Multiple rapid clicks on "Get Started" → isSaving prevents duplicate requests
- User with existing `settings_json` data (e.g., dismissedWarranties) → spread preserves existing data

---

## 11. Implementation Order

1. Create `frontend/src/styles/instructions.css`
2. Create `frontend/src/components/InstructionsModal.tsx`
3. Add export to `frontend/src/components/index.ts`
4. Add CSS import to `frontend/src/styles/index.css`
5. Modify `frontend/src/App.tsx` to import and render `InstructionsModal`
6. Build and test: `cd frontend && npm run build`

---

## 12. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `settings_json` overwrite loses other data | Low | High | Spread existing `settings_json` in update |
| Modal blocks usage if settings fail to load | Low | Medium | Modal only shows if settings exist AND flag absent |
| Font Awesome icons not available | Very Low | Low | Icons are already loaded globally |
| CSS conflicts with existing modal styles | Low | Low | All new classes prefixed with `instructions-` |

---

## 13. Future Enhancements (Out of Scope)

- **Version-aware instructions**: Track `instructionsVersion` in settings_json to re-show when app features change significantly
- **Reset option in Settings**: Let users re-read instructions from the Settings page
- **Animated walkthrough**: Step-by-step guided tour instead of a single modal
- **Localization**: Multi-language instruction text
