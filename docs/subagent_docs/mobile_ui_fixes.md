# Mobile UI Fixes — Sidebar Blocking Main Content

**Date:** 2026-02-16  
**Status:** Specification  
**Priority:** CRITICAL — App is completely unusable on mobile devices

---

## 1. Current State Analysis

### 1.1 The Problem

On mobile devices (viewport ≤ 768px), the sidebar navigation panel occupies the **entire screen width** and **never hides**, completely blocking access to the main content area. Users on phones and tablets see only the sidebar and cannot interact with inventories, settings, or any other page content.

### 1.2 Root Cause — Exact CSS Rules

#### `frontend/src/styles/sidebar.css` (Lines 243–258)

```css
@media (max-width: 768px) {
  .sidebar {
    transform: translateX(0);   /* ← BUG: Sidebar stays fully visible */
    width: 100%;                /* ← BUG: Expands to fill entire viewport */
    z-index: 1001;              /* ← Sits above everything */
  }
}
```

**Problems:**
1. `transform: translateX(0)` — The sidebar is **never hidden**. It should default to `translateX(-100%)` (off-screen left) and only show when toggled.
2. `width: 100%` — Takes up the entire viewport width, leaving zero space for main content.
3. `z-index: 1001` — Ensures the sidebar sits on top of the main content, making it inaccessible.

#### `frontend/src/styles/layout.css` (Lines 410–422)

```css
@media (max-width: 768px) {
  .main-content {
    margin-left: 0;   /* ← Correct removal of left margin, but sidebar covers it anyway */
  }
}
```

The `margin-left: 0` is correct, but irrelevant since the sidebar is a `position: fixed` element covering the viewport.

### 1.3 What's Missing

| Missing Feature | Impact |
|----------------|--------|
| **Sidebar toggle state** | No mechanism to show/hide sidebar on mobile |
| **Hamburger menu button** | No way for users to open/close the sidebar |
| **Backdrop overlay** | No semi-transparent overlay behind open sidebar |
| **Body scroll lock** | Page scrolls behind sidebar when it's open |
| **Auto-close on navigation** | Sidebar stays open after navigating to a page |
| **Auto-close on resize** | Sidebar doesn't close when viewport exceeds mobile breakpoint |

### 1.4 Sidebar Component Structure

**`frontend/src/components/Sidebar.tsx`** — Stateless component, no toggle logic:

```tsx
export function Sidebar({ currentPage, onNavigate }: SidebarProps) {
  return (
    <aside className="sidebar">
      {/* Logo header */}
      {/* Nav items: Inventories, Organizers, Notifications, Settings */}
    </aside>
  );
}
```

**`frontend/src/App.tsx`** — Layout rendering (no mobile toggle logic):

```tsx
return (
  <>
    <Sidebar currentPage={getCurrentPage()} onNavigate={handleNavigate} />
    <main className="main-content">
      {/* Routes */}
    </main>
  </>
);
```

**`frontend/src/components/Header.tsx`** — No hamburger menu button exists. Header only has notification bell, theme toggle, and user menu.

---

## 2. Humidor Project Reference Patterns

The Humidor project (`analysis/humidor/`) implements mobile sidebar correctly. Here is the complete pattern to follow:

### 2.1 HTML Structure (Humidor)

```html
<div id="app">
  <!-- Backdrop overlay behind sidebar -->
  <div class="sidebar-backdrop" id="sidebarBackdrop"></div>
  
  <!-- Sidebar (hidden by default on mobile via CSS transform) -->
  <aside class="sidebar" id="sidebar">
    <nav class="sidebar-nav">
      <!-- Nav items -->
    </nav>
  </aside>

  <main class="main-content">
    <header class="page-header">
      <div class="header-left">
        <!-- Hamburger menu button (hidden on desktop) -->
        <button class="mobile-menu-toggle" id="mobileMenuToggle" aria-label="Toggle menu">
          <span class="mobile-menu-icon mdi mdi-menu"></span>
        </button>
        <div class="app-logo">...</div>
      </div>
    </header>
  </main>
</div>
```

### 2.2 CSS Pattern (Humidor)

```css
/* Mobile menu button — hidden on desktop */
.mobile-menu-toggle {
  display: none;
  width: 40px;
  height: 40px;
  background: var(--surface-color);
  border: 1px solid var(--border-color);
  border-radius: 0.5rem;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
  touch-action: manipulation;
}

/* Backdrop — hidden by default */
.sidebar-backdrop {
  display: none;
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(2px);
  z-index: 99;
  opacity: 0;
  transition: opacity 0.3s ease;
}

.sidebar-backdrop.show {
  display: block;
  opacity: 1;
}

/* Mobile breakpoint */
@media (max-width: 1024px) {
  .mobile-menu-toggle { display: flex; }
  
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    height: 100vh;
    width: 280px;
    max-width: 80vw;
    transform: translateX(-100%);       /* Hidden off-screen */
    transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
    z-index: 1100;
  }
  
  .sidebar.open {
    transform: translateX(0);            /* Slide in when toggled */
    box-shadow: 4px 0 20px var(--shadow-color);
  }
  
  .main-content {
    margin-left: 0;
    width: 100%;
  }
}
```

### 2.3 JavaScript Toggle Logic (Humidor)

```javascript
function toggleMobileMenu() {
  const sidebar = document.getElementById('sidebar');
  const backdrop = document.getElementById('sidebarBackdrop');
  const isOpen = sidebar.classList.contains('open');
  isOpen ? closeMobileMenu() : openMobileMenu();
}

function openMobileMenu() {
  sidebar.classList.add('open');
  backdrop.classList.add('show');
  document.body.style.overflow = 'hidden';  // Lock page scroll
}

function closeMobileMenu() {
  sidebar.classList.remove('open');
  backdrop.classList.remove('show');
  document.body.style.overflow = '';         // Restore scroll
}

// Close on nav item click
// Close on backdrop click
// Close on window resize above breakpoint
```

---

## 3. Implementation Plan

### 3.1 Breakpoint Strategy

| Breakpoint | Behavior |
|-----------|----------|
| `> 768px` (desktop) | Sidebar permanently visible, 280px fixed left |
| `≤ 768px` (mobile/tablet) | Sidebar hidden off-screen, hamburger button visible, sidebar slides in as overlay |

Using 768px as the breakpoint to match existing media queries throughout the project.

### 3.2 Files to Modify

| File | Changes |
|------|---------|
| `frontend/src/styles/sidebar.css` | Fix mobile media query, add `.sidebar.open` state, add backdrop styles, add mobile-menu-toggle styles |
| `frontend/src/styles/layout.css` | Add hamburger button styles for header, adjust header mobile styles |
| `frontend/src/components/Sidebar.tsx` | Add `isOpen` prop, add `open` CSS class, add backdrop element |
| `frontend/src/components/Header.tsx` | Add hamburger menu button (visible only on mobile) |
| `frontend/src/App.tsx` | Add `sidebarOpen` state, toggle handlers, pass props to Sidebar/Header, auto-close on navigate |

### 3.3 Detailed Changes

---

#### 3.3.1 `frontend/src/styles/sidebar.css` — Fix Mobile Sidebar

**Replace the existing mobile media queries (lines 242–275) with:**

```css
/* Mobile menu toggle button - hidden on desktop */
.mobile-menu-toggle {
  display: none;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  min-width: 40px;
  min-height: 40px;
  background: none;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all 0.2s ease;
  color: var(--text-primary);
  -webkit-tap-highlight-color: transparent;
  touch-action: manipulation;
  flex-shrink: 0;
}

.mobile-menu-toggle:hover {
  background-color: var(--bg-tertiary);
  border-color: var(--accent-color);
  color: var(--accent-color);
}

.mobile-menu-toggle:active {
  transform: scale(0.95);
}

.mobile-menu-toggle i {
  font-size: 1.25rem;
  pointer-events: none;
}

/* Sidebar backdrop overlay */
.sidebar-backdrop {
  display: none;
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(2px);
  z-index: 1000;
  opacity: 0;
  transition: opacity 0.3s ease;
  -webkit-tap-highlight-color: transparent;
}

.sidebar-backdrop.visible {
  display: block;
  opacity: 1;
}

/* Mobile Responsiveness */
@media (max-width: 768px) {
  /* Show hamburger button */
  .mobile-menu-toggle {
    display: flex;
  }

  /* Sidebar hidden off-screen by default */
  .sidebar {
    transform: translateX(-100%);
    width: 280px;
    max-width: 85vw;
    z-index: 1001;
    box-shadow: none;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
  }

  /* Sidebar visible when open */
  .sidebar.open {
    transform: translateX(0);
    box-shadow: 4px 0 20px rgba(0, 0, 0, 0.3);
  }

  .sidebar-header {
    padding: 1.5rem 1rem;
    min-height: 100px;
  }

  .logo img {
    max-height: 65px;
    max-width: 170px;
  }

  /* Touch-friendly nav items */
  .nav-item {
    min-height: 44px;
    -webkit-tap-highlight-color: transparent;
  }
}

@media (max-width: 480px) {
  .sidebar {
    width: 100%;
    max-width: 100vw;
  }

  .sidebar-header {
    padding: 1rem;
    min-height: 90px;
  }

  .logo img {
    max-height: 50px;
    max-width: 140px;
  }
}
```

**Key changes:**
- Added `.mobile-menu-toggle` button styles (hidden on desktop, visible on mobile)
- Added `.sidebar-backdrop` overlay styles
- Changed `transform: translateX(0)` → `translateX(-100%)` to hide sidebar on mobile by default
- Added `.sidebar.open` class with `translateX(0)` to show sidebar when toggled
- Changed `width: 100%` → `width: 280px; max-width: 85vw` to not fill entire screen (except on small phones)
- Added touch-friendly 44px minimum height on nav items
- Added `-webkit-overflow-scrolling: touch` for smooth scrolling on iOS

---

#### 3.3.2 `frontend/src/styles/layout.css` — Header Mobile Adjustments

The existing mobile media query at line 410 is fine (`margin-left: 0`). No changes needed to `layout.css` since the hamburger button styling is in `sidebar.css` and the button itself goes in the Header component.

However, the header-actions section at 480px breakpoint should avoid stacking vertically (it breaks the layout when hamburger is present):

**Replace the 480px media query (lines 429-433) with:**

```css
@media (max-width: 480px) {
  .header {
    padding: 1rem 0.75rem;
    height: auto;
    min-height: 80px;
  }

  .page-title {
    font-size: 1.25rem;
  }

  .page-subtitle {
    font-size: 0.75rem;
  }

  .header-actions {
    gap: 0.5rem;
  }

  .header-actions .theme-toggle {
    width: 2rem;
    height: 2rem;
    font-size: 0.9rem;
  }

  /* Hide user details text on very small screens */
  .user-menu-trigger .user-details {
    display: none;
  }

  .user-menu-trigger .menu-chevron {
    display: none;
  }
}
```

---

#### 3.3.3 `frontend/src/components/Sidebar.tsx` — Add Open/Close Props

**Updated component:**

```tsx
import { useEffect } from 'react';

interface SidebarProps {
  currentPage: string;
  onNavigate: (page: string) => void;
  isOpen: boolean;
  onClose: () => void;
}

export function Sidebar({ currentPage, onNavigate, isOpen, onClose }: SidebarProps) {
  // Close sidebar on window resize above mobile breakpoint
  useEffect(() => {
    const handleResize = () => {
      if (window.innerWidth > 768 && isOpen) {
        onClose();
      }
    };

    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [isOpen, onClose]);

  // Lock body scroll when sidebar is open on mobile
  useEffect(() => {
    if (isOpen) {
      document.body.style.overflow = 'hidden';
    } else {
      document.body.style.overflow = '';
    }
    return () => {
      document.body.style.overflow = '';
    };
  }, [isOpen]);

  const handleNavigate = (page: string) => {
    onNavigate(page);
    onClose(); // Auto-close sidebar on navigation (mobile)
  };

  return (
    <>
      {/* Backdrop overlay */}
      <div
        className={`sidebar-backdrop ${isOpen ? 'visible' : ''}`}
        onClick={onClose}
        aria-hidden="true"
      />

      <aside className={`sidebar ${isOpen ? 'open' : ''}`}>
        <div className="sidebar-header">
          <a href="/" className="logo">
            <img src="/logo_full.png" alt="Home Registry" />
          </a>
        </div>

        <nav className="nav-menu">
          <div className="nav-section">
            <div className="nav-section-title">Overview</div>
            <button
              className={`nav-item ${currentPage === 'inventories' ? 'active' : ''}`}
              onClick={() => handleNavigate('inventories')}
            >
              <i className="fas fa-warehouse"></i>
              <span>Inventories</span>
            </button>
            <button
              className={`nav-item ${currentPage === 'organizers' ? 'active' : ''}`}
              onClick={() => handleNavigate('organizers')}
            >
              <i className="fas fa-folder-tree"></i>
              <span>Organizers</span>
            </button>
          </div>

          <div className="sidebar-bottom">
            <button
              className={`nav-item nav-item-notifications ${currentPage === 'notifications' ? 'active' : ''}`}
              onClick={() => handleNavigate('notifications')}
            >
              <i className="fas fa-bell"></i>
              <span>Notifications</span>
            </button>

            <div className="nav-section system-section">
              <div className="nav-section-title">System</div>
              <button
                className={`nav-item ${currentPage === 'settings' ? 'active' : ''}`}
                onClick={() => handleNavigate('settings')}
              >
                <i className="fas fa-cog"></i>
                <span>Settings</span>
              </button>
            </div>
          </div>
        </nav>
      </aside>
    </>
  );
}
```

**Key changes:**
- Added `isOpen` and `onClose` props
- Applies `open` CSS class when `isOpen` is true
- Renders backdrop overlay that calls `onClose` on click
- Locks body scroll when sidebar is open
- Auto-closes sidebar when a nav item is clicked (via `handleNavigate` wrapper)
- Auto-closes sidebar when window resizes above 768px

---

#### 3.3.4 `frontend/src/components/Header.tsx` — Add Hamburger Menu Button

**Add a hamburger button as the first element inside `header-content`:**

```tsx
interface HeaderProps {
  title: string;
  subtitle: string;
  icon?: string;
  onMenuToggle?: () => void;   // NEW PROP
}

export function Header({ title, subtitle, icon, onMenuToggle }: HeaderProps) {
  // ... existing code ...

  return (
    <header className="header">
      <div className="header-content">
        {/* Hamburger menu button — visible only on mobile via CSS */}
        {onMenuToggle && (
          <button
            className="mobile-menu-toggle"
            onClick={onMenuToggle}
            aria-label="Toggle navigation menu"
            type="button"
          >
            <i className="fas fa-bars"></i>
          </button>
        )}

        <div className="page-title-section">
          {/* ... existing title/subtitle ... */}
        </div>
        <div className="header-actions">
          {/* ... existing actions ... */}
        </div>
      </div>
    </header>
  );
}
```

**Key changes:**
- Added optional `onMenuToggle` prop
- Renders a hamburger button with `fa-bars` icon
- Button has `.mobile-menu-toggle` class (hidden on desktop, visible via CSS on ≤768px)
- Only renders when `onMenuToggle` is provided (doesn't affect auth pages)

---

#### 3.3.5 `frontend/src/App.tsx` — Wire Up Sidebar State

**Add state management in `AppContent`:**

```tsx
import { useState, useCallback } from 'react';

function AppContent() {
  const [sidebarOpen, setSidebarOpen] = useState(false);
  // ... existing hooks ...

  const toggleSidebar = useCallback(() => {
    setSidebarOpen(prev => !prev);
  }, []);

  const closeSidebar = useCallback(() => {
    setSidebarOpen(false);
  }, []);

  // ... existing code ...

  return (
    <>
      <Sidebar
        currentPage={getCurrentPage()}
        onNavigate={handleNavigate}
        isOpen={sidebarOpen}
        onClose={closeSidebar}
      />
      <InstructionsModal />
      <main className="main-content">
        <Routes>
          {/* Each page's Header gets onMenuToggle prop */}
          {/* ... */}
        </Routes>
      </main>
      <Toast />
    </>
  );
}
```

**Note:** The `onMenuToggle={toggleSidebar}` prop needs to be passed to the `Header` component inside each page. Since each page renders its own `<Header>`, there are two approaches:

**Option A (Recommended):** Pass `toggleSidebar` through React Context or as a prop to each page component.

**Option B:** Create a context to expose the toggle function.

**Recommended approach — App-level context:**

Add to `AppContext` (or create a new `LayoutContext`):

```tsx
// In AppContext or new LayoutContext:
const [sidebarOpen, setSidebarOpen] = useState(false);
const toggleSidebar = useCallback(() => setSidebarOpen(prev => !prev), []);
const closeSidebar = useCallback(() => setSidebarOpen(false), []);

// Provide: { sidebarOpen, toggleSidebar, closeSidebar }
```

Then in each page's `<Header>` usage:

```tsx
const { toggleSidebar } = useApp(); // or useLayout()
<Header title="..." subtitle="..." onMenuToggle={toggleSidebar} />
```

**Alternative simpler approach:** Since `App.tsx` renders the `<Sidebar>` directly, and each page renders its own `<Header>`, the simplest solution is to add the sidebar state to the existing `AppContext` which is already available globally.

---

## 4. Summary of All Changes

### Files to Modify (5 files)

| # | File | Type of Change |
|---|------|---------------|
| 1 | `frontend/src/styles/sidebar.css` | Replace mobile media queries, add hamburger & backdrop CSS |
| 2 | `frontend/src/styles/layout.css` | Update 480px breakpoint for better mobile header |
| 3 | `frontend/src/components/Sidebar.tsx` | Add `isOpen`/`onClose` props, backdrop, auto-close, scroll lock |
| 4 | `frontend/src/components/Header.tsx` | Add `onMenuToggle` prop and hamburger button |
| 5 | `frontend/src/App.tsx` | Add `sidebarOpen` state, wire up toggle/close to Sidebar |

### Additionally, the toggle function should be exposed via context:

| # | File | Type of Change |
|---|------|---------------|
| 6 | `frontend/src/context/AppContext.tsx` | Add `sidebarOpen`, `toggleSidebar`, `closeSidebar` to context |
| 7 | All page components using `<Header>` | Pass `onMenuToggle={toggleSidebar}` from context |

### Pages that render `<Header>` (need `onMenuToggle` prop):

Check each page in `frontend/src/pages/` that uses `<Header>` and ensure they pass `onMenuToggle`.

---

## 5. Mobile Breakpoint Strategy

```
┌─────────────────────────────────────────────────────┐
│  > 768px  │  Desktop: Sidebar fixed, always visible │
├───────────┼─────────────────────────────────────────┤
│ ≤ 768px   │  Mobile: Sidebar hidden, hamburger      │
│           │  button visible. Sidebar slides in as   │
│           │  overlay with backdrop.                  │
├───────────┼─────────────────────────────────────────┤
│ ≤ 480px   │  Small phone: Sidebar width 100%.       │
│           │  Header compact, user details hidden.   │
└───────────┴─────────────────────────────────────────┘
```

This matches the existing breakpoint strategy used across all other CSS files in the project (768px and 480px breakpoints).

---

## 6. Touch-Friendly Interactions

| Element | Minimum Size | Behavior |
|---------|-------------|----------|
| Hamburger button | 40×40px (min 44px tap target) | Opens sidebar overlay |
| Nav items | 44px min-height | Close sidebar + navigate |
| Backdrop | Full viewport | Closes sidebar on tap |
| Sidebar | 280px / 85vw on mobile | Slides in from left, smooth 0.3s transition |
| Theme toggle | 40×40px → 32×32px on small phones | Same behavior |

---

## 7. Potential Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Sidebar scroll doesn't work on iOS | Add `-webkit-overflow-scrolling: touch` |
| Body scrolls behind open sidebar | Lock body scroll with `overflow: hidden` |
| Sidebar stays open after resize to desktop | UseEffect listener closes sidebar when viewport > 768px |
| Flash of sidebar on initial mobile load | Sidebar starts hidden via CSS (`transform: translateX(-100%)`), no flash |
| Focus management (a11y) | Use `aria-label` on hamburger button, `aria-hidden` on backdrop |
| Memory leak from resize listener | Clean up in useEffect return |

---

## 8. Testing Checklist

- [ ] Mobile (≤768px): Sidebar hidden by default, only hamburger visible
- [ ] Hamburger click: Sidebar slides in from left with backdrop
- [ ] Backdrop click: Sidebar closes
- [ ] Nav item click: Sidebar closes and page navigates
- [ ] Body scroll: Locked when sidebar open, restored when closed
- [ ] Resize to desktop: Sidebar auto-closes overlay mode, shows fixed
- [ ] Desktop (>768px): No hamburger visible, sidebar always shown
- [ ] 480px: Sidebar fills 100% width, header compact
- [ ] iOS Safari: Smooth scrolling in sidebar, no body scroll behind
- [ ] Theme toggle: Works correctly on mobile
- [ ] Keyboard: Can tab to hamburger, Enter opens sidebar
