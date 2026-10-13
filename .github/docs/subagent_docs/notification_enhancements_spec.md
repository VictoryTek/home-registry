# Notification Enhancements Specification

**Date**: February 14, 2026  
**Project**: Home Registry - Notification System Enhancements  
**Frontend**: TypeScript + React  
**Location**: `frontend/` directory

---

## Executive Summary

This specification documents three enhancements to the Home Registry warranty notification system:

1. **Navigate to Item Details Modal** — Clicking a notification opens the item details modal for the specific item (not just the inventory page)
2. **Badge on Item Cards** — Visual indicator on item cards when there are active notifications about that item
3. **Clear Notifications** — Functionality to dismiss/clear notifications (mark warranty as acknowledged)

All features are **frontend-only** with **client-side** notification management. Notifications are computed from items with `warranty_expiry` dates using the `checkWarrantyNotifications()` utility function.

---

## Current State Analysis

### Notification System Architecture

#### Data Flow
```
Items (from API) → checkWarrantyNotifications() → WarrantyNotification[] → AppContext → Components
```

#### Type Definitions

**File**: `frontend/src/utils/notifications.ts`

```typescript
export interface WarrantyNotification {
  id: number;              // Item ID
  itemName: string;        // Item name
  inventoryId: number;     // Inventory ID
  warrantyExpiry: string;  // ISO date string
  daysUntilExpiry: number; // Calculated days
  status: 'expired' | 'expiring-soon' | 'expiring-this-month';
}
```

**Status Logic**:
- `expired`: `daysUntilExpiry < 0`
- `expiring-soon`: `0 <= daysUntilExpiry <= 7`
- `expiring-this-month`: `8 <= daysUntilExpiry <= 30`

#### Current Implementation

**Notification Computing** (`frontend/src/utils/notifications.ts`, lines 23-73):
```typescript
export function checkWarrantyNotifications(
  items: Item[],
  daysThreshold = 30
): WarrantyNotification[]
```

- Filters items with `warranty_expiry` date
- Calculates days until expiry
- Returns sorted array (expired first, then by days)

**Context Storage** (`frontend/src/context/AppContext.tsx`, lines 33, 64-73):
```typescript
const [warrantyNotifications, setWarrantyNotifications] = useState<WarrantyNotification[]>([]);

const checkNotifications = useCallback(() => {
  const notifications = checkWarrantyNotifications(items);
  setWarrantyNotifications(notifications);
}, [items]);

// Auto-check notifications when items change
useEffect(() => {
  checkNotifications();
}, [checkNotifications]);
```

**Key Points**:
- Notifications are **computed client-side** whenever `items` state changes
- No backend API for notifications (no persistence, no dismissal)
- Notifications are **read-only** — no mark as read, no dismissal
- Badge count in header shows `warrantyNotifications.length`

---

## Enhancement 1: Navigate to Item Details Modal

### Current Behavior

**File**: `frontend/src/pages/NotificationsPage.tsx`, lines 26-28

```typescript
const handleNotificationClick = (notification: WarrantyNotification) => {
  navigate(`/inventory/${notification.inventoryId}`);
};
```

**Problem**: Clicking a notification navigates to the inventory page but does NOT open the item details modal. Users must:
1. Navigate to inventory page
2. Scroll to find the item
3. Click "View Details" button

This requires 3 steps when users expect 1 step (direct item details).

### Desired Behavior

Clicking a notification should:
1. Navigate to the inventory page (if not already there)
2. **Automatically open the item details modal** for the specific item
3. Pre-populate modal with item data

### Implementation Analysis

#### Item Details Modal Pattern

**File**: `frontend/src/pages/InventoryDetailPage.tsx`, lines 39-40, 190-200, 717-875

**Modal State**:
```typescript
const [viewingItem, setViewingItem] = useState<Item | null>(null);
const [viewingItemOrganizerValues, setViewingItemOrganizerValues] = useState<ItemOrganizerValueWithDetails[]>([]);
```

**Open Handler**:
```typescript
const handleViewItem = async (item: Item) => {
  setViewingItem(item);
  setViewingItemOrganizerValues([]);
  if (item.id) {
    try {
      const result = await itemApi.getOrganizerValues(item.id);
      if (result.success && result.data) {
        setViewingItemOrganizerValues(result.data);
      }
    } catch {
      // Organizer values are optional, proceed without them
    }
  }
};
```

**Modal Render** (lines 717-875):
```typescript
<Modal
  isOpen={viewingItem !== null}
  onClose={() => {
    setViewingItem(null);
    setViewingItemOrganizerValues([]);
  }}
  title={viewingItem?.name ?? 'Item Details'}
  // ... modal content renders item details
>
```

#### Navigation with State

**React Router** supports navigation with state:
```typescript
navigate('/path', { state: { key: 'value' } });
```

And reading state in the target component:
```typescript
import { useLocation } from 'react-router-dom';
const location = useLocation();
const state = location.state as { itemId?: number };
```

### Proposed Solution

#### Step 1: Update Notification Click Handler

**File**: `frontend/src/pages/NotificationsPage.tsx`, line 26-28

**Current**:
```typescript
const handleNotificationClick = (notification: WarrantyNotification) => {
  navigate(`/inventory/${notification.inventoryId}`);
};
```

**Proposed**:
```typescript
const handleNotificationClick = (notification: WarrantyNotification) => {
  navigate(`/inventory/${notification.inventoryId}`, { 
    state: { openItemId: notification.id } 
  });
};
```

**Similarly update** `frontend/src/components/WarrantyNotificationBanner.tsx`, line 21-23.

#### Step 2: Handle Navigation State in InventoryDetailPage

**File**: `frontend/src/pages/InventoryDetailPage.tsx`

**Add imports** (line 2):
```typescript
import { useParams, useNavigate, useLocation } from 'react-router-dom';
```

**Add location state handling** (after line 26):
```typescript
const location = useLocation();
const locationState = location.state as { openItemId?: number } | null;
```

**Add effect to auto-open modal** (after `loadInventoryDetail` is called in useEffect):
```typescript
// Auto-open item details modal if navigated from notification
useEffect(() => {
  if (locationState?.openItemId && items.length > 0) {
    const item = items.find((i) => i.id === locationState.openItemId);
    if (item) {
      void handleViewItem(item);
      // Clear navigation state to prevent re-opening on next visit
      window.history.replaceState({}, document.title);
    }
  }
}, [items, locationState?.openItemId]);
```

**Note**: `window.history.replaceState({}, document.title)` clears the navigation state so the modal doesn't auto-open again when returning to the page via back button.

#### Step 3: Test Edge Cases

1. **Item not found** (deleted between notification and click): Modal doesn't open, no error
2. **Multiple notifications for same item**: Opens modal once
3. **Back button behavior**: Modal closes, doesn't re-open
4. **Direct URL navigation**: Works normally (no auto-open)

---

## Enhancement 2: Badge on Item Cards

### Current Behavior

**File**: `frontend/src/pages/InventoryDetailPage.tsx`, lines 410-519

Item cards render with:
- Item name (title)
- Category badge (if present)
- Description
- Details (location, quantity, purchase date, price, warranty)
- Footer buttons (View, Edit, Delete)

**No visual indicator** for active notifications.

### Desired Behavior

Item cards should display a **visual badge/indicator** when there are active warranty notifications for that item.

**Badge Requirements**:
- Positioned at top-right of card header
- Color-coded by severity (red = expired, orange = expiring soon, blue = expiring this month)
- Icon + short text (e.g., "Expired", "7 days")
- Consistent with existing badge styles

### Implementation Analysis

#### Existing Badge Patterns

**File**: `frontend/src/styles/organizers.css`, lines 135-159

```css
.badge {
  display: inline-flex;
  align-items: center;
  padding: 0.25rem 0.5rem;
  font-size: 0.75rem;
  font-weight: 500;
  border-radius: var(--radius-sm);
  text-transform: uppercase;
  letter-spacing: 0.025em;
}

.badge-primary { background: var(--primary-bg); color: var(--primary-color); }
.badge-secondary { background: var(--bg-secondary); color: var(--text-secondary); }
.badge-warning { background: rgba(245, 158, 11, 0.15); color: #d97706; }
```

**Example Usage** (`frontend/src/pages/InventoriesPage.tsx`, line 312):
```tsx
<span className="badge badge-shared">
  <i className="fas fa-share-nodes"></i>
  Shared
</span>
```

**Notification Badge in Header** (`frontend/src/components/Header.tsx`, lines 40-62):
```tsx
{notificationCount > 0 && (
  <span
    style={{
      position: 'absolute',
      top: '-4px',
      right: '-4px',
      background: 'var(--danger-color)',
      color: 'white',
      borderRadius: '50%',
      padding: '2px 6px',
      fontSize: '0.65rem',
      fontWeight: 'bold',
      // ...
    }}
  >
    {notificationCount > 99 ? '99+' : notificationCount}
  </span>
)}
```

#### Item Card Structure

**File**: `frontend/src/pages/InventoryDetailPage.tsx`, lines 413-519

```tsx
<div key={item.id} className="item-card">
  <div className="item-card-header">
    <h3 className="item-card-title">{item.name}</h3>
    {item.category && (
      <span className="item-card-category">{item.category}</span>
    )}
  </div>
  <div className="item-card-body">
    {/* ... details ... */}
  </div>
  <div className="item-card-footer">
    {/* ... buttons ... */}
  </div>
</div>
```

**CSS** (`frontend/src/styles/cards.css`, lines 203-278):
- `.item-card`: Main container
- `.item-card-header`: Title and category section
- `.item-card-title`: Item name
- `.item-card-category`: Category badge

### Proposed Solution

#### Step 1: Add CSS for Notification Badges

**File**: `frontend/src/styles/cards.css`

**Add after line 278** (after `.item-card-footer`):

```css
/* Item Notification Badge */
.item-notification-badge {
  position: absolute;
  top: 1rem;
  right: 1rem;
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 0.625rem;
  font-size: 0.75rem;
  font-weight: 600;
  border-radius: 12px;
  z-index: 1;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  transition: all 0.2s ease;
}

.item-notification-badge i {
  font-size: 0.7rem;
}

.item-notification-badge.status-expired {
  background: rgba(239, 68, 68, 0.15);
  color: #dc2626;
  border: 1.5px solid rgba(239, 68, 68, 0.3);
}

.item-notification-badge.status-expiring-soon {
  background: rgba(245, 158, 11, 0.15);
  color: #d97706;
  border: 1.5px solid rgba(245, 158, 11, 0.3);
}

.item-notification-badge.status-expiring-this-month {
  background: rgba(59, 130, 246, 0.15);
  color: #3b82f6;
  border: 1.5px solid rgba(59, 130, 246, 0.3);
}

/* Make item-card position relative for absolute badge positioning */
.item-card {
  position: relative;
}
```

**Note**: `.item-card` already has `position: relative` (line 213) — no change needed.

#### Step 2: Add Helper Function to Find Notification

**File**: `frontend/src/pages/InventoryDetailPage.tsx`

**Add after line 26** (in component body):

```typescript
const getItemNotification = (itemId: number | undefined) => {
  if (!itemId) return null;
  return warrantyNotifications.find((n) => n.id === itemId);
};
```

**Requires access to** `warrantyNotifications` from AppContext. Update line 29:

```typescript
const { showToast, setItems: setGlobalItems, warrantyNotifications } = useApp();
```

#### Step 3: Render Badge on Item Cards

**File**: `frontend/src/pages/InventoryDetailPage.tsx`, lines 413-519

**Update item card render** (after line 413, before `.item-card-header`):

```tsx
<div key={item.id} className="item-card">
  {/* Notification Badge */}
  {(() => {
    const notification = getItemNotification(item.id);
    if (!notification) return null;
    
    const { status, daysUntilExpiry } = notification;
    const statusClass = `status-${status}`;
    const icon = status === 'expired' 
      ? 'fa-exclamation-circle' 
      : status === 'expiring-soon'
      ? 'fa-exclamation-triangle'
      : 'fa-info-circle';
    
    const text = status === 'expired'
      ? 'Expired'
      : status === 'expiring-soon'
      ? `${daysUntilExpiry}d`
      : `${daysUntilExpiry}d`;
    
    return (
      <span className={`item-notification-badge ${statusClass}`}>
        <i className={`fas ${icon}`}></i>
        {text}
      </span>
    );
  })()}
  
  <div className="item-card-header">
    {/* ... existing header content ... */}
  </div>
  {/* ... rest of card ... */}
</div>
```

**Alternative: Extract to Component** (cleaner):

```typescript
const NotificationBadge = ({ itemId }: { itemId: number | undefined }) => {
  const notification = getItemNotification(itemId);
  if (!notification) return null;
  
  const { status, daysUntilExpiry } = notification;
  const statusClass = `status-${status}`;
  const icon = status === 'expired' 
    ? 'fa-exclamation-circle' 
    : status === 'expiring-soon'
    ? 'fa-exclamation-triangle'
    : 'fa-info-circle';
  
  const text = status === 'expired'
    ? 'Expired'
    : `${daysUntilExpiry}d`;
  
  return (
    <span className={`item-notification-badge ${statusClass}`}>
      <i className={`fas ${icon}`}></i>
      {text}
    </span>
  );
};

// In render:
<div key={item.id} className="item-card">
  <NotificationBadge itemId={item.id} />
  {/* ... rest of card ... */}
</div>
```

#### Step 4: Consider Accessibility

Add `aria-label` and `title` for screen readers:

```tsx
<span 
  className={`item-notification-badge ${statusClass}`}
  title={getNotificationMessage(notification)}
  aria-label={`Warranty notification: ${getNotificationMessage(notification)}`}
>
  <i className={`fas ${icon}`}></i>
  {text}
</span>
```

Requires importing `getNotificationMessage` from `@/utils/notifications`.

---

## Enhancement 3: Clear/Dismiss Notifications

### Current Behavior

**Problem**: No way to dismiss or "acknowledge" notifications. Users see the same expired warranty notifications forever, even if they:
- Replaced the item
- Renewed the warranty
- Don't care about this specific notification

**Use Cases for Dismissal**:
1. **Acknowledged expiry** — User knows warranty expired, doesn't need constant reminder
2. **Renewed warranty** — User renewed warranty but hasn't updated item yet
3. **Sold/disposed item** — Item no longer in inventory but still shows up
4. **False positive** — Notification is incorrect or irrelevant

### Desired Behavior

Users should be able to:
1. **Dismiss individual notifications** — "X" button on notification card
2. **Dismiss all notifications** — "Clear All" button at top of page
3. **Persistence** — Dismissed notifications stay dismissed across sessions
4. **Re-trigger** — Updating item's warranty date clears dismissal

### Implementation Analysis

#### Current Architecture Limitations

**Problem**: Notifications are **computed on-the-fly** from items:
- No notification persistence in backend
- No notification ID (uses item ID)
- No "dismissed" state tracked

**Options**:

##### Option A: Client-Side Dismissal (Simplest)
- Store dismissed item IDs in `localStorage`
- Filter out dismissed notifications when rendering
- No backend changes required
- **Limitation**: Per-browser only, not synced across devices

##### Option B: Backend Dismissal State (Robust)
- Add `dismissed_notifications` table in backend
- Track `(user_id, item_id, dismissed_at)` tuples
- Filter notifications via API call
- **Limitation**: Requires backend changes (outside scope of frontend-only enhancements)

##### Option C: Hybrid Approach (Recommended)
- Store dismissal state in UserSettings JSON field
- Update via existing `/api/auth/settings` endpoint
- Sync across devices (tied to user account)
- No new tables/migrations required
- **Best of both worlds**

### Proposed Solution: Hybrid Approach (Option C)

#### Architecture

```
User dismisses notification 
  → Update localStorage (immediate UI)
  → Call authApi.updateSettings({ settings_json: { dismissedWarranties: [...] } })
  → Reload user settings
  → Filter notifications in checkWarrantyNotifications()
```

#### Backend Support (Already Exists)

**File**: `src/api/auth.rs`, `src/db/mod.rs`

**Endpoint**: `PUT /api/auth/settings`

**Type** (`frontend/src/types/index.ts`, line 237-248):
```typescript
export interface UserSettings {
  id: string;
  user_id: string;
  theme: string;
  default_inventory_id?: number;
  items_per_page: number;
  date_format: string;
  currency: string;
  notifications_enabled: boolean;
  settings_json: Record<string, unknown>; // ← Store dismissals here
  created_at: string;
  updated_at: string;
}
```

**API** (`frontend/src/services/api.ts`, line 530-545):
```typescript
async updateSettings(data: UpdateUserSettingsRequest): Promise<ApiResponse<UserSettings>>
```

Where `UpdateUserSettingsRequest` includes `settings_json?: Record<string, unknown>`.

#### Step 1: Define Dismissal Data Structure

**Add to** `frontend/src/types/index.ts`:

```typescript
// Dismissed warranty notifications (stored in UserSettings.settings_json)
export interface DismissedWarranties {
  [itemId: number]: {
    dismissedAt: string; // ISO timestamp
    warrantyExpiry: string; // Date at dismissal (to detect changes)
  };
}
```

**Stored in UserSettings.settings_json**:
```json
{
  "dismissedWarranties": {
    "123": {
      "dismissedAt": "2026-02-14T10:30:00Z",
      "warrantyExpiry": "2025-12-31"
    }
  }
}
```

#### Step 2: Add Dismissal Functions to AuthContext

**File**: `frontend/src/context/AuthContext.tsx`

**Add state and functions**:

```typescript
// Helper to get dismissed warranties from settings
const getDismissedWarranties = useCallback((): DismissedWarranties => {
  return (settings?.settings_json?.dismissedWarranties as DismissedWarranties) ?? {};
}, [settings]);

// Dismiss a single notification
const dismissNotification = useCallback(async (itemId: number, warrantyExpiry: string) => {
  const dismissed = getDismissedWarranties();
  dismissed[itemId] = {
    dismissedAt: new Date().toISOString(),
    warrantyExpiry,
  };
  
  // Update settings
  const result = await authApi.updateSettings({
    settings_json: {
      ...settings?.settings_json,
      dismissedWarranties: dismissed,
    },
  });
  
  if (result.success && result.data) {
    setSettings(result.data);
    return true;
  }
  return false;
}, [settings, getDismissedWarranties]);

// Clear all dismissals
const clearAllDismissals = useCallback(async () => {
  const result = await authApi.updateSettings({
    settings_json: {
      ...settings?.settings_json,
      dismissedWarranties: {},
    },
  });
  
  if (result.success && result.data) {
    setSettings(result.data);
    return true;
  }
  return false;
}, [settings]);

// Add to AuthContext interface and provider value
interface AuthContextType {
  // ... existing fields ...
  dismissNotification: (itemId: number, warrantyExpiry: string) => Promise<boolean>;
  clearAllDismissals: () => Promise<boolean>;
  getDismissedWarranties: () => DismissedWarranties;
}
```

#### Step 3: Filter Dismissed Notifications

**File**: `frontend/src/utils/notifications.ts`

**Update** `checkWarrantyNotifications` signature and logic:

```typescript
export function checkWarrantyNotifications(
  items: Item[],
  dismissedWarranties: DismissedWarranties = {},
  daysThreshold = 30
): WarrantyNotification[] {
  const notifications: WarrantyNotification[] = [];
  const now = new Date();
  now.setHours(0, 0, 0, 0);

  items.forEach((item) => {
    if (!item.warranty_expiry || !item.id) {
      return;
    }
    
    // Check if dismissed
    const dismissed = dismissedWarranties[item.id];
    if (dismissed) {
      // If warranty date hasn't changed, keep dismissed
      if (dismissed.warrantyExpiry === item.warranty_expiry) {
        return; // Skip this notification
      }
      // If warranty date changed, show notification again (user updated it)
    }

    // ... rest of existing logic ...
  });

  return notifications.sort(/* ... */);
}
```

**Update** `frontend/src/context/AppContext.tsx` to pass dismissed warranties:

```typescript
const checkNotifications = useCallback(() => {
  const dismissedWarranties = getDismissedWarranties();
  const notifications = checkWarrantyNotifications(items, dismissedWarranties);
  setWarrantyNotifications(notifications);
}, [items, getDismissedWarranties]);
```

**Requires**: AppContext to access AuthContext (add `useAuth()` hook):

```typescript
import { useAuth } from '@/context/AuthContext';

// In AppProvider:
const { settings } = useAuth();
const getDismissedWarranties = useCallback((): DismissedWarranties => {
  return (settings?.settings_json?.dismissedWarranties as DismissedWarranties) ?? {};
}, [settings]);
```

**Issue**: Circular dependency (AppContext → AuthContext → AppContext). **Solution**: Pass `dismissedWarranties` as prop or use a separate context/store.

**Alternative**: Keep dismissal in AuthContext, expose filtered notifications via AuthContext instead of AppContext.

**Simpler Approach**: Keep dismissal logic in component level (NotificationsPage) rather than global context.

#### Step 4: Add Dismiss UI to NotificationsPage

**File**: `frontend/src/pages/NotificationsPage.tsx`

**Add dismiss handlers**:

```typescript
const { dismissNotification, clearAllDismissals } = useAuth();

const handleDismissNotification = async (notification: WarrantyNotification) => {
  const success = await dismissNotification(notification.id, notification.warrantyExpiry);
  if (success) {
    showToast('Notification dismissed', 'success');
  } else {
    showToast('Failed to dismiss notification', 'error');
  }
};

const handleClearAll = async () => {
  if (warrantyNotifications.length === 0) return;
  
  // Confirm with user
  if (!window.confirm(`Clear all ${warrantyNotifications.length} notifications?`)) {
    return;
  }
  
  const success = await clearAllDismissals();
  if (success) {
    showToast('All notifications cleared', 'success');
  } else {
    showToast('Failed to clear notifications', 'error');
  }
};
```

**Add "Clear All" button** (after line 256, in header section):

```tsx
<Header
  title="Notifications"
  subtitle="Warranty alerts and reminders"
  icon="fas fa-bell"
/>

<div className="content">
  {/* Add header actions bar */}
  <div style={{
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '1.5rem',
  }}>
    <div style={{ color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
      {warrantyNotifications.length} active alert{warrantyNotifications.length !== 1 ? 's' : ''}
    </div>
    {warrantyNotifications.length > 0 && (
      <button 
        className="btn btn-secondary btn-sm"
        onClick={handleClearAll}
      >
        <i className="fas fa-check-double"></i>
        Clear All
      </button>
    )}
  </div>
  
  {/* Summary bar */}
  {/* ... existing summary cards ... */}
</div>
```

**Add dismiss "X" button to each notification card** (update `renderNotificationCard`, line 58-155):

```tsx
const renderNotificationCard = (notification: WarrantyNotification) => {
  const statusColor = getStatusColor(notification.status);

  return (
    <div
      key={notification.id}
      style={{
        padding: '1rem 1.25rem',
        background: 'var(--bg-primary)',
        borderRadius: 'var(--radius-md)',
        marginBottom: '0.5rem',
        cursor: 'pointer',
        border: `1px solid ${statusColor}`,
        borderLeft: `4px solid ${statusColor}`,
        transition: 'all 0.2s ease',
        display: 'flex',
        alignItems: 'center',
        gap: '1rem',
        position: 'relative',
      }}
    >
      {/* Dismiss button (absolute positioned) */}
      <button
        onClick={(e) => {
          e.stopPropagation(); // Prevent card click
          void handleDismissNotification(notification);
        }}
        style={{
          position: 'absolute',
          top: '0.5rem',
          right: '0.5rem',
          background: 'none',
          border: 'none',
          color: 'var(--text-secondary)',
          cursor: 'pointer',
          padding: '0.25rem',
          fontSize: '0.875rem',
          opacity: 0.6,
          transition: 'all 0.2s ease',
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.opacity = '1';
          e.currentTarget.style.color = statusColor;
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.opacity = '0.6';
          e.currentTarget.style.color = 'var(--text-secondary)';
        }}
        title="Dismiss notification"
      >
        <i className="fas fa-times"></i>
      </button>

      {/* Rest of existing card content */}
      <div
        onClick={() => handleNotificationClick(notification)}
        style={{ flex: 1, display: 'flex', alignItems: 'center', gap: '1rem' }}
      >
        {/* ... existing icon, text, etc. ... */}
      </div>
    </div>
  );
};
```

**Alternative**: Use `ConfirmModal` component instead of `window.confirm()` for better UX.

#### Step 5: Handle Edge Cases

1. **Warranty date updated** — Notification reappears (dismissed state cleared)
2. **Item deleted** — Dismissed state remains but harmless (no notification generated)
3. **Multiple devices** — Dismissal syncs via UserSettings API
4. **Notifications disabled** — Dismissal state persists but hidden
5. **Logout/login** — Dismissal state persists (tied to user account)

---

## Implementation Order

### Phase 1: Enhancement 1 (Navigate to Item Details)
**Estimated effort**: 30 minutes

1. Update `NotificationsPage.tsx` click handler (2 minutes)
2. Update `WarrantyNotificationBanner.tsx` click handler (2 minutes)
3. Add imports to `InventoryDetailPage.tsx` (1 minute)
4. Add location state handling and useEffect (15 minutes)
5. Test navigation flow (10 minutes)

### Phase 2: Enhancement 2 (Badge on Item Cards)
**Estimated effort**: 45 minutes

1. Add CSS for notification badge (10 minutes)
2. Update `InventoryDetailPage.tsx` imports (2 minutes)
3. Add `getItemNotification` helper (3 minutes)
4. Add `NotificationBadge` component (15 minutes)
5. Render badge on item cards (5 minutes)
6. Test badge appearance and styling (10 minutes)

### Phase 3: Enhancement 3 (Clear Notifications)
**Estimated effort**: 2 hours

1. Add `DismissedWarranties` type to `types/index.ts` (5 minutes)
2. Add dismissal functions to `AuthContext.tsx` (30 minutes)
3. Update `checkWarrantyNotifications` in `notifications.ts` (20 minutes)
4. Update `AppContext.tsx` to pass dismissed state (15 minutes)
5. Add dismiss handlers to `NotificationsPage.tsx` (20 minutes)
6. Add "Clear All" button UI (10 minutes)
7. Add "X" dismiss button to notification cards (15 minutes)
8. Test dismissal flow and persistence (20 minutes)

**Total estimated time**: ~3.25 hours

---

## UI/UX Considerations

### Enhancement 1: Auto-Open Modal

**Pros**:
- Direct access to item details (1-click vs 3-click)
- Meets user expectation ("show me this item")
- Smooth transition with React Router state

**Cons**:
- Unexpected for users who prefer to scan inventory first
- Back button closes modal then returns to previous page (2 clicks to go back)

**Mitigation**:
- Clear visual transition (modal animation)
- "Close" button prominent in modal footer
- Breadcrumb or back button in modal title

### Enhancement 2: Badge Design

**Pros**:
- Immediate visual feedback on item cards
- Color-coded severity (red = urgent)
- Compact (doesn't clutter card)

**Cons**:
- May overlap with existing content (category badge)
- Too many badges = visual noise

**Design Decision**:
- Position badge at **top-right** corner (absolute positioning)
- Category badge stays inline in header (no conflict)
- Only show badge for items with active notifications (filtered list)

**Accessibility**:
- Add `title` attribute for tooltip
- Add `aria-label` for screen readers
- Sufficient color contrast (WCAG AA)

### Enhancement 3: Dismissal Behavior

**Pros**:
- Users control their notification list
- Reduces notification fatigue
- Syncs across devices (via UserSettings)

**Cons**:
- Users might accidentally dismiss important notifications
- Re-triggering logic (warranty date change) may be non-obvious

**Mitigation**:
- "Clear All" requires confirmation (prevent accidents)
- Dismissed notifications reappear if warranty date changes (user updated item)
- Clear indicator that dismissal is permanent (until warranty changes)

**Alternative**: "Snooze" instead of "Dismiss" (reappear after X days) — **out of scope for this phase**.

---

## Potential Risks & Mitigations

### Risk 1: Circular Dependency (AppContext ↔ AuthContext)

**Problem**: AppContext needs AuthContext for dismissal state, but AuthContext might need AppContext for other features.

**Mitigation**:
- Keep dismissal logic in **AuthContext only**
- Pass `dismissedWarranties` as parameter to `checkNotifications()`
- AppContext imports `useAuth()` hook (one-way dependency)
- **Alternative**: Create separate `NotificationContext` for dismissal logic

### Risk 2: localStorage vs UserSettings Race Condition

**Problem**: User dismisses notification → update localStorage immediately → API call in progress → user refreshes page → dismissal lost.

**Mitigation**:
- **Optimistic UI update**: Update local state immediately, API call in background
- **Rollback on failure**: If API call fails, restore dismissed notification
- **Debounce**: Batch dismissal updates (if user dismisses multiple at once)

### Risk 3: Notification ID Collision

**Problem**: Notifications use `item.id` as `notification.id`. If item is deleted and recreated with same ID, dismissal state persists.

**Mitigation**:
- Include `warrantyExpiry` date in dismissal key
- If warranty date changes, dismissal is cleared (notification reappears)
- **Edge case**: Item deleted, new item with same ID created → old dismissal applies. **Acceptable risk** (low probability).

### Risk 4: Performance with Large Dismissed List

**Problem**: `settings_json` grows unbounded if users dismiss hundreds of notifications.

**Mitigation**:
- **Cleanup**: Remove dismissed entries for items that no longer exist (after 90 days)
- **Pagination**: Limit dismissed list to 1000 most recent entries
- **Backend**: Consider moving to dedicated table if `settings_json` exceeds 10KB

### Risk 5: Badge Visual Clutter

**Problem**: If many items have notifications, badges clutter item cards.

**Mitigation**:
- Only show badge for items displayed in current view (filtered by organizer, search, etc.)
- **Option**: Add toggle to hide/show badges (user preference)
- Keep badge design minimal (icon + short text)

---

## Testing Strategy

### Manual Testing

#### Enhancement 1: Navigate to Item Details
1. **Test 1**: Click notification on NotificationsPage → navigates to inventory → modal opens
2. **Test 2**: Click notification on WarrantyNotificationBanner → same behavior
3. **Test 3**: Modal shows correct item details (name, warranty, etc.)
4. **Test 4**: Close modal → modal closes, stays on inventory page
5. **Test 5**: Navigate away and back → modal doesn't auto-open again
6. **Test 6**: Click notification for deleted item → navigates to inventory, modal doesn't open, no error

#### Enhancement 2: Badge on Item Cards
1. **Test 1**: Item with expired warranty → red badge with "Expired"
2. **Test 2**: Item with expiring-soon warranty (≤7d) → orange badge with "Xd"
3. **Test 3**: Item with expiring-this-month warranty → blue badge with "Xd"
4. **Test 4**: Item with no notification → no badge
5. **Test 5**: Badge tooltip shows full message on hover
6. **Test 6**: Badge doesn't overlap category badge or other content
7. **Test 7**: Badge responsive on mobile/tablet (doesn't break layout)

#### Enhancement 3: Clear Notifications
1. **Test 1**: Click "X" on notification card → notification disappears immediately
2. **Test 2**: Refresh page → dismissed notification stays dismissed
3. **Test 3**: Click "Clear All" → confirmation modal → all notifications cleared
4. **Test 4**: Update item's warranty date → dismissed notification reappears
5. **Test 5**: Logout and login → dismissed state persists
6. **Test 6**: Different browser/device → dismissed state syncs
7. **Test 7**: API call fails → dismissal rolls back, error message shown

### Automated Testing (Out of Scope)

- Unit tests for `checkWarrantyNotifications()` with dismissed list
- Integration tests for navigation state handling
- E2E tests for dismissal flow with Playwright/Cypress

---

## Success Metrics

### Enhancement 1: Navigate to Item Details
- **Metric**: Time from notification click to viewing item details
- **Target**: < 2 seconds (vs ~10 seconds with manual navigation)
- **Measurement**: User session analytics (if implemented)

### Enhancement 2: Badge on Item Cards
- **Metric**: User awareness of item notifications
- **Target**: 90% of users notice badge within 5 seconds of viewing inventory
- **Measurement**: User feedback survey, heatmap tracking

### Enhancement 3: Clear Notifications
- **Metric**: Notification dismissal rate
- **Target**: 50% of users dismiss at least one notification per session
- **Measurement**: Track dismissal API calls, localStorage updates

---

## Future Enhancements (Out of Scope)

1. **Snooze Notifications** — Temporarily hide for X days instead of permanent dismissal
2. **Notification Preferences** — Per-item notification settings (disable for specific items)
3. **Email/Push Notifications** — Send alerts to user's email or phone
4. **Batch Actions** — Select multiple notifications and dismiss all at once
5. **Notification History** — View dismissed notifications (with restore option)
6. **Custom Notification Rules** — User-defined thresholds (e.g., alert 60 days before expiry)
7. **Notification Grouping** — Group by status, inventory, or category
8. **Notification Sorting** — Sort by date, severity, item name

---

## Files to Modify

### Enhancement 1: Navigate to Item Details
- `frontend/src/pages/NotificationsPage.tsx` (update click handler)
- `frontend/src/components/WarrantyNotificationBanner.tsx` (update click handler)
- `frontend/src/pages/InventoryDetailPage.tsx` (add location state handling, useEffect)

### Enhancement 2: Badge on Item Cards
- `frontend/src/styles/cards.css` (add badge CSS)
- `frontend/src/pages/InventoryDetailPage.tsx` (add badge component and render)

### Enhancement 3: Clear Notifications
- `frontend/src/types/index.ts` (add `DismissedWarranties` type)
- `frontend/src/context/AuthContext.tsx` (add dismissal functions)
- `frontend/src/utils/notifications.ts` (filter dismissed notifications)
- `frontend/src/context/AppContext.tsx` (pass dismissed state to checkNotifications)
- `frontend/src/pages/NotificationsPage.tsx` (add dismiss UI and handlers)

**Total files**: 7

---

## Conclusion

This specification provides a comprehensive plan for implementing three notification enhancements in the Home Registry frontend:

1. **Navigate to Item Details Modal** — 1-click access to item details from notifications
2. **Badge on Item Cards** — Visual indicator for items with active notifications
3. **Clear Notifications** — User control over notification list with persistent dismissal

All enhancements are **frontend-only**, leveraging existing backend APIs (UserSettings.settings_json) for persistence. No database migrations or backend changes required.

**Next Steps**:
1. Review specification with stakeholders
2. Implement Phase 1 (Navigate to Item Details)
3. Test and validate Phase 1
4. Implement Phase 2 (Badge on Item Cards)
5. Test and validate Phase 2
6. Implement Phase 3 (Clear Notifications)
7. Test and validate Phase 3
8. Deploy to production

**Estimated total implementation time**: ~3.25 hours

---

**End of Specification**
