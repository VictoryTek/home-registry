# Rate Limiting Issue - Diagnostic and Fix Specification

**Date:** February 12, 2026  
**Issue:** Severe rate limiting with hundreds of requests causing 429 errors  
**Status:** Root cause identified - React hook dependency infinite loop

---

## Executive Summary

The rate limiting issue is **NOT** caused by insufficient rate limits. The root cause is an **infinite request loop** in the frontend React components caused by improper `useCallback` dependency management. This creates hundreds of requests per second, overwhelming even generous rate limits (100 RPS, 200 burst).

**Primary Issue:** React hook dependency cycle  
**Secondary Issues:** No retry logic, global rate limiter scope, CSP violations  
**Impact:** Application unusable, all API requests return 429  
**Fix Priority:** CRITICAL - Frontend first, then backend improvements

---

## Current State Analysis

### Backend Rate Limiter Configuration
**File:** `src/main.rs` (lines 72-99)

```rust
let requests_per_second = env::var("RATE_LIMIT_RPS")
    .unwrap_or_else(|_| "50".to_string())
    .parse::<u64>()
    .unwrap_or(50);

let burst_size = env::var("RATE_LIMIT_BURST")
    .unwrap_or_else(|_| "100".to_string())
    .parse::<u32>()
    .unwrap_or(100);

let governor_conf = GovernorConfigBuilder::default()
    .requests_per_second(requests_per_second)
    .burst_size(burst_size)
    .finish()
    .expect("Failed to build rate limiter configuration");
```

**Configuration:**
- Algorithm: GCRA (Generic Cell Rate Algorithm) via `actix-governor` 0.8.0
- Default: 50 requests/second, 100 burst
- Environment Variables: `RATE_LIMIT_RPS`, `RATE_LIMIT_BURST`
- Scope: **Global** - applied to ALL routes (line 118: `.wrap(Governor::new(&governor_conf))`)

**Problems:**
1. ✅ Rate limit algorithm is correct (GCRA)
2. ❌ Applied globally to static assets, health checks, logos, etc.
3. ❌ No per-endpoint customization
4. ❌ No distinction between authenticated vs unauthenticated
5. ✅ Configuration is environment-variable driven (good)

### Frontend API Client
**File:** `frontend/src/services/api.ts`

**Problems Identified:**
1. ❌ **NO retry logic** - Failed requests don't retry
2. ❌ **NO exponential backoff** - Immediate failure on 429
3. ❌ **NO Retry-After header parsing** - Ignores server hints
4. ✅ Basic error handling exists (401 redirect, JSON validation)

```typescript
// Current handleResponse - NO retry logic
async function handleResponse<T>(response: Response): Promise<ApiResponse<T>> {
  if (response.status === 401) {
    // Handles 401, but NOT 429
    localStorage.removeItem(TOKEN_KEY);
    window.location.href = '/login';
  }
  // ... continues without retry
}
```

### Frontend Component Request Patterns
**File:** `frontend/src/pages/InventoriesPage.tsx` (lines 30-74)

**THE ROOT CAUSE - INFINITE LOOP:**

```tsx
// Line 11: Context destructuring
const { showToast, inventories, setInventories, setItems } = useApp();

// Lines 30-71: loadInventories with problematic dependencies
const loadInventories = useCallback(async () => {
  setLoading(true);
  try {
    const result = await inventoryApi.getAll();
    if (result.success && result.data) {
      setInventories(result.data);  // ← Triggers context re-render
      // ... more code ...
      setItems(allItems);  // ← Triggers context re-render
    }
  } catch {
    showToast('Failed to load inventories', 'error');
  } finally {
    setLoading(false);
  }
}, [showToast, setItems, setInventories]); // ← PROBLEMATIC DEPENDENCIES

// Lines 72-74: Effect that depends on loadInventories
useEffect(() => {
  void loadInventories();
}, [loadInventories]); // ← Re-runs when loadInventories changes
```

**Why This Creates an Infinite Loop:**

1. **Component mounts** → `useEffect` runs → calls `loadInventories()`
2. **loadInventories executes** → calls `setInventories(data)` and `setItems(data)`
3. **AppContext re-renders** → Context state changes (inventories, items updated)
4. **Context re-render creates new function references** for `showToast`, `setInventories`, `setItems` (NOT wrapped in useCallback in AppContext.tsx)
5. **New function references trigger useCallback** → creates new `loadInventories` function
6. **New loadInventories triggers useEffect** → goes back to step 2
7. **INFINITE LOOP** → Hundreds of requests per second until rate limit hit

**File:** `frontend/src/context/AppContext.tsx`

```tsx
export function AppProvider({ children }: { children: ReactNode }) {
  const [inventories, setInventories] = useState<Inventory[]>([]);
  const [items, setItems] = useState<Item[]>([]);
  
  // ❌ showToast is NOT wrapped in useCallback
  const showToast = (message: string, type: ToastMessage['type']) => {
    const id = Date.now().toString();
    setToasts(prev => [...prev, { id, message, type }]);
    setTimeout(() => removeToast(id), 3000);
  };
  
  // ❌ setInventories and setItems are state setters (stable, but not explicitly memoized)
  
  return (
    <AppContext.Provider value={{
      showToast,        // NEW reference on every render
      setInventories,   // Stable from useState, but passed as dependency
      setItems,         // Stable from useState, but passed as dependency
      // ...
    }}>
      {children}
    </AppContext.Provider>
  );
}
```

**Same Issue in OrganizersPage.tsx** (lines 69-89):
```tsx
const loadInventories = useCallback(async () => {
  // ...
}, []);  // Empty dependencies, but still problematic

useEffect(() => {
  if (inventoryId) {
    void loadData();
  } else {
    void loadInventories();
  }
}, [inventoryId, loadData, loadInventories]); // ← loadInventories in deps
```

### CSP Violations (Secondary Issue)
**File:** `src/main.rs` (lines 128-137)

```rust
.add(("Content-Security-Policy", 
      "default-src 'self'; \
       script-src 'self' 'unsafe-inline' 'unsafe-eval'; \
       style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdnjs.cloudflare.com; \
       img-src 'self' data: https:; \
       font-src 'self' https://fonts.gstatic.com data:; \
       connect-src 'self'; \
       frame-ancestors 'none'"))
```

**Problems:**
- ✅ `style-src` already allows `https://fonts.googleapis.com` and `https://cdnjs.cloudflare.com`
- ✅ `font-src` already allows `https://fonts.gstatic.com`
- ❓ CSP violations may be from Font Awesome CDN not being explicitly listed
- ❓ May need to add `https://use.fontawesome.com` or similar

---

## Research: Best Practices

### 1. GCRA Rate Limiting Algorithm (Governor Library)
**Source:** Governor documentation (from Context7)

**Key Findings:**
- ✅ GCRA (Generic Cell Rate Algorithm) is industry-standard
- ✅ `actix-governor` 0.8.0 uses Governor 0.6+ with GCRA
- ✅ Supports burst capacity (allow temporary spikes)
- ✅ More accurate than simple token bucket

**Configuration Best Practices:**
```rust
// Recommended pattern from Governor docs
let quota = Quota::per_second(nonzero!(50u32))
    .allow_burst(nonzero!(100u32));

// For keyed rate limiting (per-user):
let limiter = RateLimiter::keyed(Quota::per_minute(nonzero!(100u32)));
```

**Recommendation:** Current algorithm is correct, but scope needs adjustment.

### 2. Actix-Web Rate Limiting Middleware
**Source:** actix-extras documentation

**Key Findings:**
- ✅ `actix-governor` is the recommended crate (we're using it)
- ✅ Can be applied globally or per-route
- ❌ Current implementation applies globally (affects static assets)
- ✅ Supports custom key extraction (per-IP, per-user, per-endpoint)

**Best Practice Pattern:**
```rust
// Apply only to API routes
.service(
    web::scope("/api")
        .wrap(Governor::new(&governor_conf))
        .service(api::init_routes())
)
```

### 3. Frontend Retry Logic with Exponential Backoff
**Source:** MDN Web Docs - HTTP 429 Status

**Key Findings:**
- ✅ HTTP 429 should return `Retry-After` header (in seconds or HTTP date)
- ✅ Clients should respect `Retry-After` before retrying
- ✅ Exponential backoff prevents thundering herd
- Standard pattern: 1s → 2s → 4s → 8s → 16s (with jitter)

**Recommended Implementation:**
```typescript
async function fetchWithRetry<T>(
  url: string,
  options: RequestInit,
  maxRetries = 3
): Promise<Response> {
  let lastError: Error;
  
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      const response = await fetch(url, options);
      
      if (response.status === 429) {
        if (attempt === maxRetries) {
          throw new Error('Rate limit exceeded after retries');
        }
        
        // Parse Retry-After header
        const retryAfter = response.headers.get('Retry-After');
        let waitMs = Math.min(Math.pow(2, attempt) * 1000, 32000); // Max 32s
        
        if (retryAfter) {
          const retrySeconds = parseInt(retryAfter, 10);
          if (!isNaN(retrySeconds)) {
            waitMs = retrySeconds * 1000;
          }
        }
        
        // Add jitter (±25%)
        waitMs = waitMs * (0.75 + Math.random() * 0.5);
        
        await new Promise(resolve => setTimeout(resolve, waitMs));
        continue;
      }
      
      return response;
    } catch (error) {
      lastError = error as Error;
      if (attempt === maxRetries) throw lastError;
      
      // Exponential backoff for network errors
      const waitMs = Math.pow(2, attempt) * 1000;
      await new Promise(resolve => setTimeout(resolve, waitMs));
    }
  }
  
  throw lastError!;
}
```

### 4. React useCallback and useEffect Dependencies
**Source:** React documentation, React hook best practices

**Key Findings:**
- ✅ Functions from context should be wrapped in `useCallback`
- ✅ State setters from `useState` are stable (don't need `useCallback`)
- ❌ Including unstable functions in `useCallback` deps causes infinite loops
- ✅ Solution: Wrap context functions in `useCallback` or use empty deps array

**Best Practice:**
```typescript
// In AppContext.tsx - wrap functions in useCallback
const showToast = useCallback((message: string, type: ToastMessage['type']) => {
  const id = Date.now().toString();
  setToasts(prev => [...prev, { id, message, type }]);
  setTimeout(() => removeToast(id), 3000);
}, []); // Empty deps - showToast is now stable

// In component - DON'T include setState in deps if not needed
const loadData = useCallback(async () => {
  const result = await api.getData();
  setData(result); // setState is stable, but don't include in deps
}, []); // Empty deps - loadData is stable
```

### 5. Per-Endpoint vs Global Rate Limiting
**Source:** Industry best practices, API design

**Key Findings:**
- ✅ Static assets should NOT be rate limited
- ✅ Health checks should have generous limits (monitoring)
- ✅ Expensive operations (DB queries) should have strict limits
- ✅ Different endpoints have different capacity needs

**Recommended Strategy:**
```rust
// Health checks - no rate limit
.route("/health", web::get().to(health))

// Static assets - no rate limit or very generous
.service(fs::Files::new("/assets", "static/assets"))

// API routes - strict rate limiting
.service(
    web::scope("/api")
        .wrap(Governor::new(&governor_conf))
        .service(api::init_routes())
)
```

### 6. CSP Configuration for External Resources
**Source:** MDN Content Security Policy documentation

**Key Findings:**
- ✅ Need to explicitly list all external domains
- ✅ Google Fonts requires: `fonts.googleapis.com` (styles) and `fonts.gstatic.com` (fonts)
- ✅ Font Awesome CDN needs: `use.fontawesome.com` or `cdnjs.cloudflare.com`
- ✅ Use specific domains instead of wildcard `https:` when possible

**Recommended CSP for this app:**
```rust
"default-src 'self'; \
 script-src 'self' 'unsafe-inline' 'unsafe-eval' https://use.fontawesome.com https://cdnjs.cloudflare.com; \
 style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://use.fontawesome.com https://cdnjs.cloudflare.com; \
 font-src 'self' https://fonts.gstatic.com https://use.fontawesome.com https://cdnjs.cloudflare.com data:; \
 img-src 'self' data: https:; \
 connect-src 'self'; \
 frame-ancestors 'none'"
```

---

## Root Cause Summary

### Primary Cause (99% of the problem)
**React Hook Dependency Infinite Loop**

1. `InventoriesPage` and `OrganizersPage` have `loadInventories` wrapped in `useCallback`
2. Dependencies include context functions: `showToast`, `setInventories`, `setItems`
3. `AppContext.tsx` does NOT wrap `showToast` in `useCallback`
4. Every context re-render creates new `showToast` reference
5. New reference triggers `useCallback` to recreate `loadInventories`
6. New `loadInventories` triggers `useEffect` to run again
7. **Result:** Hundreds of API requests per second until rate limit is hit
8. All requests return 429, application becomes unusable

### Contributing Factors (Minor)
- No retry logic with exponential backoff in frontend
- Rate limiter applied globally (affects static assets unnecessarily)
- No Retry-After header parsing
- CSP violations causing separate console errors

---

## Proposed Solution Architecture

### Phase 1: Fix React Hook Dependencies (CRITICAL - Do First)

**Goal:** Stop the infinite request loop

**Files to Modify:**
1. `frontend/src/context/AppContext.tsx`
2. `frontend/src/pages/InventoriesPage.tsx`
3. `frontend/src/pages/InventoryDetailPage.tsx`
4. `frontend/src/pages/OrganizersPage.tsx`

**Changes:**

#### 1.1 Wrap Context Functions in useCallback
**File:** `frontend/src/context/AppContext.tsx`

```typescript
// Wrap showToast in useCallback to prevent re-creation
const showToast = useCallback((message: string, type: ToastMessage['type']) => {
  const id = Date.now().toString();
  setToasts(prev => [...prev, { id, message, type }]);
  
  setTimeout(() => {
    removeToast(id);
  }, 3000);
}, []); // No dependencies - function is stable

// removeToast should also be wrapped
const removeToast = useCallback((id: string) => {
  setToasts(prev => prev.filter(toast => toast.id !== id));
}, []);

// toggleTheme is already stable, but wrap for consistency
const toggleTheme = useCallback(() => {
  setTheme(prev => prev === 'light' ? 'dark' : 'light');
}, []);

// checkNotifications already wrapped - ensure it stays that way
const checkNotifications = useCallback(() => {
  const notifications = checkWarrantyNotifications(items);
  setWarrantyNotifications(notifications);
}, [items]); // items is the only dependency
```

#### 1.2 Remove Unnecessary Dependencies from useCallback
**File:** `frontend/src/pages/InventoriesPage.tsx`

```typescript
// CURRENT (BROKEN):
const loadInventories = useCallback(async () => {
  // ... implementation ...
}, [showToast, setItems, setInventories]); // ❌ PROBLEMATIC

// FIXED:
const loadInventories = useCallback(async () => {
  setLoading(true);
  try {
    const result = await inventoryApi.getAll();
    if (result.success && result.data) {
      setInventories(result.data);
      
      // ... item loading logic ...
      
      setItems(allItems);
    } else {
      showToast(result.error ?? 'Failed to load inventories', 'error');
    }
  } catch {
    showToast('Failed to load inventories', 'error');
  } finally {
    setLoading(false);
  }
}, []); // ✅ EMPTY - setState functions are stable, showToast now stable
```

**Explanation:**
- `setInventories` and `setItems` are from `useState` → **stable references** (React guarantees this)
- `showToast` is now wrapped in `useCallback` in AppContext → **stable reference**
- `setLoading` (local state) is also stable
- Therefore: **no dependencies needed**

### Phase 2: Add Retry Logic with Exponential Backoff

**Goal:** Gracefully handle rate limits and transient failures

**File to Modify:** `frontend/src/services/api.ts`

**Implementation:** Add `fetchWithRetry` function with exponential backoff, jitter, and Retry-After header parsing (see section 3 above for full code).

### Phase 3: Improve Backend Rate Limiter Scope

**Goal:** Apply rate limiting only to API routes, not static assets

**File to Modify:** `src/main.rs`

**Change:** Move `.wrap(Governor::new(&governor_conf))` from global App level to only wrap the `/api` scope.

### Phase 4: Add Retry-After Header to Backend

**Goal:** Help clients retry at the right time

**Note:** actix-governor should already add this header automatically. Verify and document.

### Phase 5: Fix CSP Headers

**Goal:** Allow external resources without violations

**File:** `src/main.rs` (lines 128-137)

**Change:** Add Font Awesome CDN domains to CSP directives.

---

## Implementation Steps

### Step 1: Fix Frontend Hook Dependencies (DO THIS FIRST)
**Priority:** 🔥 CRITICAL - This is the root cause
**Files:**
1. `frontend/src/context/AppContext.tsx`
2. `frontend/src/pages/InventoriesPage.tsx`
3. `frontend/src/pages/InventoryDetailPage.tsx`
4. `frontend/src/pages/OrganizersPage.tsx`

**Actions:**
1. Wrap `showToast`, `removeToast`, `toggleTheme` in `useCallback` in AppContext
2. Remove dependencies from `loadInventories` callbacks (use empty array `[]`)
3. Test: Application should no longer flood API with requests

**Testing:**
```bash
# Open browser dev tools → Network tab
# Load the page
# Verify: Should see ~5-10 requests on load, NOT hundreds
# Check: No 429 errors in console
```

### Step 2: Add Retry Logic
**Priority:** ⚠️ HIGH - Improves resilience
**File:** `frontend/src/services/api.ts`

**Actions:**
1. Add retry utility functions (`fetchWithRetry`, `calculateBackoff`, `addJitter`, `sleep`)
2. Update all API methods to use `fetchWithRetry` instead of `fetch`
3. Test with intentionally low rate limits

### Step 3: Scope Rate Limiter to API Routes
**Priority:** ⚠️ MEDIUM - Performance improvement
**Files:**
1. `src/main.rs`
2. `src/api/mod.rs`

**Actions:**
1. Move Governor middleware from global to `/api` scope
2. Update `api::init_routes()` to use `ServiceConfig`
3. Rebuild backend: `cargo build`

### Step 4: Verify Retry-After Header
**Priority:** ℹ️ LOW - May already work
**Actions:**
1. Trigger rate limit
2. Check response headers for `Retry-After`
3. If missing, implement custom error handler

### Step 5: Fix CSP Headers
**Priority:** ℹ️ LOW - Cosmetic fix
**Actions:**
1. Add Font Awesome CDN domains to CSP
2. Rebuild: `cargo build`
3. Test: No CSP violations in browser console

---

## Success Criteria

### Must Have (Phase 1)
- ✅ No infinite request loops
- ✅ Page load triggers ≤10 API requests
- ✅ No 429 errors during normal usage
- ✅ Application is usable

### Should Have (Phase 2-3)
- ✅ Retry logic handles transient 429s gracefully
- ✅ Static assets not rate limited
- ✅ Health checks not rate limited
- ✅ API routes properly rate limited

### Nice to Have (Phase 4-5)
- ✅ Retry-After header present in 429 responses
- ✅ No CSP violations in console
- ✅ Clean console output

---

## References

### Documentation Sources
1. **Governor Library** - GCRA rate limiting algorithm (Context7: `/boinkor-net/governor`)
2. **Actix Extras** - actix-governor middleware (Context7: `/actix/actix-extras`)
3. **MDN Web Docs** - HTTP 429 Status Code
4. **React Documentation** - useCallback and useEffect
5. **Actix-Web Documentation** - Middleware and Routing
6. **Content Security Policy** - MDN CSP Guide

---

**End of Specification**
