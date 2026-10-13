# Item Card Fixes Specification

**Date**: 2026-02-14  
**Scope**: Four frontend issues with inventory item cards in the InventoryDetailPage  
**Files primarily affected**: `frontend/src/pages/InventoryDetailPage.tsx`, `frontend/src/components/Toast.tsx`, `frontend/src/styles/components.css`

---

## Table of Contents

1. [Issue 1: View Details Button Does Nothing](#issue-1-view-details-button-does-nothing)
2. [Issue 2: Edit Button Does Nothing](#issue-2-edit-button-does-nothing)
3. [Issue 3: Delete Uses Native confirm() Dialog](#issue-3-delete-uses-native-confirm-dialog)
4. [Issue 4: Toast Notifications Full Page Height](#issue-4-toast-notifications-full-page-height)
5. [Existing Patterns & Reusable Components](#existing-patterns--reusable-components)
6. [Implementation Steps](#implementation-steps)
7. [Risks & Mitigations](#risks--mitigations)

---

## Issue 1: View Details Button Does Nothing

### Current Code

**File**: `frontend/src/pages/InventoryDetailPage.tsx`, lines ~296-298

```tsx
<button className="btn btn-sm btn-ghost" title="View Details">
  <i className="fas fa-eye"></i>
</button>
```

The button has **no `onClick` handler**. Clicking it does nothing.

### Available Data

Each item in the `items` array (typed as `Item[]`) has the following fields available on the card:
- `item.id` (number, optional) — unique item identifier
- `item.inventory_id` (number) — parent inventory ID
- `item.name`, `item.description`, `item.category`, `item.location`
- `item.purchase_date`, `item.purchase_price`, `item.warranty_expiry`
- `item.quantity`, `item.notes`
- `item.created_at`, `item.updated_at`

The inventory ID is also available from `useParams` as `id`.

### Existing API Support

The `itemApi` service (`frontend/src/services/api.ts`) already has:
- `itemApi.getById(id: number)` — fetches a single item by ID (GET `/api/items/{id}`)
- `itemApi.getOrganizerValues(itemId: number)` — fetches organizer values for an item

### Existing Routes

No item detail route exists. Current routes in `App.tsx`:
- `/` — InventoriesPage
- `/inventory/:id` — InventoryDetailPage
- `/inventory/:id/report` — InventoryReportPage
- `/inventory/:id/organizers` — OrganizersPage
- `/settings` — SettingsPage
- `/notifications` — NotificationsPage

No `ItemDetailPage` component exists.

### Proposed Solution

**Approach: Modal-based item detail view** (consistent with existing Add Item modal pattern)

Create a "View Details" modal that opens inline on the InventoryDetailPage. This avoids creating a new route/page and keeps the user in context.

**State additions** in `InventoryDetailPage.tsx`:
```tsx
const [viewingItem, setViewingItem] = useState<Item | null>(null);
const [viewingItemOrganizerValues, setViewingItemOrganizerValues] = useState<ItemOrganizerValueWithDetails[]>([]);
```

**Button fix**:
```tsx
<button
  className="btn btn-sm btn-ghost"
  onClick={() => handleViewItem(item)}
  title="View Details"
>
  <i className="fas fa-eye"></i>
</button>
```

**Handler**:
```tsx
const handleViewItem = async (item: Item) => {
  setViewingItem(item);
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

**Modal**: Use the existing `<Modal>` component to display all item fields in a read-only format.

### Data to Display in Detail Modal

- Item name (title)
- Category (badge)
- Description
- Location
- Quantity
- Purchase date (formatted via `formatDate`)
- Purchase price (formatted via `formatCurrency`)
- Total value (price × quantity)
- Warranty expiry (formatted, with expired/active indicator)
- Notes
- Organizer values (fetched via API)
- Created/Updated timestamps

---

## Issue 2: Edit Button Does Nothing

### Current Code

**File**: `frontend/src/pages/InventoryDetailPage.tsx`, lines ~299-301

```tsx
<button className="btn btn-sm btn-ghost" title="Edit Item">
  <i className="fas fa-edit"></i>
</button>
```

The button has **no `onClick` handler**. Clicking it does nothing.

### Existing API Support

The `itemApi` service already has:
- `itemApi.update(id: number, data: UpdateItemRequest)` — updates an item (PUT `/api/items/{id}`)
- `itemApi.getOrganizerValues(itemId: number)` — fetches current organizer values
- `itemApi.setOrganizerValues(itemId: number, data)` — updates organizer values

The `UpdateItemRequest` type (`frontend/src/types/index.ts`, lines ~107-117):
```typescript
export interface UpdateItemRequest {
  name?: string;
  description?: string;
  category?: string;
  location?: string;
  purchase_date?: string;
  purchase_price?: number;
  warranty_expiry?: string;
  notes?: string;
  quantity?: number;
  inventory_id?: number;
}
```

### Existing Patterns

The InventoriesPage (`frontend/src/pages/InventoriesPage.tsx`) shows the established edit pattern:
1. State: `showEditModal`, `editingInventory`, `formData`
2. `openEditModal()` — sets state and pre-fills form
3. `handleEditInventory()` — calls API and reloads data
4. Uses the `<Modal>` component with form inputs and Save/Cancel footer

### Proposed Solution

**Approach: Modal-based edit form** (identical pattern to inventory edit in InventoriesPage)

**State additions** in `InventoryDetailPage.tsx`:
```tsx
const [showEditItemModal, setShowEditItemModal] = useState(false);
const [editingItem, setEditingItem] = useState<Item | null>(null);
const [editItemData, setEditItemData] = useState<UpdateItemRequest>({});
const [editOrganizerValues, setEditOrganizerValues] = useState<Record<string, { optionId?: number; textValue?: string }>>({});
```

**Button fix**:
```tsx
<button
  className="btn btn-sm btn-ghost"
  onClick={() => handleOpenEditItem(item)}
  title="Edit Item"
>
  <i className="fas fa-edit"></i>
</button>
```

**Handlers**:
```tsx
const handleOpenEditItem = async (item: Item) => {
  setEditingItem(item);
  setEditItemData({
    name: item.name,
    description: item.description ?? '',
    category: item.category ?? '',
    location: item.location ?? '',
    purchase_date: item.purchase_date ?? '',
    purchase_price: item.purchase_price,
    warranty_expiry: item.warranty_expiry ?? '',
    notes: item.notes ?? '',
    quantity: item.quantity ?? 1,
  });
  
  // Load current organizer values
  if (item.id) {
    try {
      const result = await itemApi.getOrganizerValues(item.id);
      if (result.success && result.data) {
        const values: Record<string, { optionId?: number; textValue?: string }> = {};
        for (const val of result.data) {
          values[String(val.organizer_type_id)] = {
            optionId: val.organizer_option_id,
            textValue: val.text_value,
          };
        }
        setEditOrganizerValues(values);
      }
    } catch {
      // Continue without pre-filled organizer values
    }
  }
  setShowEditItemModal(true);
};

const handleEditItem = async () => {
  if (!editingItem?.id || !editItemData.name?.trim()) {
    showToast('Please enter an item name', 'error');
    return;
  }

  try {
    const result = await itemApi.update(editingItem.id, editItemData);
    if (result.success) {
      // Save organizer values
      const valuesToSave: SetItemOrganizerValueRequest[] = [];
      for (const [typeIdStr, value] of Object.entries(editOrganizerValues)) {
        const typeId = parseInt(typeIdStr);
        if (value.optionId || value.textValue?.trim()) {
          valuesToSave.push({
            organizer_type_id: typeId,
            organizer_option_id: value.optionId,
            text_value: value.textValue?.trim(),
          });
        }
      }
      if (valuesToSave.length > 0) {
        await itemApi.setOrganizerValues(editingItem.id, { values: valuesToSave });
      }

      showToast('Item updated successfully!', 'success');
      setShowEditItemModal(false);
      setEditingItem(null);
      void loadInventoryDetail(parseInt(id ?? '0', 10));
    } else {
      showToast(result.error ?? 'Failed to update item', 'error');
    }
  } catch {
    showToast('Failed to update item', 'error');
  }
};
```

**Modal**: Reuses the `<Modal>` component with the same form fields as the "Add Item" modal, pre-populated with existing values. The form layout should be identical to the Add Item form for consistency.

---

## Issue 3: Delete Uses Native confirm() Dialog

### Current Code

**File**: `frontend/src/pages/InventoryDetailPage.tsx`, lines ~147-152

```tsx
const handleDeleteItem = async (itemId: number) => {
  // eslint-disable-next-line no-alert
  if (!confirm('Are you sure you want to delete this item?')) {
    return;
  }
  // ... delete logic
};
```

This uses the browser's native `confirm()` dialog, which:
- Looks ugly and inconsistent with the app's design
- Cannot be styled or themed
- Has an ESLint suppression comment (`no-alert`)
- Is jarring compared to the rest of the polished UI

### Existing Solution Already Available

A `ConfirmModal` component **already exists** at `frontend/src/components/ConfirmModal.tsx` and is already exported from `frontend/src/components/index.ts`.

This exact component is already used successfully in `InventoriesPage.tsx` (lines ~617-630) for inventory deletion:

```tsx
<ConfirmModal
  isOpen={showDeleteModal}
  onClose={() => {
    setShowDeleteModal(false);
    setDeletingInventory(null);
  }}
  onConfirm={handleDeleteInventory}
  title="Delete Inventory"
  message={`Are you sure you want to delete "${deletingInventory?.name}"? This action cannot be undone.`}
  confirmText="Delete"
  confirmButtonClass="btn-danger"
  icon="fas fa-trash"
/>
```

### Proposed Solution

**Approach: Replace `confirm()` with existing `ConfirmModal`** (follow exact pattern from InventoriesPage)

**Import change** in `InventoryDetailPage.tsx` — add `ConfirmModal` to existing imports:
```tsx
import {
  Header,
  LoadingState,
  EmptyState,
  Modal,
  ConfirmModal,       // ADD THIS
  ShareInventoryModal,
} from '@/components';
```

**State additions**:
```tsx
const [showDeleteItemModal, setShowDeleteItemModal] = useState(false);
const [deletingItem, setDeletingItem] = useState<Item | null>(null);
```

**Refactored handler** (remove `confirm()`, split into trigger + action):
```tsx
const openDeleteItemModal = (item: Item) => {
  setDeletingItem(item);
  setShowDeleteItemModal(true);
};

const handleDeleteItem = async () => {
  if (!deletingItem?.id) return;

  try {
    const result = await itemApi.delete(deletingItem.id);
    if (result.success) {
      showToast('Item deleted successfully!', 'success');
      void loadInventoryDetail(parseInt(id ?? '0', 10));
    } else {
      showToast(result.error ?? 'Failed to delete item', 'error');
    }
  } catch {
    showToast('Failed to delete item', 'error');
  }
};
```

**Button change**:
```tsx
<button
  className="btn btn-sm btn-ghost text-danger"
  onClick={() => openDeleteItemModal(item)}
  title="Delete Item"
>
  <i className="fas fa-trash"></i>
</button>
```

**Modal (at end of component JSX)**:
```tsx
<ConfirmModal
  isOpen={showDeleteItemModal}
  onClose={() => {
    setShowDeleteItemModal(false);
    setDeletingItem(null);
  }}
  onConfirm={handleDeleteItem}
  title="Delete Item"
  message={`Are you sure you want to delete "${deletingItem?.name}"? This action cannot be undone.`}
  confirmText="Delete"
  confirmButtonClass="btn-danger"
  icon="fas fa-trash"
/>
```

### ConfirmModal Component Analysis

The existing `ConfirmModal` component (`frontend/src/components/ConfirmModal.tsx`):
- Renders a styled modal overlay with backdrop blur
- Has a max-width of 400px (compact, centered dialog)
- Includes Cancel and Confirm buttons
- Calls `onConfirm()` then `onClose()` on confirm
- Supports customizable title, message, confirm text, button class, and icon
- Stops event propagation correctly
- **Note**: Does NOT use `createPortal` like the `Modal` component does — it renders in place. This could potentially cause stacking context issues if a parent has `overflow: hidden` or a lower z-index. However, since the same component works in InventoriesPage without issues, it should be fine here too.

---

## Issue 4: Toast Notifications Full Page Height

### Root Cause

**Files**: 
- `frontend/src/styles/components.css`, lines ~113-126 (CSS)
- `frontend/src/components/Toast.tsx`, line ~52 (inline style)

The CSS defines:
```css
.toast {
  position: fixed;
  bottom: 2rem;     /* ← sets bottom edge */
  right: 2rem;
  /* ... */
  max-width: 400px;
  /* NOTE: no explicit height set */
}
```

The React component adds an inline style:
```tsx
style={{ top: `${2 + index * 4.5}rem` }}   /* ← sets top edge */
```

**The bug**: When a `position: fixed` element has BOTH `top` and `bottom` set but NO explicit `height`, the browser calculates the element's height as:
```
height = viewport_height - top - bottom
```

For the first toast (index 0), this becomes:
```
height = 100vh - 2rem - 2rem = nearly full page height
```

This causes each toast to stretch from near the top to near the bottom of the viewport.

### CSS Analysis

The toast CSS in `frontend/src/styles/components.css`:
```css
.toast {
  position: fixed;
  bottom: 2rem;          /* PROBLEM: combines with inline `top` */
  right: 2rem;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  padding: 1rem 1.5rem;
  box-shadow: var(--shadow-xl);
  z-index: 3000;
  display: flex;
  align-items: center;
  gap: 0.75rem;
  transform: translateX(100%);
  transition: transform 0.3s ease;
  max-width: 400px;
}
```

### Proposed Solution

**Option A (Recommended): Remove `bottom` from CSS, keep `top` positioning from inline style**

This preserves the current stacking behavior (toasts appear from the top, each subsequent toast offset by 4.5rem):

```css
.toast {
  position: fixed;
  /* bottom: 2rem;  ← REMOVE THIS LINE */
  right: 2rem;
  /* ... rest stays the same */
}
```

**Option B: Remove inline `top`, use only `bottom` positioning**

This would position all toasts at the bottom-right corner. However, stacking multiple toasts from the bottom is more complex and would require changing the inline style calculation.

**Option C: Add explicit height constraint**

Add `height: auto` to override the stretched height:
```css
.toast {
  /* ... existing styles ... */
  height: auto; /* Prevent stretching when top is set inline */
}
```

**Recommendation**: Option A is cleanest and most intentional — the inline `top` style already handles positioning per-toast, so the CSS `bottom` rule is simply incorrect/leftover.

---

## Existing Patterns & Reusable Components

### Components Already Available

| Component | Location | Used For |
|-----------|----------|----------|
| `Modal` | `frontend/src/components/Modal.tsx` | General-purpose modal with portal (used for Add Item, Edit Inventory, etc.) |
| `ConfirmModal` | `frontend/src/components/ConfirmModal.tsx` | Delete confirmations (used for inventory deletion in InventoriesPage) |
| `Toast` | `frontend/src/components/Toast.tsx` | Notification toasts (used globally via AppContext) |

### API Services Already Available

| Method | Description |
|--------|-------------|
| `itemApi.getById(id)` | GET single item by ID |
| `itemApi.update(id, data)` | PUT update item |
| `itemApi.delete(id)` | DELETE item |
| `itemApi.getOrganizerValues(itemId)` | GET organizer values for item |
| `itemApi.setOrganizerValues(itemId, data)` | PUT organizer values for item |

### Type Definitions Already Available

| Type | Description |
|------|-------------|
| `Item` | Full item interface with all fields |
| `UpdateItemRequest` | Update request (all fields optional) |
| `ItemOrganizerValueWithDetails` | Organizer value with type name |
| `SetItemOrganizerValueRequest` | Set organizer value request |
| `SetItemOrganizerValuesRequest` | Batch set organizer values |

### Formatting Utilities Already Available

| Utility | Import Path |
|---------|-------------|
| `formatDate(date, format)` | `@/utils/dateFormat` |
| `formatCurrency(amount, currency)` | `@/utils/currencyFormat` |

### Routing

React Router is configured via `BrowserRouter` in `App.tsx`. No new routes are needed — all fixes are modal-based within the existing `InventoryDetailPage`.

---

## Implementation Steps

### Recommended Order

1. **Fix Toast CSS** (Issue 4) — Smallest change, independent of others, immediately visible improvement
   - Edit `frontend/src/styles/components.css`: remove `bottom: 2rem` from `.toast`

2. **Fix Delete Confirmation** (Issue 3) — Uses existing `ConfirmModal`, no new components needed
   - Import `ConfirmModal` in `InventoryDetailPage.tsx`
   - Add state for `showDeleteItemModal` and `deletingItem`
   - Refactor `handleDeleteItem` to remove `confirm()`, add `openDeleteItemModal`
   - Update delete button `onClick` to call `openDeleteItemModal`
   - Add `<ConfirmModal>` to JSX

3. **Implement View Details Modal** (Issue 1) — Read-only display, moderate complexity
   - Add state for `viewingItem` and `viewingItemOrganizerValues`
   - Add `handleViewItem` handler (fetches organizer values)
   - Update View Details button `onClick`
   - Add `<Modal>` for item detail display with all fields

4. **Implement Edit Item Modal** (Issue 2) — Most complex, reuses Add Item form pattern
   - Add state for `showEditItemModal`, `editingItem`, `editItemData`, `editOrganizerValues`
   - Add `handleOpenEditItem` handler (pre-fills form, loads organizer values)
   - Add `handleEditItem` handler (calls API, saves organizer values)
   - Update Edit button `onClick`
   - Add `<Modal>` for edit item form (mirrors Add Item modal)

### New Imports Needed in InventoryDetailPage.tsx

```tsx
import {
  Header,
  LoadingState,
  EmptyState,
  Modal,
  ConfirmModal,           // ADD for Issue 3
  ShareInventoryModal,
} from '@/components';
```

```tsx
import type {
  Inventory,
  Item,
  CreateItemRequest,
  UpdateItemRequest,                    // ADD for Issue 2
  OrganizerTypeWithOptions,
  SetItemOrganizerValueRequest,
  ItemOrganizerValueWithDetails,        // ADD for Issue 1
} from '@/types';
```

### Summary of New State Variables

```tsx
// Issue 1: View Details
const [viewingItem, setViewingItem] = useState<Item | null>(null);
const [viewingItemOrganizerValues, setViewingItemOrganizerValues] = useState<ItemOrganizerValueWithDetails[]>([]);

// Issue 2: Edit Item
const [showEditItemModal, setShowEditItemModal] = useState(false);
const [editingItem, setEditingItem] = useState<Item | null>(null);
const [editItemData, setEditItemData] = useState<UpdateItemRequest>({});
const [editOrganizerValues, setEditOrganizerValues] = useState<Record<string, { optionId?: number; textValue?: string }>>({});

// Issue 3: Delete Confirmation
const [showDeleteItemModal, setShowDeleteItemModal] = useState(false);
const [deletingItem, setDeletingItem] = useState<Item | null>(null);
```

---

## Risks & Mitigations

### Risk 1: Multiple Modals Open Simultaneously
**Risk**: User could potentially trigger View + Edit + Delete modals at the same time  
**Mitigation**: Each modal has independent `isOpen` state. The Modal component uses `createPortal` to render at document root, and z-index 2000 ensures proper layering. The ConfirmModal uses z-index from `.modal-overlay` (2000). Only one action at a time is natural UX.

### Risk 2: ConfirmModal Not Using Portal
**Risk**: The `ConfirmModal` component does NOT use `createPortal` (unlike `Modal` which does). If rendered inside a parent with `overflow: hidden` or constrained z-index, it might clip.  
**Mitigation**: The `ConfirmModal` is already used successfully in `InventoriesPage` with the same DOM structure. The `.modal-overlay` has `position: fixed; z-index: 2000` which should work correctly. If issues arise, refactoring `ConfirmModal` to use `createPortal` (like `Modal`) would fix it.

### Risk 3: Organizer Values API Call Failure
**Risk**: `itemApi.getOrganizerValues()` could fail (network error, 404 for items without organizer values)  
**Mitigation**: Wrap API call in try/catch, continue without organizer values if it fails. The detail/edit modals should gracefully handle empty organizer data.

### Risk 4: Toast Positioning Break on Explicit `bottom` Override
**Risk**: Other code or CSS might depend on the `bottom: 2rem` rule for toast positioning  
**Mitigation**: The Toast component's inline `top` style was always intended to be the positioning mechanism (it handles per-toast stacking). The `bottom` rule was conflicting. Removing it only restores intended behavior.

### Risk 5: Form State Persisting Between Edits
**Risk**: If user opens Edit for Item A, closes without saving, then opens Edit for Item B, stale data from Item A might appear  
**Mitigation**: The `handleOpenEditItem` handler fully resets `editItemData` and `editOrganizerValues` from the selected item's data before opening the modal. Additionally, the modal close handler should reset the editing state.

---

## Files to Modify

| File | Changes |
|------|---------|
| `frontend/src/styles/components.css` | Remove `bottom: 2rem` from `.toast` (Issue 4) |
| `frontend/src/pages/InventoryDetailPage.tsx` | All logic changes: imports, state, handlers, button onClick, modals (Issues 1-3) |

## No New Files Required

All fixes use existing components (`Modal`, `ConfirmModal`), existing API methods (`itemApi`), and existing types. No new pages, routes, components, or CSS files are needed.
