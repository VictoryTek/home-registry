# Specification: Move Warranty Notifications to Dedicated Notifications Page

**Date:** 2026-02-14  
**Status:** Draft  
**Author:** Auto-generated research  

---

## 1. Current State Analysis

### 1.1 Notification Data Flow

```
Items loaded in InventoriesPage
       │
       ▼
setItems() → AppContext global state
       │
       ▼
useEffect → checkNotifications() → checkWarrantyNotifications(items)
       │
       ▼
warrantyNotifications[] stored in AppContext
       │
       ├──▶ Header.tsx reads count → shows bell icon with badge
       └──▶ WarrantyNotificationBanner.tsx reads full list → renders on page
```

### 1.2 Key Files & Their Roles

| File | Role | Lines of Interest |
|------|------|-------------------|
| [frontend/src/utils/notifications.ts](frontend/src/utils/notifications.ts) | Utility functions: `checkWarrantyNotifications()`, `getNotificationMessage()`, `WarrantyNotification` interface | L8-15 (interface), L23-75 (check fn), L80-91 (message fn) |
| [frontend/src/context/AppContext.tsx](frontend/src/context/AppContext.tsx) | Stores `warrantyNotifications` state, provides `checkNotifications()` | L17-18 (type), L33 (state), L64-67 (check fn), L69-71 (auto-check effect) |
| [frontend/src/components/Header.tsx](frontend/src/components/Header.tsx) | Shows bell icon with notification count badge | L12-13 (imports), L15 (count calc), L32-64 (bell button JSX) |
| [frontend/src/components/WarrantyNotificationBanner.tsx](frontend/src/components/WarrantyNotificationBanner.tsx) | Full notification display - grouped by status, clickable cards | Full file (223 lines) |
| [frontend/src/components/index.ts](frontend/src/components/index.ts) | Barrel export for `WarrantyNotificationBanner` | L11 |
| [frontend/src/pages/InventoriesPage.tsx](frontend/src/pages/InventoriesPage.tsx) | Imports & renders `<WarrantyNotificationBanner />` on dashboard | L9 (import), L234 (render) |
| [frontend/src/pages/InventoryDetailPage.tsx](frontend/src/pages/InventoryDetailPage.tsx) | Imports & renders `<WarrantyNotificationBanner />` on detail page | L8 (import), L211 (render) |
| [frontend/src/App.tsx](frontend/src/App.tsx) | React Router configuration - no `/notifications` route exists | L147-209 (protected routes) |
| [frontend/src/pages/index.ts](frontend/src/pages/index.ts) | Barrel exports for all pages | Full file |
| [frontend/src/types/index.ts](frontend/src/types/index.ts) | `Item` type with `warranty_expiry` field, `inventory_id` | L27-41 |

### 1.3 WarrantyNotification Interface

**File:** `frontend/src/utils/notifications.ts` (L8-15)

```typescript
export interface WarrantyNotification {
  id: number;          // item ID
  itemName: string;
  inventoryId: number; // ✅ inventory ID available for navigation
  warrantyExpiry: string;
  daysUntilExpiry: number;
  status: 'expired' | 'expiring-soon' | 'expiring-this-month';
}
```

**Key finding:** Both `id` (item ID) and `inventoryId` are already available in the notification data, enabling direct navigation to `/inventory/{inventoryId}`.

### 1.4 Current Bell Icon Behavior

**File:** `frontend/src/components/Header.tsx` (L32-64)

The bell icon currently:
- Only shows when `notificationCount > 0`
- Displays a count badge (red circle with number)
- Has **NO onClick handler** — it's a static display-only button
- Uses `className="theme-toggle"` for styling (same as the theme button)

### 1.5 Current Banner Rendering

**File:** `frontend/src/components/WarrantyNotificationBanner.tsx`

The banner:
- Groups notifications into 3 sections: Expired, Expiring Soon, Expiring This Month
- Shows max 3 per group with "+N more" overflow text
- Each notification card is clickable → navigates to `/inventory/{notification.inventoryId}`
- Respects `settings.notifications_enabled` toggle
- Uses inline styles with CSS variables

### 1.6 Where Banner Is Currently Rendered

1. **InventoriesPage.tsx** (L234): `<WarrantyNotificationBanner />` inside `.inventories-container`
2. **InventoryDetailPage.tsx** (L211): `<WarrantyNotificationBanner />` inside `.inventory-detail`

### 1.7 How Items Are Loaded for Notifications

**File:** `frontend/src/pages/InventoriesPage.tsx` (L44-85)

- `loadInventories()` fetches all inventories, then fetches items for each inventory in parallel
- All items are pushed to `setItems(allItems)` in AppContext
- AppContext auto-triggers `checkNotifications()` whenever `items` changes (via useEffect)

**Important:** Notification data is computed client-side from all loaded items. There is no dedicated backend notification API endpoint.

---

## 2. Proposed Changes

### 2.1 Overview

| Action | File | Description |
|--------|------|-------------|
| **REMOVE** | `InventoriesPage.tsx` | Remove `WarrantyNotificationBanner` import and render |
| **REMOVE** | `InventoryDetailPage.tsx` | Remove `WarrantyNotificationBanner` import and render |
| **CREATE** | `pages/NotificationsPage.tsx` | New dedicated full-page notifications view |
| **MODIFY** | `pages/index.ts` | Export `NotificationsPage` |
| **MODIFY** | `App.tsx` | Add `/notifications` route, update `getCurrentPage()` and `handleNavigate()` |
| **MODIFY** | `components/Header.tsx` | Add `onClick` to bell icon → navigate to `/notifications` |
| **MODIFY** | `components/Sidebar.tsx` | Add "Notifications" nav item with badge |
| **KEEP** | `utils/notifications.ts` | No changes needed — utility functions reused |
| **KEEP** | `context/AppContext.tsx` | No changes needed — state management stays the same |
| **KEEP** | `components/WarrantyNotificationBanner.tsx` | Keep the file but it will no longer be imported anywhere on inventory pages. Could optionally be deleted or repurposed. |
| **KEEP** | `components/index.ts` | Keep existing export (still referenced by new page or can be removed if new page has its own UI) |

### 2.2 What to REMOVE

#### 2.2.1 InventoriesPage.tsx

**Import removal (L9):**
```tsx
// REMOVE from the import block:
  WarrantyNotificationBanner,
```

**Render removal (L234):**
```tsx
// REMOVE this line:
          <WarrantyNotificationBanner />
```

#### 2.2.2 InventoryDetailPage.tsx

**Import removal (L8):**
```tsx
// REMOVE from the import block:
  WarrantyNotificationBanner,
```

**Render removal (L211):**
```tsx
// REMOVE this line:
          <WarrantyNotificationBanner />
```

---

### 2.3 What to CREATE

#### 2.3.1 `frontend/src/pages/NotificationsPage.tsx`

A new dedicated page component with the following structure:

```
NotificationsPage
├── Header (title="Notifications", icon="fas fa-bell")
├── Content area
│   ├── Empty state (if no notifications or notifications disabled)
│   ├── Summary bar (total count, expired count, expiring-soon count)
│   ├── "Expired Warranties" section
│   │   └── NotificationCard[] (clickable → /inventory/{inventoryId})
│   ├── "Expiring Soon" section (≤7 days)
│   │   └── NotificationCard[]
│   └── "Expiring This Month" section (≤30 days)
│       └── NotificationCard[]
```

**Key requirements:**

1. **Uses existing data:** Read `warrantyNotifications` from `useApp()` context (already computed)
2. **Uses existing utilities:** Import `getNotificationMessage` from `@/utils/notifications`
3. **Respects settings:** Check `settings.notifications_enabled` from `useAuth()`
4. **All notifications shown:** Unlike the banner (limited to 3 per group), the full page shows ALL notifications
5. **Clickable cards:** Each notification navigates to `/inventory/{notification.inventoryId}`  
6. **Grouped display:** Three sections — Expired, Expiring Soon, Expiring This Month
7. **Inventory name enrichment:** Look up inventory name from `inventories` in AppContext to display alongside item name (e.g., "Laptop · Home Office Inventory")
8. **Consistent styling:** Use existing CSS variables and patterns (`.content`, card patterns from inventory pages)

**Component design:**

```tsx
import { useNavigate } from 'react-router-dom';
import { Header } from '@/components';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { getNotificationMessage } from '@/utils/notifications';

export function NotificationsPage() {
  const navigate = useNavigate();
  const { warrantyNotifications, inventories } = useApp();
  const { settings } = useAuth();

  // Get inventory name by ID for display
  const getInventoryName = (inventoryId: number): string => {
    const inv = inventories.find(i => i.id === inventoryId);
    return inv?.name ?? 'Unknown Inventory';
  };

  // Group notifications by status
  const expired = warrantyNotifications.filter(n => n.status === 'expired');
  const expiringSoon = warrantyNotifications.filter(n => n.status === 'expiring-soon');
  const expiringThisMonth = warrantyNotifications.filter(n => n.status === 'expiring-this-month');

  const handleNotificationClick = (notification) => {
    navigate(`/inventory/${notification.inventoryId}`);
  };

  // ... render grouped sections with clickable cards
}
```

**Card design per notification:**

Each notification card should display:
- **Icon:** Status-appropriate icon (exclamation-circle for expired, exclamation-triangle for expiring-soon, info-circle for this-month)
- **Item name** (bold)
- **Inventory name** (secondary text — looked up from inventories context)
- **Message:** Output of `getNotificationMessage(notification)` — e.g., "Laptop warranty expired 5 days ago"
- **Warranty expiry date** (formatted)
- **Visual urgency indicator:** Border/accent color matching status (danger for expired, warning for expiring-soon, info for this-month)
- **Hover effect:** Subtle translateX or background change to indicate clickability
- **Arrow icon** on the right side to indicate navigation

**Empty state:**

If `warrantyNotifications.length === 0` or `!settings?.notifications_enabled`:
- Show an `EmptyState` component (already exists in project) with:
  - Icon: `fas fa-bell-slash` (if disabled) or `fas fa-check-circle` (if no notifications)
  - Title: "Notifications are disabled" or "No warranty alerts"
  - Description: "Enable notifications in Settings" or "All your warranties are up to date!"

---

### 2.4 What to MODIFY

#### 2.4.1 `frontend/src/components/Header.tsx`

**Current bell button (L32-64):** Static display only, no `onClick`.

**Change:** Add `onClick` handler and `useNavigate` to navigate to `/notifications`.

```tsx
// ADD import:
import { useNavigate } from 'react-router-dom';

// ADD inside component:
const navigate = useNavigate();

// MODIFY the bell button: add onClick
<button
  className="theme-toggle"
  onClick={() => navigate('/notifications')}
  title={`${notificationCount} warranty notification${notificationCount !== 1 ? 's' : ''}`}
  style={{ position: 'relative', cursor: 'pointer' }}
>
```

**Also:** Show bell icon even when count is 0 (always visible), just without the badge. This gives users a consistent way to access the notifications page.

The updated logic:
```tsx
// CHANGE: Always show bell (remove the notificationCount > 0 conditional wrapper)
<div style={{ position: 'relative', marginRight: '0.5rem' }}>
  <button
    className="theme-toggle"
    onClick={() => navigate('/notifications')}
    title={notificationCount > 0 
      ? `${notificationCount} warranty notification${notificationCount !== 1 ? 's' : ''}`
      : 'Notifications'}
    style={{ position: 'relative', cursor: 'pointer' }}
  >
    <i className="fas fa-bell"></i>
    {notificationCount > 0 && (
      <span style={{ /* existing badge styles */ }}>
        {notificationCount > 99 ? '99+' : notificationCount}
      </span>
    )}
  </button>
</div>
```

#### 2.4.2 `frontend/src/App.tsx`

**Add `/notifications` route** in the protected routes section (after the `/settings` route, ~L200):

```tsx
<Route
  path="/notifications"
  element={
    <ProtectedRoute>
      <NotificationsPage />
    </ProtectedRoute>
  }
/>
```

**Update imports** (L5-16): Add `NotificationsPage` to the pages import.

**Update `getCurrentPage()` (~L53-65):** Add notifications path detection:
```tsx
if (location.pathname === '/notifications') {
  return 'notifications';
}
```

**Update `handleNavigate()` (~L67-80):** Add notifications case:
```tsx
case 'notifications':
  navigate('/notifications');
  break;
```

#### 2.4.3 `frontend/src/pages/index.ts`

**Add export:**
```tsx
export { NotificationsPage } from './NotificationsPage';
```

#### 2.4.4 `frontend/src/components/Sidebar.tsx` (Optional Enhancement)

Add a "Notifications" nav item in the Overview section with a badge showing count:

```tsx
<button
  className={`nav-item ${currentPage === 'notifications' ? 'active' : ''}`}
  onClick={() => onNavigate('notifications')}
>
  <i className="fas fa-bell"></i>
  <span>Notifications</span>
  {/* Optional: badge with count */}
</button>
```

**Note:** The Sidebar currently doesn't have access to notification count. To show a badge, it would need to import `useApp()` from AppContext. This is optional — the bell icon in the Header is the primary entry point.

---

## 3. Navigation & Linking Logic

### 3.1 Entry Points to Notifications Page

| Entry Point | Current Behavior | New Behavior |
|-------------|-----------------|--------------|
| Bell icon (Header) | Static display, no click handler | `onClick → navigate('/notifications')` |
| Sidebar (new) | N/A | Optional nav item → `/notifications` |
| Direct URL | 404 redirect | `/notifications` route loads `NotificationsPage` |

### 3.2 Navigation FROM Notifications Page

| Click Target | Navigation |
|-------------|------------|
| Notification card | `navigate('/inventory/{notification.inventoryId}')` |
| Back button (optional) | `navigate(-1)` or `navigate('/')` |

### 3.3 Available Navigation Data

From `WarrantyNotification` interface:
- `inventoryId` → navigates to `/inventory/{inventoryId}`
- `id` → this is the item ID, available for future deep-link to specific item

**Current limitation:** The app does not have a direct item detail route (e.g., `/inventory/{id}/item/{itemId}`). Items are displayed inline on the `InventoryDetailPage`. Navigation to `/inventory/{inventoryId}` is the deepest available route. A future enhancement could scroll to or highlight the specific item.

---

## 4. API Data Requirements

### 4.1 No Backend Changes Needed

Notifications are computed **entirely on the client side**:

1. `InventoriesPage.loadInventories()` fetches all inventories (`GET /api/inventories`)
2. For each inventory, items are fetched (`GET /api/items/{inventory_id}`)
3. All items are stored in AppContext via `setItems(allItems)`
4. `checkWarrantyNotifications(items)` computes notifications from items with `warranty_expiry` dates
5. Results are stored in AppContext as `warrantyNotifications`

The new `NotificationsPage` simply reads from this existing AppContext state. **No new API endpoints are required.**

### 4.2 Data Available for Display

From AppContext:
- `warrantyNotifications: WarrantyNotification[]` — grouped/sorted notification data
- `inventories: Inventory[]` — for looking up inventory names by ID

From each `WarrantyNotification`:
- `id` (item ID)
- `itemName` (item name)
- `inventoryId` (parent inventory ID)
- `warrantyExpiry` (date string)
- `daysUntilExpiry` (number, negative if expired)
- `status` ('expired' | 'expiring-soon' | 'expiring-this-month')

### 4.3 Edge Case: Items Not Loaded Yet

If a user navigates directly to `/notifications` (e.g., via bookmark), items may not be loaded yet because `loadInventories()` runs in `InventoriesPage`. 

**Solution options:**
1. **Preferred:** Trigger item loading in `NotificationsPage` if `items` is empty (add a `useEffect` that calls the same API pattern as `InventoriesPage`)
2. **Alternative:** Show a "Load notifications" button or redirect to inventories first
3. **Note:** Since the app redirects unknown routes to `/` which loads `InventoriesPage`, and `AppContext` persists during SPA navigation, items will typically already be loaded

---

## 5. Styling Guidelines

### 5.1 Consistent with Existing App

- Use existing CSS variables: `--bg-primary`, `--bg-secondary`, `--text-primary`, `--text-secondary`, `--border-color`, `--danger-color`, `--warning-color`, `--info-color`, `--radius-md`, `--radius-lg`
- Use existing class names where applicable: `.content`, `.btn`, `.btn-primary`, `.btn-ghost`
- Follow inline style patterns used in `WarrantyNotificationBanner.tsx` (the project uses inline styles extensively)
- Support light/dark themes via CSS variables (automatic with existing setup)

### 5.2 Responsive Design

- Cards should stack vertically on mobile
- Use `max-width` on the content area consistent with other pages
- Sections should collapse/expand cleanly on smaller screens

---

## 6. Component Hierarchy Summary

```
App.tsx
├── Sidebar (add 'notifications' nav item)
├── Header (bell icon → onClick → /notifications)
└── Routes
    ├── / → InventoriesPage (REMOVE WarrantyNotificationBanner)
    ├── /inventory/:id → InventoryDetailPage (REMOVE WarrantyNotificationBanner)
    ├── /notifications → NotificationsPage (NEW)
    │   ├── Header (title="Notifications")
    │   ├── EmptyState (if no notifications)
    │   └── Notification sections
    │       ├── Expired section → NotificationCard[]
    │       ├── Expiring Soon section → NotificationCard[]
    │       └── Expiring This Month section → NotificationCard[]
    ├── /organizers → OrganizersPage
    └── /settings → SettingsPage
```

---

## 7. Implementation Steps (Ordered)

1. **Create `NotificationsPage.tsx`** — new page component with full notification display
2. **Update `pages/index.ts`** — export the new page
3. **Update `App.tsx`** — add route, update `getCurrentPage()`, update `handleNavigate()`
4. **Update `Header.tsx`** — add onClick to bell icon, always show bell icon
5. **Update `Sidebar.tsx`** — add Notifications nav item (optional, but recommended)
6. **Update `InventoriesPage.tsx`** — remove `WarrantyNotificationBanner` import and render
7. **Update `InventoryDetailPage.tsx`** — remove `WarrantyNotificationBanner` import and render
8. **Test** — verify bell icon navigates, notifications display, cards are clickable, empty states work

---

## 8. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Items not loaded on direct `/notifications` access | Empty notifications page | Add data-loading useEffect in NotificationsPage |
| Notification count badge disappears when leaving inventories page | Users miss notifications | Bell icon in Header is always visible across all pages (already the case) |
| Breaking existing banner references | Compile errors | Systematic removal from both pages + verify no other imports |
| AppContext re-renders | Performance | No new state added; existing warrantyNotifications state is reused |

---

## 9. Files Changed Summary

| File | Action | Lines Affected |
|------|--------|---------------|
| `frontend/src/pages/NotificationsPage.tsx` | CREATE | ~150-250 lines (new file) |
| `frontend/src/pages/index.ts` | MODIFY | +1 line (export) |
| `frontend/src/App.tsx` | MODIFY | +12 lines (route, import, getCurrentPage, handleNavigate) |
| `frontend/src/components/Header.tsx` | MODIFY | ~15 lines (add navigate, onClick, always-show bell) |
| `frontend/src/components/Sidebar.tsx` | MODIFY | +8 lines (optional nav item) |
| `frontend/src/pages/InventoriesPage.tsx` | MODIFY | -2 lines (remove import + render) |
| `frontend/src/pages/InventoryDetailPage.tsx` | MODIFY | -2 lines (remove import + render) |

**No backend changes required.**  
**No new dependencies required.**  
**No database changes required.**
