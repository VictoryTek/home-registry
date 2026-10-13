# Notification UI Improvements Specification

**Date**: February 14, 2026  
**Project**: Home Registry - Frontend TypeScript/React  
**Focus**: Three styling/layout improvements for notification UI

---

## Executive Summary

This specification addresses three specific UI/UX issues in the Home Registry notification system:

1. **Notification badge positioning** - Move badges from top-right to bottom-right of item cards (aligned with action buttons)
2. **"Clear All" button sizing** - Fix full-width button to be appropriately sized and positioned
3. **Notification list styling** - Modernize notification cards to match project design language

---

## Current State Analysis

### File Structure

```
frontend/src/
├── pages/
│   ├── InventoryDetailPage.tsx    # Item cards with notification badges
│   └── NotificationsPage.tsx      # Notification list and Clear All button
├── styles/
│   ├── cards.css                  # Current badge and card styling
│   ├── organizers.css             # Reference design patterns
│   ├── variables.css              # Design tokens
│   └── buttons.css                # Button styling
```

### Issue #1: Notification Badge Positioning

**Current Implementation** ([cards.css:281-319](c:\Projects\home-registry\frontend\src\styles\cards.css#L281-L319))

```css
.item-notification-badge {
  position: absolute;
  top: 1rem;           /* ❌ Currently at top */
  right: 1rem;         /* ❌ Currently at right edge */
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
```

**Current Rendering** ([InventoryDetailPage.tsx:438-466](c:\Projects\home-registry\frontend\src\pages\InventoryDetailPage.tsx#L438-L466))

```tsx
<div key={item.id} className="item-card">
  {/* Badge rendered FIRST (before header) */}
  {notification && (
    <span className={`item-notification-badge ${statusClass}`}>
      <i className={`fas ${icon}`}></i>
      {text}
    </span>
  )}
  
  <div className="item-card-header">...</div>
  <div className="item-card-body">...</div>
  
  {/* Footer with action buttons */}
  <div className="item-card-footer">
    <button className="btn btn-sm btn-ghost">...</button>  {/* View */}
    <button className="btn btn-sm btn-ghost">...</button>  {/* Edit */}
    <button className="btn btn-sm btn-ghost">...</button>  {/* Delete */}
  </div>
</div>
```

**Problem**: Badge overlaps with card title in top-right corner, making it visually disconnected from actionable elements.

**Desired State**: Badge positioned in bottom-right of `.item-card-footer`, next to action buttons (View/Edit/Delete).

---

### Issue #2: "Clear All" Button Size

**Current Implementation** ([NotificationsPage.tsx:337-357](c:\Projects\home-registry\frontend\src\pages\NotificationsPage.tsx#L337-L357))

```tsx
<div style={{
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  marginBottom: '1rem',
}}>
  <div style={{ color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
    {activeNotifications.length} active alert{activeNotifications.length !== 1 ? 's' : ''}
  </div>
  {activeNotifications.length > 0 && (
    <button 
      className="btn btn-secondary btn-sm"
      onClick={handleClearAll}
    >
      <i className="fas fa-check-double"></i>
      Clear All
    </button>
  )}
</div>
```

**Button Classes Applied**:
- `.btn` - Base button styles ([buttons.css:1-15](c:\Projects\home-registry\frontend\src\styles\buttons.css#L1-L15))
- `.btn-secondary` - Secondary variant ([buttons.css:46-51](c:\Projects\home-registry\frontend\src\styles\buttons.css#L46-L51))
- `.btn-sm` - Small size modifier ([buttons.css:68-71](c:\Projects\home-registry\frontend\src\styles\buttons.css#L68-L71))

**Potential Issue**: On mobile devices (< 480px), `.btn` has `width: 100%` rule ([buttons.css:114-117](c:\Projects\home-registry\frontend\src\styles\buttons.css#L114-L117)), causing button to span full width.

**Analysis**:

```css
/* From buttons.css */
@media (max-width: 480px) {
  .btn {
    width: 100%;           /* ❌ Forces all buttons to full width on mobile */
    justify-content: center;
  }
}
```

**Problem**: This media query affects ALL buttons, including the Clear All button which should remain compact even on mobile.

---

### Issue #3: Notification List Styling

**Current Implementation** ([NotificationsPage.tsx:100-227](c:\Projects\home-registry\frontend\src\pages\NotificationsPage.tsx#L100-L227))

The notification cards use **inline styles** throughout:

```tsx
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
  onMouseEnter={...}  // Inline hover handling
  onMouseLeave={...}
>
  {/* Dismiss button with inline styles */}
  {/* Icon section with inline styles */}
  {/* Text content with inline styles */}
  {/* Date/chevron with inline styles */}
</div>
```

**Section Headers** ([NotificationsPage.tsx:240-268](c:\Projects\home-registry\frontend\src\pages\NotificationsPage.tsx#L240-L268))

```tsx
<div style={{ marginBottom: '1.5rem' }}>
  <div style={{
    display: 'flex',
    alignItems: 'center',
    gap: '0.5rem',
    marginBottom: '0.75rem',
  }}>
    <i className={icon} style={{ color, fontSize: '1rem' }}></i>
    <h3 style={{
      margin: 0,
      fontSize: '1rem',
      fontWeight: 600,
      color,
    }}>
      {title} ({notifications.length})
    </h3>
  </div>
  {notifications.map(renderNotificationCard)}
</div>
```

**Problems**:
1. ❌ Extensive inline styles make code verbose and hard to maintain
2. ❌ No CSS classes for consistent reusability
3. ❌ Hover states handled via React events instead of CSS
4. ❌ Visual design feels "afterthought" - lacks polish of organizers page

---

### Reference: Organizers Page Design Pattern

**Organizer Card Styling** ([organizers.css:7-21](c:\Projects\home-registry\frontend\src\styles\organizers.css#L7-L21))

```css
.organizer-card {
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  padding: 1.25rem;
  transition:
    box-shadow 0.2s ease,
    border-color 0.2s ease;
}

.organizer-card:hover {
  border-color: var(--accent-color);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}
```

**Design Elements to Adopt**:
- ✅ Clean card borders with hover effects
- ✅ Consistent spacing using CSS variables
- ✅ Smooth transitions
- ✅ Proper use of design tokens (--border-color, --bg-primary, etc.)
- ✅ CSS :hover states instead of JavaScript handlers
- ✅ Semantic class names

---

## Proposed Solutions

### Solution #1: Badge Repositioning

**Approach**: Move badge from top-right to bottom-right, integrated into `.item-card-footer`.

**CSS Changes** - Update [cards.css:281-319](c:\Projects\home-registry\frontend\src\styles\cards.css#L281-L319)

```css
/* BEFORE */
.item-notification-badge {
  position: absolute;
  top: 1rem;
  right: 1rem;
  /* ... rest of styles */
}

/* AFTER */
.item-notification-badge {
  /* Remove absolute positioning */
  /* Badge will now be placed in footer via DOM structure */
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 0.625rem;
  font-size: 0.75rem;
  font-weight: 600;
  border-radius: 12px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  transition: all 0.2s ease;
  margin-left: auto;  /* Push to far right in flex container */
}
```

**JSX Changes** - Update [InventoryDetailPage.tsx:438-545](c:\Projects\home-registry\frontend\src\pages\InventoryDetailPage.tsx#L438-L545)

```tsx
/* BEFORE */
<div key={item.id} className="item-card">
  {notification && (
    <span className={`item-notification-badge ${statusClass}`}>
      {/* ... */}
    </span>
  )}
  <div className="item-card-header">...</div>
  <div className="item-card-body">...</div>
  <div className="item-card-footer">
    <button>View</button>
    <button>Edit</button>
    <button>Delete</button>
  </div>
</div>

/* AFTER */
<div key={item.id} className="item-card">
  <div className="item-card-header">...</div>
  <div className="item-card-body">...</div>
  <div className="item-card-footer">
    <button>View</button>
    <button>Edit</button>
    <button>Delete</button>
    {notification && (
      <span className={`item-notification-badge ${statusClass}`}>
        {/* ... */}
      </span>
    )}
  </div>
</div>
```

**Visual Result**:
```
┌─────────────────────────────────────┐
│ Item Card                           │
│ [Title]                    [Badge]  │  ← BEFORE (overlapping)
│                                     │
│ Description and details...          │
│                                     │
├─────────────────────────────────────┤
│ [View] [Edit] [Delete]    [Badge]  │  ← AFTER (integrated)
└─────────────────────────────────────┘
```

**Benefits**:
- ✅ Badge no longer overlaps title
- ✅ Grouped with actionable elements (buttons)
- ✅ Maintains visual prominence via color coding
- ✅ No need for absolute positioning z-index complexity

---

### Solution #2: Button Size Fix

**Problem Root Cause**: Mobile-first CSS forcing all `.btn` to `width: 100%`.

**Approach**: Create exception class `.btn-inline` for buttons that should remain compact.

**CSS Changes** - Add to [buttons.css](c:\Projects\home-registry\frontend\src\styles\buttons.css)

```css
/* Add after existing button styles, before @media query */

.btn-inline {
  width: auto !important;  /* Override mobile full-width */
  flex-shrink: 0;          /* Prevent flex compression */
}

/* Existing mobile media query remains unchanged */
@media (max-width: 480px) {
  .btn {
    width: 100%;
    justify-content: center;
  }
  
  /* Exception for inline buttons */
  .btn.btn-inline {
    width: auto !important;
  }
}
```

**JSX Changes** - Update [NotificationsPage.tsx:346-352](c:\Projects\home-registry\frontend\src\pages\NotificationsPage.tsx#L346-L352)

```tsx
/* BEFORE */
<button 
  className="btn btn-secondary btn-sm"
  onClick={handleClearAll}
>
  <i className="fas fa-check-double"></i>
  Clear All
</button>

/* AFTER */
<button 
  className="btn btn-secondary btn-sm btn-inline"
  onClick={handleClearAll}
>
  <i className="fas fa-check-double"></i>
  Clear All
</button>
```

**Alternative Approach** (if broader change needed):

Refactor mobile button behavior to only apply to primary action buttons:

```css
@media (max-width: 480px) {
  /* Only make form submit buttons full-width */
  .btn-primary:not(.btn-inline),
  .modal-footer .btn {
    width: 100%;
    justify-content: center;
  }
}
```

**Benefits**:
- ✅ Fixes Clear All button spanning full width
- ✅ Maintains responsive design for appropriate buttons
- ✅ Reusable pattern for other compact buttons (filters, toggles)

---

### Solution #3: Notification List Redesign

**Approach**: Convert inline styles to CSS classes matching organizers page pattern.

**Step 1: Create Notification CSS Module**

Create new file: `frontend/src/styles/notifications.css`

```css
/* Notification Page Styles */

/* Header section with count and actions */
.notifications-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
  padding-bottom: 1rem;
  border-bottom: 1px solid var(--border-color);
}

.notifications-count {
  color: var(--text-secondary);
  font-size: 0.9rem;
  font-weight: 500;
}

/* Summary statistics bar */
.notifications-summary {
  display: flex;
  gap: 1rem;
  margin-bottom: 1.5rem;
  flex-wrap: wrap;
}

.notification-stat-card {
  background: var(--bg-secondary);
  border-radius: var(--radius-md);
  padding: 0.75rem 1.25rem;
  border: 1px solid var(--border-color);
  display: flex;
  align-items: center;
  gap: 0.5rem;
  transition: all 0.2s ease;
}

.notification-stat-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-sm);
}

.notification-stat-card i {
  font-size: 1.125rem;
}

.notification-stat-card .stat-value {
  font-weight: 600;
  color: var(--text-primary);
}

.notification-stat-card .stat-label {
  color: var(--text-secondary);
  font-size: 0.875rem;
}

/* Section grouping (expired, expiring soon, etc.) */
.notification-section {
  margin-bottom: 2rem;
}

.notification-section-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 1rem;
  padding-bottom: 0.5rem;
  border-bottom: 2px solid currentColor;
}

.notification-section-header i {
  font-size: 1.125rem;
}

.notification-section-header h3 {
  margin: 0;
  font-size: 1.125rem;
  font-weight: 600;
}

.notification-section-count {
  margin-left: 0.5rem;
  font-weight: 500;
  opacity: 0.8;
}

/* Individual notification cards */
.notification-card {
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  margin-bottom: 0.75rem;
  padding: 1.25rem;
  cursor: pointer;
  transition: all 0.2s ease;
  position: relative;
  display: flex;
  align-items: center;
  gap: 1rem;
}

.notification-card:hover {
  transform: translateX(4px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
  border-color: var(--accent-color);
}

/* Status-specific left border accent */
.notification-card.status-expired {
  border-left: 4px solid var(--danger-color);
}

.notification-card.status-expiring-soon {
  border-left: 4px solid var(--warning-color);
}

.notification-card.status-expiring-this-month {
  border-left: 4px solid var(--info-color);
}

/* Dismiss button */
.notification-dismiss {
  position: absolute;
  top: 0.75rem;
  right: 0.75rem;
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 0.25rem;
  font-size: 0.875rem;
  opacity: 0;
  transition: all 0.2s ease;
  z-index: 1;
}

.notification-card:hover .notification-dismiss {
  opacity: 0.6;
}

.notification-dismiss:hover {
  opacity: 1 !important;
  color: var(--danger-color);
  transform: scale(1.1);
}

/* Icon section */
.notification-icon {
  font-size: 1.5rem;
  flex-shrink: 0;
  width: 2.5rem;
  height: 2.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-md);
  background: var(--bg-secondary);
}

.notification-card.status-expired .notification-icon {
  color: var(--danger-color);
  background: rgba(239, 68, 68, 0.1);
}

.notification-card.status-expiring-soon .notification-icon {
  color: var(--warning-color);
  background: rgba(245, 158, 11, 0.1);
}

.notification-card.status-expiring-this-month .notification-icon {
  color: var(--info-color);
  background: rgba(59, 130, 246, 0.1);
}

/* Content section */
.notification-content {
  flex: 1;
  min-width: 0;
}

.notification-title {
  font-weight: 600;
  font-size: 1rem;
  color: var(--text-primary);
  margin-bottom: 0.25rem;
}

.notification-inventory {
  font-size: 0.875rem;
  color: var(--text-secondary);
  margin-bottom: 0.25rem;
}

.notification-message {
  font-size: 0.875rem;
  font-weight: 500;
}

.notification-card.status-expired .notification-message {
  color: var(--danger-color);
}

.notification-card.status-expiring-soon .notification-message {
  color: var(--warning-color);
}

.notification-card.status-expiring-this-month .notification-message {
  color: var(--info-color);
}

/* Meta section (date + chevron) */
.notification-meta {
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-shrink: 0;
}

.notification-date {
  font-size: 0.8125rem;
  color: var(--text-secondary);
  text-align: right;
}

.notification-chevron {
  color: var(--text-tertiary);
  font-size: 0.875rem;
  transition: transform 0.2s ease;
}

.notification-card:hover .notification-chevron {
  transform: translateX(4px);
  color: var(--accent-color);
}

/* Responsive adjustments */
@media (max-width: 768px) {
  .notification-card {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.75rem;
  }
  
  .notification-meta {
    width: 100%;
    justify-content: space-between;
  }
  
  .notifications-summary {
    flex-direction: column;
  }
  
  .notification-stat-card {
    width: 100%;
  }
}
```

**Step 2: Import CSS in NotificationsPage.tsx**

Add to imports section:

```tsx
import '@/styles/notifications.css';
```

**Step 3: Refactor NotificationsPage.tsx Component Structure**

**Replace** [NotificationsPage.tsx:337-453](c:\Projects\home-registry\frontend\src\pages\NotificationsPage.tsx#L337-L453) **with**:

```tsx
return (
  <>
    <Header
      title="Notifications"
      subtitle="Warranty alerts and reminders"
      icon="fas fa-bell"
    />

    <div className="content">
      {/* Header with count and Clear All button */}
      <div className="notifications-header">
        <div className="notifications-count">
          {activeNotifications.length} active alert{activeNotifications.length !== 1 ? 's' : ''}
        </div>
        {activeNotifications.length > 0 && (
          <button 
            className="btn btn-secondary btn-sm btn-inline"
            onClick={handleClearAll}
          >
            <i className="fas fa-check-double"></i>
            Clear All
          </button>
        )}
      </div>

      {/* Summary statistics */}
      <div className="notifications-summary">
        <div className="notification-stat-card">
          <i className="fas fa-bell" style={{ color: 'var(--warning-color)' }}></i>
          <span className="stat-value">{activeNotifications.length}</span>
          <span className="stat-label">Total Alerts</span>
        </div>

        {expired.length > 0 && (
          <div className="notification-stat-card">
            <i className="fas fa-exclamation-circle" style={{ color: 'var(--danger-color)' }}></i>
            <span className="stat-value">{expired.length}</span>
            <span className="stat-label">Expired</span>
          </div>
        )}

        {expiringSoon.length > 0 && (
          <div className="notification-stat-card">
            <i className="fas fa-exclamation-triangle" style={{ color: 'var(--warning-color)' }}></i>
            <span className="stat-value">{expiringSoon.length}</span>
            <span className="stat-label">Expiring Soon</span>
          </div>
        )}

        {expiringThisMonth.length > 0 && (
          <div className="notification-stat-card">
            <i className="fas fa-info-circle" style={{ color: 'var(--info-color)' }}></i>
            <span className="stat-value">{expiringThisMonth.length}</span>
            <span className="stat-label">This Month</span>
          </div>
        )}
      </div>

      {/* Notification sections */}
      {renderSection('Expired Warranties', 'fas fa-exclamation-circle', 'var(--danger-color)', expired)}
      {renderSection('Expiring Soon', 'fas fa-exclamation-triangle', 'var(--warning-color)', expiringSoon)}
      {renderSection('Expiring This Month', 'fas fa-info-circle', 'var(--info-color)', expiringThisMonth)}
    </div>
  </>
);
```

**Step 4: Refactor Helper Functions**

**Replace** `renderNotificationCard` **function** ([NotificationsPage.tsx:100-227](c:\Projects\home-registry\frontend\src\pages\NotificationsPage.tsx#L100-L227)):

```tsx
const renderNotificationCard = (notification: WarrantyNotification) => {
  const statusClass = `status-${notification.status}`;

  return (
    <div
      key={notification.id}
      className={`notification-card ${statusClass}`}
      onClick={() => handleNotificationClick(notification)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          handleNotificationClick(notification);
        }
      }}
      role="button"
      tabIndex={0}
      aria-label={`${notification.itemName} - ${getNotificationMessage(notification)}`}
    >
      {/* Dismiss button */}
      <button
        className="notification-dismiss"
        onClick={(e) => void handleDismissNotification(notification, e)}
        title="Dismiss notification"
        aria-label="Dismiss notification"
      >
        <i className="fas fa-times"></i>
      </button>

      {/* Icon */}
      <div className="notification-icon">
        <i className={getStatusIcon(notification.status)}></i>
      </div>

      {/* Content */}
      <div className="notification-content">
        <div className="notification-title">{notification.itemName}</div>
        <div className="notification-inventory">{getInventoryName(notification.inventoryId)}</div>
        <div className="notification-message">{getNotificationMessage(notification)}</div>
      </div>

      {/* Meta (date + chevron) */}
      <div className="notification-meta">
        <div className="notification-date">{formatDate(notification.warrantyExpiry)}</div>
        <i className="fas fa-chevron-right notification-chevron"></i>
      </div>
    </div>
  );
};
```

**Replace** `renderSection` **function** ([NotificationsPage.tsx:230-268](c:\Projects\home-registry\frontend\src\pages\NotificationsPage.tsx#L230-L268)):

```tsx
const renderSection = (
  title: string,
  icon: string,
  color: string,
  notifications: WarrantyNotification[]
) => {
  if (notifications.length === 0) {
    return null;
  }

  return (
    <div className="notification-section">
      <div className="notification-section-header" style={{ color }}>
        <i className={icon}></i>
        <h3>
          {title}
          <span className="notification-section-count">({notifications.length})</span>
        </h3>
      </div>
      {notifications.map(renderNotificationCard)}
    </div>
  );
};
```

**Benefits**:
- ✅ Eliminates 200+ lines of inline styles
- ✅ Consistent with organizers page design pattern
- ✅ CSS :hover states for better performance
- ✅ Easier to maintain and theme
- ✅ Improved accessibility with semantic markup
- ✅ Better responsive behavior

---

## Design Consistency

### Color Variables Used (from [variables.css](c:\Projects\home-registry\frontend\src\styles\variables.css))

```css
--danger-color: #ef4444;    /* Expired warranties */
--warning-color: #f97316;   /* Expiring soon */
--info-color: #3b82f6;      /* Expiring this month */
--accent-color: #f97316;    /* Hover accents */
--border-color: #e2e8f0;    /* Card borders */
--bg-primary: #ffffff;      /* Card backgrounds */
--bg-secondary: #f8fafc;    /* Stat card backgrounds */
--text-primary: #0f172a;    /* Main text */
--text-secondary: #64748b;  /* Secondary text */
--text-tertiary: #94a3b8;   /* Tertiary text */
```

### Shadow Values

```css
--shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.05);
--shadow-md: 0 4px 6px -1px rgb(0 0 0 / 0.1);
--shadow-lg: 0 10px 15px -3px rgb(0 0 0 / 0.1);
```

### Border Radius

```css
--radius-sm: 0.375rem;  /* 6px */
--radius-md: 0.5rem;    /* 8px */
--radius-lg: 0.75rem;   /* 12px */
--radius-xl: 1rem;      /* 16px */
```

---

## Implementation Steps

### Phase 1: Badge Repositioning (Issue #1)

1. **Update** `frontend/src/styles/cards.css` (lines 281-319)
   - Remove `position: absolute`, `top`, `right` properties
   - Add `margin-left: auto` for flex positioning
   
2. **Update** `frontend/src/pages/InventoryDetailPage.tsx` (lines 438-545)
   - Move notification badge JSX from before header to inside footer
   - Place after action buttons but before closing `</div>` of footer

3. **Test**:
   - Verify badge appears at far right of footer
   - Check alignment with buttons
   - Test with/without notification badges present
   - Validate responsive behavior on mobile

### Phase 2: Button Size Fix (Issue #2)

1. **Update** `frontend/src/styles/buttons.css`
   - Add `.btn-inline` class definition before mobile media query
   - Add exception inside `@media (max-width: 480px)`

2. **Update** `frontend/src/pages/NotificationsPage.tsx` (line 348)
   - Add `btn-inline` class to Clear All button

3. **Test**:
   - Verify button stays compact on desktop
   - Verify button stays compact on mobile (< 480px)
   - Check other buttons still work correctly (forms, modals)

### Phase 3: Notification List Redesign (Issue #3)

1. **Create** `frontend/src/styles/notifications.css`
   - Add all notification-specific CSS classes
   - Follow organizers.css pattern for consistency

2. **Update** `frontend/src/pages/NotificationsPage.tsx`
   - Import new CSS module at top
   - Refactor `renderNotificationCard` function (lines 100-227)
   - Refactor `renderSection` function (lines 230-268)
   - Update main render JSX (lines 337-453)

3. **Test**:
   - Verify all three sections render correctly (expired, expiring soon, this month)
   - Test hover states on notification cards
   - Test dismiss button functionality
   - Test click-to-navigate functionality
   - Verify responsive layout on mobile
   - Check dark mode compatibility (if applicable)

### Phase 4: Final Validation

1. **Cross-browser testing**:
   - Chrome, Firefox, Safari, Edge
   - Mobile browsers (iOS Safari, Chrome Mobile)

2. **Accessibility audit**:
   - Keyboard navigation works
   - Screen reader labels are correct
   - Focus states are visible
   - Color contrast meets WCAG AA standards

3. **Performance check**:
   - No layout shifts
   - Smooth animations
   - No JavaScript errors in console

4. **User testing**:
   - Confirm badge positioning makes sense to users
   - Verify Clear All button is discoverable
   - Ensure notification cards are easy to scan

---

## Risk Analysis

### Issue #1 Risks: Badge Repositioning

| Risk | Severity | Mitigation |
|------|----------|------------|
| Badge may not align properly with buttons | Low | Use flexbox `margin-left: auto` which automatically handles alignment |
| Footer may grow too tall with badge | Low | Badge height matches button height; no vertical impact |
| Badge may not be noticeable in new position | Medium | Status colors provide visibility; consider user feedback post-deploy |

### Issue #2 Risks: Button Size

| Risk | Severity | Mitigation |
|------|----------|------------|
| `!important` usage may be problematic | Low | Only used for mobile exception; minimal specificity impact |
| Other buttons may be affected | Low | `.btn-inline` is opt-in class; existing buttons unaffected |
| Flex layout may break on narrow screens | Low | `flex-shrink: 0` prevents compression |

### Issue #3 Risks: Notification List

| Risk | Severity | Mitigation |
|------|----------|------------|
| Large refactor may introduce bugs | Medium | Test all notification types (expired, expiring soon, this month) |
| CSS hover may not work on touch devices | Low | `:hover` fallback to `:active` on mobile; functionality unchanged |
| Removing inline styles may break styling | Medium | Comprehensive CSS coverage ensures all styles are replicated |
| Breaking changes to component API | Low | No prop changes; only internal rendering logic |

### General Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Layout breaks on specific screen sizes | Medium | Test at common breakpoints: 320px, 768px, 1024px, 1440px |
| Dark mode colors may look wrong | Low | All colors use CSS variables; dark mode already defined in variables.css |
| Regression in other pages | Low | Changes are scoped to notifications and item cards; limited blast radius |

---

## Success Criteria

### Issue #1: Badge Positioning
- [ ] Badge appears in bottom-right of item card footer
- [ ] Badge aligns vertically with action buttons
- [ ] Badge maintains color-coded status indicators
- [ ] Badge does not overlap with buttons or card edges
- [ ] Layout remains stable with/without notification badges

### Issue #2: Clear All Button
- [ ] Button uses compact sizing on desktop (auto width)
- [ ] Button remains compact on mobile (does not span full width)
- [ ] Button maintains proper spacing in header
- [ ] Button hover/active states work correctly

### Issue #3: Notification List
- [ ] All inline styles converted to CSS classes
- [ ] Cards match organizers page visual design
- [ ] Hover effects work smoothly via CSS
- [ ] Section headers have proper visual hierarchy
- [ ] Summary statistics cards are visually appealing
- [ ] Responsive layout works on mobile devices
- [ ] Accessibility features maintained (keyboard nav, ARIA labels)
- [ ] Dismiss and navigation functionality preserved

---

## Future Enhancements (Out of Scope)

- **Notification grouping**: Group notifications by inventory
- **Bulk actions**: Select multiple notifications for batch dismiss
- **Filter/sort**: Add filtering by status or date
- **Animation**: Add subtle entrance animations for notification cards
- **Custom notification sounds**: Browser notifications for new alerts
- **Notification preferences**: Per-inventory notification settings

---

## Related Files Reference

### Primary Files to Modify

| File Path | Lines | Change Type |
|-----------|-------|-------------|
| `frontend/src/styles/cards.css` | 281-319 | Modify badge positioning |
| `frontend/src/pages/InventoryDetailPage.tsx` | 438-545 | Move badge to footer |
| `frontend/src/styles/buttons.css` | End of file + 114-117 | Add `.btn-inline` class |
| `frontend/src/pages/NotificationsPage.tsx` | 1-458 | Major refactor with CSS classes |
| `frontend/src/styles/notifications.css` | New file | Create notification styles |

### Reference Files (No Changes)

- `frontend/src/styles/variables.css` - Color and design tokens
- `frontend/src/styles/organizers.css` - Design pattern reference
- `frontend/src/utils/notifications.ts` - Notification logic (unchanged)

---

## Testing Checklist

```markdown
### Issue #1: Badge Positioning
- [ ] Desktop: Badge visible in bottom-right of card footer
- [ ] Desktop: Badge aligns with buttons
- [ ] Mobile (< 768px): Badge positioning maintained
- [ ] Edge case: Card without notifications (no badge shown)
- [ ] Edge case: Long badge text (e.g., "Expiring in 30d")

### Issue #2: Clear All Button
- [ ] Desktop (> 1024px): Button compact, right-aligned
- [ ] Tablet (768px): Button maintains size
- [ ] Mobile (480px): Button stays compact (not full-width)
- [ ] Mobile (320px): Button readable and clickable
- [ ] Edge case: No notifications (button hidden)
- [ ] Edge case: Very long notification count text

### Issue #3: Notification List
- [ ] Desktop: Cards render with proper spacing
- [ ] Desktop: Hover effects work (transform, shadow, color)
- [ ] Desktop: Dismiss button appears on hover
- [ ] Mobile: Touch interactions work
- [ ] Mobile: Cards stack properly
- [ ] All statuses: Expired, expiring soon, this month
- [ ] Empty states: No notifications shown correctly
- [ ] Keyboard nav: Tab through cards, Enter to open
- [ ] Screen reader: Proper ARIA labels read
- [ ] Color contrast: Passes WCAG AA (4.5:1 text, 3:1 UI)
- [ ] Dark mode: Colors work in both themes

### Integration Testing
- [ ] Navigate from notification to item detail page
- [ ] Dismiss notification and verify UI updates
- [ ] Clear all notifications and verify empty state
- [ ] Add new item with warranty, verify notification appears
- [ ] Browser console: No errors or warnings
```

---

## Appendix: Code Snippets

### A. Current Badge CSS (BEFORE)

```css
/* frontend/src/styles/cards.css:281-319 */
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
```

### B. Organizer Card CSS (REFERENCE)

```css
/* frontend/src/styles/organizers.css:7-21 */
.organizer-card {
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  padding: 1.25rem;
  transition:
    box-shadow 0.2s ease,
    border-color 0.2s ease;
}

.organizer-card:hover {
  border-color: var(--accent-color);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}
```

### C. Design Token Examples

```css
/* Common color usage patterns */

/* Error/Danger (Expired warranties) */
color: var(--danger-color);           /* #ef4444 */
background: rgba(239, 68, 68, 0.1);   /* 10% opacity background */
border: 1px solid var(--danger-color);

/* Warning (Expiring soon) */
color: var(--warning-color);          /* #f97316 */
background: rgba(245, 158, 11, 0.1);
border: 1px solid var(--warning-color);

/* Info (Expiring this month) */
color: var(--info-color);             /* #3b82f6 */
background: rgba(59, 130, 246, 0.1);
border: 1px solid var(--info-color);

/* Accent (Hover states) */
border-color: var(--accent-color);    /* #f97316 */
color: var(--accent-color);

/* Shadows */
box-shadow: var(--shadow-md);         /* Hover elevation */
box-shadow: var(--shadow-sm);         /* Subtle depth */
```

---

## Document Control

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-02-14 | Research Subagent | Initial specification |

**Next Steps**: Implementation → Code Review → QA Testing → Deploy

---

**End of Specification**
