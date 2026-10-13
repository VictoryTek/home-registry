# Badge Display Fix Specification

**Date:** February 14, 2026  
**Issue:** Warranty status badges appearing too frequently on item cards  
**Priority:** Medium  
**Estimated Effort:** 1-2 hours

---

## Executive Summary

Warranty status badges are currently showing on item cards for items with warranties that expired months or even years ago. The badges should only appear when a warranty is actively expiring or recently expired (within a reasonable timeframe), not for items with long-expired warranties.

---

## Current State Analysis

### File Locations
- **Badge Rendering:** `frontend/src/pages/InventoryDetailPage.tsx` (lines 438-461)
- **Notification Logic:** `frontend/src/utils/notifications.ts` (lines 23-87)
- **Context Management:** `frontend/src/context/AppContext.tsx` (lines 75-85)

### Current Badge Display Logic

#### 1. Badge Rendering (InventoryDetailPage.tsx)
```tsx
{items.map((item) => {
  const notification = getItemNotification(item.id);
  return (
    <div key={item.id} className="item-card">
      {/* Badge shown if notification exists */}
      {notification && (() => {
        const { status, daysUntilExpiry } = notification;
        // Badge rendering logic...
      })()}
      {/* Rest of item card */}
    </div>
  );
})}
```

**Analysis:** Badge is rendered conditionally based on whether `getItemNotification(item.id)` returns a truthy value. This part is working correctly.

#### 2. Get Item Notification Helper (InventoryDetailPage.tsx, line 71)
```tsx
const getItemNotification = (itemId: number | undefined) => {
  if (!itemId) return null;
  return warrantyNotifications.find((n) => n.id === itemId);
};
```

**Analysis:** Simply finds a notification in the global `warrantyNotifications` array. This part is working correctly.

#### 3. Notification Creation Logic (notifications.ts, lines 23-87)
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
    // ✅ CORRECT: Skip items without warranty dates
    if (!item.warranty_expiry || !item.id) {
      return;
    }

    // ✅ CORRECT: Skip dismissed notifications
    const dismissed = dismissedWarranties[String(item.id)];
    if (dismissed && dismissed.warrantyExpiry === item.warranty_expiry) {
      return;
    }

    // Calculate days until expiry
    const expiryDate = new Date(item.warranty_expiry);
    expiryDate.setHours(0, 0, 0, 0);
    const diffTime = expiryDate.getTime() - now.getTime();
    const diffDays = Math.ceil(diffTime / (1000 * 60 * 60 * 24));

    // 🐛 BUG: Expired items are ALWAYS shown, regardless of how long ago
    let status: WarrantyNotification['status'];
    if (diffDays < 0) {
      status = 'expired';  // ⚠️ No time limit check!
    } else if (diffDays <= 7) {
      status = 'expiring-soon';
    } else if (diffDays <= daysThreshold) {
      status = 'expiring-this-month';
    } else {
      return; // ✅ CORRECT: Skip warranties far from expiring
    }

    // Creates notification for expired items without time limit
    notifications.push({
      id: item.id,
      itemName: item.name,
      inventoryId: item.inventory_id,
      warrantyExpiry: item.warranty_expiry,
      daysUntilExpiry: diffDays,
      status,
    });
  });

  return notifications;
}
```

---

## Root Cause

**The bug is on lines 57-58 of `frontend/src/utils/notifications.ts`:**

```typescript
if (diffDays < 0) {
  status = 'expired';
}
```

When `diffDays < 0`, the warranty is expired. However, there is **no check for how long ago it expired**. This means:

- ✅ An item with a warranty that expired 5 days ago → Shows badge (CORRECT)
- ✅ An item with a warranty that expired 20 days ago → Shows badge (CORRECT)
- ❌ An item with a warranty that expired 100 days ago → Shows badge (INCORRECT)
- ❌ An item with a warranty that expired 1 year ago → Shows badge (INCORRECT)
- ❌ An item with a warranty that expired 5 years ago → Shows badge (INCORRECT)

### Sample Data Evidence

Looking at `migrations/019_add_sample_inventory_data.sql`, the sample data includes 9 items with expired warranties (as of Feb 14, 2026):

1. **Webcam HD 1080p:** 2025-01-10 (expired 400+ days ago) ❌
2. **Instant Pot Duo:** 2024-10-30 (expired 471+ days ago) ❌
3. **Cuisinart Coffee Maker:** 2024-07-15 (expired 578+ days ago) ❌
4. **Ryobi Miter Saw:** 2025-07-08 (expired 220+ days ago) ❌
5. **Shop Vacuum:** 2025-01-25 (expired 385+ days ago) ❌
6. **Air Compressor:** 2024-09-10 (expired 492+ days ago) ❌
7. **Air Purifier:** 2026-01-05 (expired 40 days ago) ✅
8. **Logitech MX Master 3 Mouse:** 2025-03-05 (expired 346+ days ago) ❌
9. **Ninja Air Fryer:** 2025-02-14 (expired exactly 365 days ago) ❌

**Result:** 9 items show warranty badges, but only 1-2 should show badges (recently expired items).

---

## Proposed Solution

### Requirements

1. **Badges MUST show when:**
   - Warranty expires within 30 days (status: 'expiring-this-month')
   - Warranty expires within 7 days (status: 'expiring-soon')
   - Warranty expired within last 30 days (status: 'expired')

2. **Badges MUST NOT show when:**
   - Item has no warranty_expiry field (null/undefined)
   - Warranty expired more than 30 days ago
   - Warranty expires more than 30 days in the future
   - User has dismissed the notification

### Implementation Plan

**File to Modify:** `frontend/src/utils/notifications.ts`

**Change Required:** Add a time limit check for expired warranties in the `checkWarrantyNotifications` function.

#### Code Change

Replace lines 54-65 with:

```typescript
    // Determine status based on days until expiry
    let status: WarrantyNotification['status'];

    if (diffDays < 0) {
      // Warranty expired - only show if expired within the threshold
      const daysExpired = Math.abs(diffDays);
      if (daysExpired > daysThreshold) {
        return; // Expired too long ago - don't show badge
      }
      status = 'expired';
    } else if (diffDays <= 7) {
      status = 'expiring-soon';
    } else if (diffDays <= daysThreshold) {
      status = 'expiring-this-month';
    } else {
      return; // Not expiring soon enough to notify
    }
```

**Explanation:**
- When `diffDays < 0`, the warranty is expired
- Calculate `daysExpired = Math.abs(diffDays)` to get the number of days since expiration
- If `daysExpired > daysThreshold` (default 30 days), skip creating a notification
- Otherwise, create a notification with status 'expired'

This ensures expired warranty badges only show for items that expired within the last 30 days.

---

## Testing Strategy

### Manual Testing

1. **Test expired warranties (old):**
   - Items with warranties expired >30 days ago should NOT show badges
   - Current sample data: Webcam (expired 400+ days ago) should have NO badge

2. **Test expired warranties (recent):**
   - Items with warranties expired ≤30 days ago SHOULD show badges
   - Current sample data: Air Purifier (expired 40 days ago) currently shows badge, but after fix should NOT (exceeds 30 days)
   - To test: Manually update an item's warranty to yesterday's date - should show badge

3. **Test expiring warranties:**
   - Items with warranties expiring in 1-7 days should show "expiring-soon" badge
   - Items with warranties expiring in 8-30 days should show "expiring-this-month" badge
   - Items with warranties expiring >30 days should NOT show badge

4. **Test items without warranties:**
   - Items with `warranty_expiry: null` should NOT show badges
   - Example: Herman Miller Aeron Chair (no warranty) should have NO badge

5. **Test dismissed notifications:**
   - Dismissed notifications should NOT show badges (already working)

### Automated Testing (Optional Future Enhancement)

Create unit tests in `frontend/src/utils/notifications.test.ts`:

```typescript
describe('checkWarrantyNotifications', () => {
  it('should not show badges for warranties expired >30 days ago', () => {
    const items = [{
      id: 1,
      name: 'Test Item',
      warranty_expiry: '2024-01-01', // Expired over a year ago
      // ...other fields
    }];
    const notifications = checkWarrantyNotifications(items, {}, 30);
    expect(notifications).toHaveLength(0);
  });

  it('should show badges for warranties expired ≤30 days ago', () => {
    const yesterday = new Date();
    yesterday.setDate(yesterday.getDate() - 1);
    const items = [{
      id: 1,
      name: 'Test Item',
      warranty_expiry: yesterday.toISOString().split('T')[0],
      // ...other fields
    }];
    const notifications = checkWarrantyNotifications(items, {}, 30);
    expect(notifications).toHaveLength(1);
    expect(notifications[0].status).toBe('expired');
  });

  it('should not show badges for items without warranty dates', () => {
    const items = [{
      id: 1,
      name: 'Test Item',
      warranty_expiry: null,
      // ...other fields
    }];
    const notifications = checkWarrantyNotifications(items, {}, 30);
    expect(notifications).toHaveLength(0);
  });
});
```

---

## Impact Analysis

### User-Facing Changes
- **Fewer badges shown:** Items with old expired warranties will no longer show badges
- **More relevant notifications:** Only recently expired or soon-to-expire warranties will be highlighted
- **Improved UX:** Users won't be overwhelmed by badges on items they likely already know are expired

### Backend/API Changes
- **None:** This is a frontend-only fix

### Performance Impact
- **Negligible:** Adds one additional comparison per expired item (minimal computational cost)
- **Positive:** Fewer notifications in state means slightly less DOM rendering

### Breaking Changes
- **None:** No API changes, no data model changes

---

## Alternative Solutions Considered

### Alternative 1: Only show badges for expiring (not expired)
**Approach:** Don't show badges for expired items at all, only for items expiring soon.

**Pros:**
- Simplest implementation
- Focuses user attention on actionable items

**Cons:**
- User might miss recently expired warranties they should act on
- Less informative for items that expired yesterday or last week

**Decision:** Rejected - showing recently expired items is valuable

### Alternative 2: Different UI for old expired items
**Approach:** Show badges for all expired items, but use different styling (grayed out) for old expired items.

**Pros:**
- Provides complete information
- Visual distinction between recent and old expiry

**Cons:**
- More complex implementation
- Still clutters the UI with less relevant information
- Requires additional CSS and status types

**Decision:** Rejected - simpler to just hide old expired items

### Alternative 3: Make threshold configurable per user
**Approach:** Let users configure how long expired warranties should show badges (e.g., 7, 14, 30, 60 days).

**Pros:**
- Maximum flexibility
- Power users can customize to their preference

**Cons:**
- Adds complexity to settings
- 30 days is a reasonable default for most users
- Can be added later if users request it

**Decision:** Deferred - implement simple fix first, add configurability if needed

---

## Rollout Plan

1. **Implement fix** in `frontend/src/utils/notifications.ts`
2. **Manual testing** using current sample data
3. **Deploy to production** (frontend-only change, no backend coordination needed)
4. **Monitor user feedback** for any issues or adjustment requests
5. **Consider automated tests** if this area sees more changes in the future

---

## Success Criteria

✅ Badges only show for warranties expiring within 30 days  
✅ Badges only show for warranties expired within 30 days  
✅ Badges do NOT show for items without warranty dates  
✅ Badges do NOT show for warranties expired >30 days ago  
✅ Dismissed notifications remain dismissed  
✅ Sample data shows ~0-2 badges instead of 9 badges  

---

## Related Files

- `frontend/src/utils/notifications.ts` - Main fix location
- `frontend/src/pages/InventoryDetailPage.tsx` - Badge rendering
- `frontend/src/context/AppContext.tsx` - Notification state management
- `migrations/019_add_sample_inventory_data.sql` - Sample data for testing

---

## Notes

- The `daysThreshold` parameter is already exposed in the function signature (default 30)
- This same threshold should apply to both future expiry AND past expiry
- The fix maintains backward compatibility with the existing notification system
- No database migration required
- No API changes required

---

**Document Status:** Ready for Implementation  
**Reviewed By:** N/A  
**Approved By:** N/A
