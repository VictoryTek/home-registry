# CSP Violations & Rate Limiting Fix Specification

**Date**: February 12, 2026  
**Document Version**: 1.0  
**Status**: Ready for Implementation

---

## Executive Summary

Home Registry is experiencing two critical issues affecting user experience:

1. **Content Security Policy (CSP) Violations**: External stylesheets from Google Fonts and cdnjs.cloudflare.com are blocked by overly restrictive CSP headers, preventing proper font rendering and icon display.

2. **Rate Limiting (429 Errors)**: Frontend code is making excessive sequential API calls, triggering hundreds of 429 "Too Many Requests" errors and degrading application performance.

This specification provides a comprehensive analysis and solution for both issues.

---

## 1. Current State Analysis

### 1.1 CSP Configuration

**Location**: `src/main.rs` (lines 128-133)

**Current CSP Header**:
```rust
.add(("Content-Security-Policy", 
      "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'"))
```

**Problematic Directives**:
- `style-src 'self' 'unsafe-inline'` - Blocks external stylesheets
- `font-src 'self'` - Blocks external font files

**Blocked Resources** (from `frontend/index.html` lines 19-20):
```html
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&display=swap" rel="stylesheet">
<link href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css" rel="stylesheet">
```

**Impact**:
- Google Fonts (Inter typeface) cannot load → fallback to system fonts
- Font Awesome icons cannot load → missing icons throughout UI
- Browser console shows CSP violation errors
- Degraded visual appearance and user experience

### 1.2 Rate Limiting Configuration

**Location**: `src/main.rs` (lines 73-91)

**Current Settings**:
```rust
let requests_per_second = env::var("RATE_LIMIT_RPS")
    .unwrap_or_else(|_| "50".to_string())
    .parse::<u64>()
    .unwrap_or(50);

let burst_size = env::var("RATE_LIMIT_BURST")
    .unwrap_or_else(|_| "100".to_string())
    .parse::<u32>()
    .unwrap_or(100);
```

**Default Values**:
- RPS (Requests Per Second): 50
- Burst Capacity: 100

**Environment Variables** (docker-compose.yml lines 30-31):
```yaml
# RATE_LIMIT_RPS: 50  # Max requests per second per IP (default: 50)
# RATE_LIMIT_BURST: 100  # Initial burst capacity (default: 100)
```
*Note: Currently commented out, using hardcoded defaults*

### 1.3 Frontend API Call Patterns

#### Problem Area 1: InventoriesPage.tsx (lines 30-64)

**Sequential API Calls in Loop**:
```tsx
const loadInventories = useCallback(async () => {
  setLoading(true);
  try {
    const result = await inventoryApi.getAll(); // Call #1
    if (result.success && result.data) {
      setInventories(result.data);
      
      // Load item counts and all items for notification checking
      const counts: Record<number, number> = {};
      const allItems: Item[] = [];
      
      for (const inv of result.data) {  // ⚠️ PROBLEM: Sequential loop
        if (inv.id) {
          const itemsResult = await inventoryApi.getItems(inv.id); // Calls #2-N
          if (itemsResult.success && itemsResult.data) {
            counts[inv.id] = itemsResult.data.length;
            allItems.push(...itemsResult.data);
          }
        }
      }
      
      setItemCounts(counts);
      setItems(allItems); // Update global items state for notifications
    }
  } finally {
    setLoading(false);
  }
}, [showToast, setItems, setInventories]);
```

**Issue Analysis**:
- **N+1 Query Problem**: 1 call to get inventories + N calls to get items for each inventory
- **Example**: With 10 inventories → 11 sequential API calls
- **Sequential Execution**: No parallelization, each call waits for the previous
- **Dependency Issues**: `useCallback` depends on context functions that may trigger re-renders
- **Triggering Effect** (line 65-67):
  ```tsx
  useEffect(() => {
    void loadInventories();
  }, [loadInventories]);
  ```

#### Problem Area 2: Multiple useEffect Triggers

**Auto-Navigation Effect** (lines 70-77):
```tsx
useEffect(() => {
  if (!loading && !hasAutoNavigated.current && settings?.default_inventory_id && inventories.length > 0) {
    // Check if the default inventory exists
    const defaultInventory = inventories.find(inv => inv.id === settings.default_inventory_id);
    if (defaultInventory) {
      hasAutoNavigated.current = true;
      navigate(`/inventory/${settings.default_inventory_id}`);
    }
  }
}, [loading, settings, inventories, navigate]);
```
- Dependencies on `loading`, `settings`, `inventories`, `navigate`
- Can trigger multiple times as state changes

#### Problem Area 3: AppContext.tsx (lines 63-66)

**Notification Checking**:
```tsx
// Auto-check notifications when items change
useEffect(() => {
  checkNotifications();
}, [checkNotifications]);
```
- Runs every time `checkNotifications` changes
- `checkNotifications` depends on `items` array (line 57)
- Can trigger re-renders in child components

### 1.4 Root Cause Summary

**Rate Limiting Cascade**:
1. User navigates to InventoriesPage
2. `loadInventories` makes 11+ sequential API calls
3. All calls hit rate limiter within ~1 second
4. With 50 RPS limit, sustained load quickly exhausts burst capacity
5. Context updates trigger re-renders in some cases
6. Page refreshes or navigation repeats the pattern
7. Result: Hundreds of 429 errors flood console

**CSP Restriction**:
- Backend CSP policy explicitly blocks external style and font sources
- Frontend HTML hardcodes Google Fonts and Font Awesome CDN links
- Browser enforces CSP → blocks resource loading → degraded UI

---

## 2. Research Findings & Best Practices

### 2.1 CSP Best Practices

**Source 1: MDN Web Docs - Content Security Policy**
- **Recommendation**: Use specific domain whitelisting for trusted CDNs
- **Quote**: "Rather than blocking all external resources, whitelist trusted domains"
- **Best Practice**: Separate `style-src` and `font-src` directives
- **Security**: Modern CSP recommends moving away from `'unsafe-inline'` but it's acceptable for legacy compatibility

**Source 2: Google Fonts CSP Guidelines**
- **Required Domains**:
  - `fonts.googleapis.com` - For stylesheet loading (style-src)
  - `fonts.gstatic.com` - For actual font file delivery (font-src)
- **Note**: Both domains required for proper functionality
- **Security**: Google CDN has high reputation and HTTPS enforcement

**Source 3: OWASP CSP Cheat Sheet**
- **Principle**: Start restrictive, whitelist as needed
- **Anti-Pattern**: Using `*` or overly broad wildcards
- **Recommendation**: Specific domain listing for each directive
- **Font Handling**: `data:` URI support for embedded fonts

**Source 4: cdnjs.cloudflare.com Security**
- **Reputation**: Widely trusted CDN operated by Cloudflare
- **SRI (Subresource Integrity)**: Should be used for integrity verification
- **Best Practice**: Include specific version URLs rather than "latest"
- **Alternative**: Consider self-hosting Font Awesome for tighter security

**Source 5: CSP Level 3 Specification**
- **Modern Directive**: `'strict-dynamic'` for script sources (future consideration)
- **Font Sources**: Support for `data:` and specific HTTPS origins
- **Reporting**: Can add `report-uri` for CSP violation monitoring

**Source 6: Actix-Web Security Headers (from Context7 docs)**
- **Middleware**: Use `DefaultHeaders` middleware for security headers
- **Pattern**: Add headers at application level, not per-route
- **Best Practice**: Combine with other security headers (X-Frame-Options, etc.)

**Recommended CSP Policy**:
```
default-src 'self'; 
script-src 'self' 'unsafe-inline' 'unsafe-eval'; 
style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdnjs.cloudflare.com; 
img-src 'self' data: https:; 
font-src 'self' https://fonts.gstatic.com data:; 
connect-src 'self'; 
frame-ancestors 'none'
```

### 2.2 Rate Limiting Best Practices

**Source 1: Actix-Governor Documentation**
- **Current Implementation**: Per-IP rate limiting using token bucket algorithm
- **Token Bucket**: Allows burst traffic while maintaining average rate
- **Best Practice**: RPS should be 3-5x typical usage for smooth UX
- **Burst Size**: Should cover initial page load requirements

**Source 2: API Rate Limiting for CRUD Applications**
- **Typical Usage**: Single-user home inventory app makes 10-30 requests on page load
- **Recommendation**: 
  - Development: 100 RPS, burst 200
  - Production: 50 RPS, burst 150
- **Home Application Context**: Single household (1-5 concurrent users) vs. public API
- **Consideration**: Rate limiting per IP may be too strict for home networks (all family members share IP)

**Source 3: Frontend Caching Strategies**
- **Problem**: N+1 queries are an anti-pattern
- **Solutions**:
  1. **Backend Endpoint**: Create `/api/inventories/with-items` that returns inventories with item counts in single call
  2. **Parallel Fetching**: Use `Promise.all()` to parallelize item fetches
  3. **Frontend Cache**: Implement TanStack Query for automatic caching and deduplication
  4. **Stale-While-Revalidate**: Show cached data immediately, refresh in background

**Source 4: TanStack Query (from Context7 docs - /tanstack/query v5.84.1)**
- **Purpose**: Powerful async state management for data fetching
- **Features**:
  - Automatic caching with configurable TTL
  - Background refetching
  - Request deduplication
  - Polling and real-time updates
  - Optimistic updates
- **React Integration**: Hooks like `useQuery`, `useMutation`, `useInfiniteQuery`
- **Cache Strategy**: `staleTime` and `cacheTime` control freshness
- **Example**:
  ```tsx
  const { data, isLoading } = useQuery({
    queryKey: ['inventories'],
    queryFn: () => inventoryApi.getAll(),
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
  ```

**Source 5: React useEffect Dependencies**
- **Problem**: Callback dependencies causing re-renders
- **Solution**: Stabilize dependencies with `useCallback` and careful dependency arrays
- **Best Practice**: Avoid including entire context objects in dependencies
- **Pattern**: Use ref-based flags to prevent duplicate calls

**Source 6: Backend Optimization Patterns**
- **Aggregation Endpoint**: Single endpoint returning joined data
- **GraphQL Alternative**: Allow clients to request exactly what they need
- **Pagination**: Implement cursor or offset pagination for large datasets
- **Field Selection**: Allow clients to specify which fields to return

---

## 3. Proposed Solutions

### 3.1 Solution 1: Fix CSP Headers (CRITICAL - Low Risk)

**Priority**: P0 (Immediate)  
**Estimated Effort**: 15 minutes  
**Risk**: Very Low  
**Dependencies**: None

**Implementation**:
Update CSP header in `src/main.rs` (line 130-132) to whitelist trusted CDN domains:

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

**Changes Made**:
- `style-src`: Added `https://fonts.googleapis.com` and `https://cdnjs.cloudflare.com`
- `font-src`: Added `https://fonts.gstatic.com` and kept `data:`
- Formatted as multi-line string for readability

**Validation**:
1. Build and restart application
2. Open browser DevTools → Console
3. Verify no CSP violation errors
4. Check that Inter font loads correctly
5. Verify Font Awesome icons display properly

**Security Considerations**:
- ✅ Minimal security impact - whitelisting widely-trusted CDNs
- ✅ Google Fonts and Cloudflare CDN have strong security track records
- ✅ Still blocks unknown external resources
- ✅ Maintains `frame-ancestors 'none'` for clickjacking protection
- ⚠️ Future Enhancement: Consider self-hosting fonts for complete control

### 3.2 Solution 2: Increase Rate Limits (IMMEDIATE - Low Risk)

**Priority**: P0 (Immediate)  
**Estimated Effort**: 10 minutes  
**Risk**: Very Low  
**Dependencies**: None

**Rationale**:
- Home inventory app is single-household use (not public API)
- Current limits too strict for typical page load patterns
- Increasing limits provides immediate relief while frontend optimization proceeds

**Implementation**:
Update `docker-compose.yml` (lines 30-31) to uncomment and adjust rate limits:

```yaml
environment:
  DATABASE_URL: postgres://postgres:password@db:5432/home_inventory
  PORT: 8210
  RUST_LOG: info
  # JWT_SECRET: "your-secure-secret-here"  # Uncomment and set for production
  # JWT_TOKEN_LIFETIME_HOURS: 24  # Token lifetime in hours (default: 24)
  RATE_LIMIT_RPS: 100  # Increased from 50 for home use
  RATE_LIMIT_BURST: 200  # Increased from 100 to handle page load spikes
```

**Recommended Values**:
- **RPS**: 100 (doubled from 50)
  - Allows 100 requests per second sustained
  - More than sufficient for household use (1-5 concurrent users)
  - Still protects against accidental infinite loops or DoS
- **BURST**: 200 (doubled from 100)
  - Accommodates initial page load with 20+ requests
  - Provides buffer for rapid user interactions
  - Refills at 100 requests/second

**Alternative Configuration for Development**:
```yaml
RATE_LIMIT_RPS: 500  # Very permissive for development
RATE_LIMIT_BURST: 1000
```

**Validation**:
1. Stop docker-compose: `docker-compose down`
2. Update docker-compose.yml with new values
3. Start services: `docker-compose up -d`
4. Navigate to InventoriesPage and observe console
5. Verify no 429 errors on page load
6. Backend logs should show new rate limit values on startup

**Trade-offs**:
- ✅ Immediate fix with no code changes
- ✅ Appropriate for home/household use case
- ✅ Still provides DoS protection
- ⚠️ Does not address root cause (inefficient API calls)
- ⚠️ Should be combined with Solution 3 for optimal results

### 3.3 Solution 3: Parallelize API Calls (HIGH PRIORITY - Medium Risk)

**Priority**: P1 (High - should implement soon)  
**Estimated Effort**: 1-2 hours  
**Risk**: Medium (requires careful testing)  
**Dependencies**: None (uses existing patterns)

**Rationale**:
- Sequential API calls (N+1 pattern) are inefficient
- Parallelization reduces total load time and rate limit pressure
- No new dependencies required

**Implementation**:
Update `frontend/src/pages/InventoriesPage.tsx` `loadInventories` function (lines 30-64):

**Current Code**:
```tsx
for (const inv of result.data) {
  if (inv.id) {
    const itemsResult = await inventoryApi.getItems(inv.id);
    if (itemsResult.success && itemsResult.data) {
      counts[inv.id] = itemsResult.data.length;
      allItems.push(...itemsResult.data);
    } else {
      counts[inv.id] = 0;
    }
  }
}
```

**Improved Code**:
```tsx
// Load all items in parallel instead of sequentially
const itemsPromises = result.data.map(inv => 
  inv.id ? inventoryApi.getItems(inv.id) : Promise.resolve({ success: false, data: null })
);

const itemsResults = await Promise.all(itemsPromises);

itemsResults.forEach((itemsResult, index) => {
  const inv = result.data[index];
  if (inv.id) {
    if (itemsResult.success && itemsResult.data) {
      counts[inv.id] = itemsResult.data.length;
      allItems.push(...itemsResult.data);
    } else {
      counts[inv.id] = 0;
    }
  }
});
```

**Benefits**:
- ✅ All item fetches happen simultaneously
- ✅ Reduces total load time from ~2-3s to ~300-500ms (assuming 10 inventories)
- ✅ Still makes same number of requests, but faster
- ✅ Better user experience with faster loading
- ✅ Reduces rate limit pressure by spreading requests over shorter time

**Testing Requirements**:
1. Test with 0 inventories (empty state)
2. Test with 1 inventory
3. Test with 10+ inventories
4. Test error handling when one inventory fails
5. Verify item counts display correctly
6. Verify warranty notifications still work

**Risk Mitigation**:
- Error in one inventory fetch doesn't crash entire page
- Preserves existing error handling patterns
- Backward compatible with existing API

### 3.4 Solution 4: Create Aggregated Backend Endpoint (RECOMMENDED - Medium Risk)

**Priority**: P1 (High - best long-term solution)  
**Estimated Effort**: 2-3 hours  
**Risk**: Medium (new endpoint, needs testing)  
**Dependencies**: Database schema knowledge

**Rationale**:
- Eliminates N+1 problem at the source
- Single database query with JOIN is more efficient than N queries
- Reduces network overhead and latency
- Reduces frontend complexity

**Implementation**:

#### Backend Changes

**Step 1**: Add database method to `src/db/mod.rs` (after existing inventory methods):

```rust
/// Get all inventories with item counts for the current user
pub async fn get_inventories_with_counts(&self, user_id: i32) -> Result<Vec<InventoryWithCount>, PoolError> {
    let conn = self.pool.get().await?;
    
    let rows = conn.query(
        "SELECT 
            i.id, i.name, i.description, i.location, i.image_url, i.created_at, i.updated_at, i.user_id,
            COALESCE(COUNT(DISTINCT it.id), 0)::INTEGER as item_count
         FROM inventories i
         LEFT JOIN items it ON it.inventory_id = i.id
         WHERE i.user_id = $1
         GROUP BY i.id, i.name, i.description, i.location, i.image_url, i.created_at, i.updated_at, i.user_id
         ORDER BY i.created_at DESC",
        &[&user_id],
    ).await?;
    
    let inventories: Vec<InventoryWithCount> = rows
        .iter()
        .map(|row| InventoryWithCount {
            id: row.get("id"),
            name: row.get("name"),
            description: row.get("description"),
            location: row.get("location"),
            image_url: row.get("image_url"),
            created_at: row.get::<_, Option<String>>("created_at"),
            updated_at: row.get::<_, Option<String>>("updated_at"),
            user_id: row.get("user_id"),
            item_count: row.get("item_count"),
        })
        .collect();
    
    Ok(inventories)
}
```

**Step 2**: Add model to `src/models/mod.rs`:

```rust
/// Inventory with item count
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InventoryWithCount {
    pub id: Option<i32>,
    pub name: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub image_url: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub user_id: i32,
    pub item_count: i32,
}
```

**Step 3**: Add API endpoint to `src/api/mod.rs`:

```rust
/// GET /api/inventories/with-counts - Get all inventories with item counts
#[get("/inventories/with-counts")]
async fn get_inventories_with_counts(
    db: web::Data<Pool>,
    user_data: web::ReqData<models::UserData>,
) -> Result<HttpResponse, Error> {
    let user_id = user_data.user_id;
    let db_service = DatabaseService::new(db.get_ref().clone());
    
    match db_service.get_inventories_with_counts(user_id).await {
        Ok(inventories) => Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(inventories),
            error: None,
        })),
        Err(e) => {
            log::error!("Failed to get inventories with counts for user {}: {}", user_id, e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: Some("Failed to fetch inventories with counts".to_string()),
            }))
        }
    }
}
```

**Step 4**: Register endpoint in `pub fn init_routes()` function:

```rust
pub fn init_routes() -> Scope {
    web::scope("/api")
        .wrap(auth::AuthMiddleware)
        // ... existing routes ...
        .service(get_inventories_with_counts)  // Add this line
        // ... rest of routes ...
}
```

#### Frontend Changes

**Step 1**: Add API method to `frontend/src/services/api.ts`:

```typescript
async getInventoriesWithCounts(): Promise<ApiResponse<InventoryWithCount[]>> {
  const response = await fetch(`${API_BASE}/inventories/with-counts`, {
    headers: getHeaders(),
  });
  return handleResponse<InventoryWithCount[]>(response);
},
```

**Step 2**: Add type to `frontend/src/types/index.ts`:

```typescript
export interface InventoryWithCount extends Inventory {
  item_count: number;
}
```

**Step 3**: Update `frontend/src/pages/InventoriesPage.tsx`:

```tsx
const loadInventories = useCallback(async () => {
  setLoading(true);
  try {
    // Single API call gets inventories with item counts
    const result = await inventoryApi.getInventoriesWithCounts();
    if (result.success && result.data) {
      setInventories(result.data);
      
      // Extract item counts from the response
      const counts: Record<number, number> = {};
      for (const inv of result.data) {
        if (inv.id) {
          counts[inv.id] = inv.item_count;
        }
      }
      setItemCounts(counts);
      
      // Still need to load items separately for warranty notifications
      // But this can be done in the background without blocking the UI
      loadAllItemsForNotifications(result.data);
    } else {
      showToast(result.error ?? 'Failed to load inventories', 'error');
    }
  } catch {
    showToast('Failed to load inventories', 'error');
  } finally {
    setLoading(false);
  }
}, [showToast, setInventories]);

// Background loading for warranty notifications (non-blocking)
const loadAllItemsForNotifications = async (inventories: InventoryWithCount[]) => {
  try {
    const itemsPromises = inventories
      .filter(inv => inv.id)
      .map(inv => inventoryApi.getItems(inv.id!));
    
    const itemsResults = await Promise.all(itemsPromises);
    const allItems: Item[] = [];
    
    itemsResults.forEach(result => {
      if (result.success && result.data) {
        allItems.push(...result.data);
      }
    });
    
    setItems(allItems); // Update global items state for notifications
  } catch (error) {
    console.error('Failed to load items for notifications:', error);
    // Don't show error to user - notifications are non-critical
  }
};
```

**Benefits**:
- ✅ Reduces API calls from N+1 to 1 for inventory display
- ✅ Warranty notifications still work (loaded in background)
- ✅ Much faster initial page load
- ✅ Database query is more efficient with JOIN
- ✅ Reduces network latency (1 round trip vs N+1)
- ✅ Sets pattern for future optimizations

**Testing Requirements**:
1. Verify inventories display with correct item counts
2. Test with 0, 1, and 10+ inventories
3. Verify warranty notifications still work
4. Test error handling
5. Verify pagination works (if implemented)
6. Load test with 50+ inventories
7. Check database query performance with EXPLAIN ANALYZE

### 3.5 Solution 5: Implement TanStack Query (OPTIMAL - High Effort)

**Priority**: P2 (Medium - future enhancement)  
**Estimated Effort**: 4-6 hours  
**Risk**: Medium-High (new dependency, refactoring required)  
**Dependencies**: `@tanstack/react-query` npm package

**Rationale**:
- Industry-standard solution for data fetching in React
- Automatic caching eliminates duplicate requests
- Background refetching keeps data fresh
- Built-in loading and error states
- Request deduplication
- Optimistic updates support

**Implementation Overview**:

**Step 1**: Install dependency:
```bash
cd frontend
npm install @tanstack/react-query@5.84.1
```

**Step 2**: Setup QueryClient in `frontend/src/main.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ReactQueryDevtools } from '@tanstack/react-query-devtools';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000, // Data considered fresh for 5 minutes
      cacheTime: 10 * 60 * 1000, // Cache retained for 10 minutes
      refetchOnWindowFocus: false, // Don't refetch when window regains focus
      retry: 1, // Retry failed requests once
    },
  },
});

root.render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
      <ReactQueryDevtools initialIsOpen={false} />
    </QueryClientProvider>
  </React.StrictMode>
);
```

**Step 3**: Create query hooks in `frontend/src/hooks/useInventories.ts`:

```tsx
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { inventoryApi } from '@/services/api';
import type { Inventory, CreateInventoryRequest, UpdateInventoryRequest } from '@/types';

export const INVENTORIES_QUERY_KEY = ['inventories'];

export function useInventories() {
  return useQuery({
    queryKey: INVENTORIES_QUERY_KEY,
    queryFn: async () => {
      const result = await inventoryApi.getAll();
      if (!result.success) {
        throw new Error(result.error ?? 'Failed to load inventories');
      }
      return result.data!;
    },
  });
}

export function useCreateInventory() {
  const queryClient = useQueryClient();
  
  return useMutation({
    mutationFn: (data: CreateInventoryRequest) => inventoryApi.create(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: INVENTORIES_QUERY_KEY });
    },
  });
}

export function useUpdateInventory() {
  const queryClient = useQueryClient();
  
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: UpdateInventoryRequest }) => 
      inventoryApi.update(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: INVENTORIES_QUERY_KEY });
    },
  });
}

export function useDeleteInventory() {
  const queryClient = useQueryClient();
  
  return useMutation({
    mutationFn: (id: number) => inventoryApi.delete(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: INVENTORIES_QUERY_KEY });
    },
  });
}
```

**Step 4**: Refactor `InventoriesPage.tsx` to use hooks:

```tsx
import { useInventories, useCreateInventory, useUpdateInventory, useDeleteInventory } from '@/hooks/useInventories';

export function InventoriesPage() {
  const navigate = useNavigate();
  const { showToast } = useApp();
  
  // Replace manual state management with TanStack Query
  const { data: inventories = [], isLoading, error } = useInventories();
  const createMutation = useCreateInventory();
  const updateMutation = useUpdateInventory();
  const deleteMutation = useDeleteInventory();
  
  // ... rest of component logic
  
  const handleCreateInventory = async () => {
    if (!formData.name.trim()) {
      showToast('Please enter an inventory name', 'error');
      return;
    }

    createMutation.mutate(formData, {
      onSuccess: () => {
        showToast('Inventory created successfully!', 'success');
        setShowCreateModal(false);
        resetForm();
        // No need to manually reload - TanStack Query handles it
      },
      onError: (error) => {
        showToast((error as Error).message || 'Failed to create inventory', 'error');
      },
    });
  };
  
  // Similar pattern for update and delete...
}
```

**Benefits**:
- ✅ Automatic caching - no duplicate requests
- ✅ Stale-while-revalidate pattern keeps UI responsive
- ✅ Request deduplication - parallel calls merged into one
- ✅ Background refetching keeps data fresh
- ✅ Optimistic updates improve perceived performance
- ✅ Built-in loading and error states
- ✅ DevTools for debugging query state
- ✅ Eliminates manual cache management
- ✅ Reduces boilerplate code

**Trade-offs**:
- ⚠️ New dependency (~50KB gzipped)
- ⚠️ Learning curve for team members unfamiliar with TanStack Query
- ⚠️ Requires refactoring existing components
- ⚠️ May need to adjust AppContext to work with query cache

**Migration Strategy**:
1. Start with read-only pages (InventoriesPage, InventoryDetailPage)
2. Gradually migrate write operations (create, update, delete)
3. Eventually remove manual state management from AppContext
4. Consider migrating other API calls (items, organizers, auth)

**Testing Requirements**:
1. Verify caching behavior (network tab shows fewer requests)
2. Test cache invalidation after mutations
3. Verify offline behavior
4. Test concurrent mutations
5. Verify DevTools work correctly
6. Performance testing with large datasets

---

## 4. Implementation Plan & Priority

### Phase 1: Immediate Fixes (Day 1 - 30 minutes)

**Objective**: Stop the bleeding - resolve CSP violations and rate limit errors immediately

1. ✅ **Solution 1**: Update CSP headers in `src/main.rs`
   - Estimated: 15 minutes
   - Risk: Very Low
   - Testing: Visual inspection, console check

2. ✅ **Solution 2**: Increase rate limits in `docker-compose.yml`
   - Estimated: 10 minutes
   - Risk: Very Low
   - Testing: Load page, verify no 429 errors

3. ✅ **Build & Deploy**:
   ```bash
   docker-compose down
   docker-compose build
   docker-compose up -d
   ```

4. ✅ ** Validation**:
   - Open application in browser
   - Check DevTools console for errors
   - Verify fonts and icons load correctly
   - Navigate through multiple inventories
   - Confirm no 429 errors in console

**Success Criteria**:
- ✅ No CSP violation errors in console
- ✅ Google Fonts (Inter) loads correctly
- ✅ Font Awesome icons display properly
- ✅ No 429 rate limit errors during normal usage
- ✅ Page loads complete successfully

### Phase 2: Frontend Optimization (Day 2 - 2-3 hours)

**Objective**: Improve frontend efficiency with parallelization

1. ✅ **Solution 3**: Parallelize API calls in `InventoriesPage.tsx`
   - Estimated: 1-2 hours
   - Risk: Medium
   - Testing: Unit tests, integration tests, manual testing

2. ✅ **Testing**:
   - Test with 0, 1, 10, 20+ inventories
   - Verify item counts correct
   - Check warranty notifications work
   - Measure load time improvement
   - Monitor rate limit usage

3. ✅ **Commit & Deploy**:
   ```bash
   git add frontend/src/pages/InventoriesPage.tsx
   git commit -m "feat: parallelize inventory loading to improve performance"
   git push
   ```

**Success Criteria**:
- ✅ Page load time reduced by 50-70%
- ✅ Same functionality as before
- ✅ No regression in error handling
- ✅ Reduced rate limit pressure

### Phase 3: Backend Optimization (Week 1 - 3-4 hours)

**Objective**: Eliminate N+1 query pattern at the source

1. ✅ **Solution 4**: Implement aggregated backend endpoint
   - Database method: 1 hour
   - API endpoint: 1 hour
   - Frontend integration: 1 hour
   - Testing: 1 hour

2. ✅ **Testing**:
   - Database query performance (EXPLAIN ANALYZE)
   - API endpoint integration tests
   - Frontend rendering tests
   - Load testing with 50+ inventories
   - Verify warranty notifications still work

3. ✅ **Documentation**:
   - Update API.md with new endpoint
   - Add code comments explaining optimization

**Success Criteria**:
- ✅ Inventory page makes 1 API call instead of N+1
- ✅ Database query executes in <50ms
- ✅ Page load time further reduced
- ✅ All existing features work correctly

### Phase 4: Advanced Caching (Future - 4-6 hours)

**Objective**: Implement industry-standard caching solution

1. ⏳ **Solution 5**: Integrate TanStack Query
   - Setup: 1 hour
   - Create query hooks: 2 hours
   - Refactor components: 2 hours
   - Testing & debugging: 1 hour

2. ⏳ **Migration Strategy**:
   - Start with InventoriesPage
   - Move to InventoryDetailPage
   - Migrate other data-fetching components
   - Eventually remove manual cache from AppContext

3. ⏳ **Documentation**:
   - Document query key patterns
   - Add examples for team members
   - Update CONTRIBUTING.md with caching guidelines

**Success Criteria**:
- ✅ Zero duplicate API requests
- ✅ Instant navigation between cached pages
- ✅ Stale-while-revalidate keeps data fresh
- ✅ DevTools show clean query state
- ✅ Reduced bundle size vs manual cache

---

## 5. Risk Assessment & Mitigation

### 5.1 Security Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **CSP Whitelist Exploited** | Very Low | Medium | Google Fonts and Cloudflare CDN are highly trusted; both enforce HTTPS; consider SRI hashes for Font Awesome |
| **Increased Rate Limits Allow DoS** | Low | Low | New limits still protect against DoS (100 RPS is still restrictive); home network context reduces risk |
| **SQL Injection in New Endpoint** | Very Low | High | Using parameterized queries ($1 syntax); PostgreSQL prevents SQL injection with prepared statements |

**Recommended Additional Security**:
- Add Subresource Integrity (SRI) hashes to Font Awesome link in `frontend/index.html`:
  ```html
  <link href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css" 
        rel="stylesheet" 
        integrity="sha512-iecdLmaskl7CVkqkXNQ/ZH/XLlvWZOJyj7Yy7tcenmpD1ypASozpmT/E0iPtmFIB46ZmdtAc9eNBvH0H/ZpiBw==" 
        crossorigin="anonymous" 
        referrerpolicy="no-referrer">
  ```

### 5.2 Performance Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **Database JOIN Performance Degradation** | Low | Medium | Add test with 100+ inventories; monitor query execution time; add index on items.inventory_id if needed |
| **TanStack Query Memory Usage** | Very Low | Low | Query cache auto-evicts stale data; configure `cacheTime` appropriately |
| **Increased Rate Limits Consume Resources** | Very Low | Very Low | Home use case has low traffic; server can handle 100 RPS easily |

**Monitoring Recommendations**:
- Add logging for slow database queries (>100ms)
- Monitor memory usage with TanStack Query DevTools
- Set up error tracking (Sentry, etc.) for production

### 5.3 Compatibility Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **Browser CSP Support** | Very Low | Low | CSP Level 2 supported by all modern browsers (>95% coverage) |
| **TanStack Query React Version Conflict** | Low | Low | TanStack Query v5 supports React 18+ (currently using React 18) |
| **Breaking Changes to Existing Code** | Medium | Medium | Thorough testing; staged rollout; feature flags if needed |

### 5.4 User Experience Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **Flash of Loading State During Refetch** | Low | Low | TanStack Query shows cached data while refetching; staleTime prevents excessive refetching |
| **Warranty Notifications Delayed** | Low | Low | Load items in background after initial render; notifications appear within 1-2 seconds |
| **Unexpected Behavior with Cache** | Medium | Medium | Clear documentation; user-facing cache clear button in settings |

---

## 6. Testing & Validation Plan

### 6.1 CSP Fix Validation

**Manual Testing**:
1. ✅ Open browser DevTools → Console
2. ✅ Navigate to any page in the application
3. ✅ Verify no "Refused to load..." CSP violation errors
4. ✅ Check Network tab: fonts.googleapis.com stylesheet loads (200 OK)
5. ✅ Check Network tab: fonts.gstatic.com font files load (200 OK)
6. ✅ Check Network tab: cdnjs.cloudflare.com Font Awesome loads (200 OK)
7. ✅ Visual inspection: Inter font renders correctly (not system fallback)
8. ✅ Visual inspection: Font Awesome icons appear (not missing/broken)

**Automated Testing**:
```bash
# Test CSP header present
curl -I http://localhost:8210/ | grep -i content-security-policy

# Expected output includes:
# content-security-policy: default-src 'self'; ... style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdnjs.cloudflare.com; ...
```

### 6.2 Rate Limit Fix Validation

**Manual Testing**:
1. ✅ Clear browser cache (Ctrl+Shift+Delete)
2. ✅ Navigate to Inventories page
3. ✅ Open DevTools → Console
4. ✅ Verify no 429 errors appear
5. ✅ Check Network tab: all API requests return 200 OK
6. ✅ Rapidly navigate between pages and refresh
7. ✅ Verify no rate limit warnings

**Automated Testing**:
```bash
# Check backend logs for rate limit configuration
docker-compose logs app | grep -i "rate limiting"

# Expected output:
# Rate limiting: 100 requests/second, burst size: 200
```

**Load Testing** (optional):
```bash
# Install Apache Bench
# Test sustained load
ab -n 1000 -c 10 http://localhost:8210/api/inventories

# Verify:
# - No 429 responses
# - Average response time <100ms
# - All requests succeed
```

### 6.3 Frontend Optimization Validation

**Performance Testing**:
1. ✅ Open DevTools → Network tab
2. ✅ Hard refresh (Ctrl+Shift+R)
3. ✅ Count API requests to `/api/inventories/*/items`
   - **Before**: N sequential requests
   - **After Solution 3**: N parallel requests (faster)
   - **After Solution 4**: 0 requests (using `/api/inventories/with-counts`)
4. ✅ Measure page load time:
   - **Before**: ~2-3 seconds
   - **After Solution 3**: ~500-800ms
   - **After Solution 4**: ~200-400ms

**Functional Testing**:
1. ✅ Create new inventory → verify count updates
2. ✅ Add item to inventory → verify count increments
3. ✅ Delete item → verify count decrements
4. ✅ Delete inventory → verify it disappears
5. ✅ Check warranty notifications banner still appears
6. ✅ Test with 0, 1, 10, 20+ inventories

### 6.4 Backend Endpoint Validation

**Database Performance**:
```sql
-- Run in PostgreSQL to analyze query performance
EXPLAIN ANALYZE
SELECT 
    i.id, i.name, i.description, i.location, i.image_url, i.created_at, i.updated_at, i.user_id,
    COALESCE(COUNT(DISTINCT it.id), 0)::INTEGER as item_count
FROM inventories i
LEFT JOIN items it ON it.inventory_id = i.id
WHERE i.user_id = 1
GROUP BY i.id, i.name, i.description, i.location, i.image_url, i.created_at, i.updated_at, i.user_id
ORDER BY i.created_at DESC;

-- Expected:
-- Execution time: <50ms
-- Uses index on items.inventory_id (if exists)
```

**API Testing**:
```bash
# Test new endpoint
curl -H "Authorization: Bearer <token>" http://localhost:8210/api/inventories/with-counts

# Expected response:
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "Kitchen",
      "description": "Kitchen items",
      "location": null,
      "image_url": null,
      "created_at": "2026-02-12T10:30:00Z",
      "updated_at": "2026-02-12T10:30:00Z",
      "user_id": 1,
      "item_count": 15
    }
  ],
  "error": null
}
```

### 6.5 TanStack Query Validation

**Cache Behavior**:
1. ✅ Navigate to Inventories page
2. ✅ Check Network tab: API request made
3. ✅ Navigate away and back
4. ✅ Check Network tab: No new request (served from cache)
5. ✅ Wait 5 minutes (staleTime)
6. ✅ Return to page: Background refetch occurs
7. ✅ Open React Query DevTools
8. ✅ Verify query status: "success", "stale", "fetching", etc.

**Multi-Tab Testing**:
1. ✅ Open application in two browser tabs
2. ✅ Create inventory in Tab 1
3. ✅ Verify Tab 2 eventually updates (background refetch)

---

## 7. Rollback Plan

### 7.1 If CSP Changes Cause Issues

**Symptoms**:
- New resources fail to load
- Different CSP violations appear

**Rollback Steps**:
1. Revert `src/main.rs` CSP header to original:
   ```rust
   .add(("Content-Security-Policy", 
         "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'"))
   ```
2. Rebuild: `docker-compose build`
3. Restart: `docker-compose up -d`
4. Font Awesome and Google Fonts will be blocked again, but application remains functional

### 7.2 If Rate Limit Changes Cause Performance Issues

**Symptoms**:
- Server CPU usage spikes
- Legitimate traffic blocked
- Memory issues

**Rollback Steps**:
1. Edit `docker-compose.yml` and revert to original:
   ```yaml
   RATE_LIMIT_RPS: 50
   RATE_LIMIT_BURST: 100
   ```
2. Restart: `docker-compose restart app`
3. 429 errors will return, but server performance stabilizes

### 7.3 If Frontend Changes Break Functionality

**Symptoms**:
- Inventories not loading
- Item counts incorrect
- Errors in console

**Rollback Steps**:
1. Revert `frontend/src/pages/InventoriesPage.tsx` to original version:
   ```bash
   git checkout HEAD~1 frontend/src/pages/InventoriesPage.tsx
   ```
2. Rebuild frontend: `cd frontend && npm run build`
3. Restart: `docker-compose restart app`
4. Sequential API calls resume, but functionality restored

### 7.4 If Backend Endpoint Has Issues

**Symptoms**:
- 500 errors on `/api/inventories/with-counts`
- Database query timeout
- Incorrect item counts

**Rollback Steps**:
1. Frontend can fallback to original approach (load items individually)
2. Comment out new endpoint in `src/api/mod.rs`
3. Rebuild: `cargo build`
4. Restart: `docker-compose restart app`
5. Frontend detects 404 and uses fallback behavior

### 7.5 If TanStack Query Causes Issues

**Symptoms**:
- Stale data displayed
- Cache inconsistencies
- Memory leaks

**Rollback Steps**:
1. Revert to pre-TanStack Query version:
   ```bash
   git checkout <last-good-commit> frontend/src/
   ```
2. Uninstall dependency:
   ```bash
   cd frontend && npm uninstall @tanstack/react-query
   ```
3. Rebuild and restart

---

## 8. Documentation Updates Required

### 8.1 Files to Update

1. **README.md** - Add section on rate limiting configuration
2. **API.md** - Document new `/api/inventories/with-counts` endpoint
3. **CONTRIBUTING.md** - Add guidelines for TanStack Query usage
4. **docker-compose.yml** - Update environment variable comments
5. **.env.example** - Add rate limit variable examples

### 8.2 Code Documentation

Add inline comments explaining:
- CSP policy reasoning in `src/main.rs`
- Rate limit configuration in `src/main.rs`
- Database query optimization in `src/db/mod.rs`
- TanStack Query patterns in frontend hooks

---

## 9. Future Enhancements

### 9.1 Self-Host External Resources

**Rationale**: Complete control over assets, no external dependencies

**Steps**:
1. Download Google Fonts (Inter) and Font Awesome
2. Place in `static/assets/fonts/`
3. Update `frontend/index.html` to use local paths
4. Update CSP to remove external domains
5. Configure caching headers for font assets

**Benefits**:
- ✅ No CSP whitelist needed
- ✅ Faster loading (no external DNS/TCP)
- ✅ Works offline
- ✅ No third-party tracking

**Trade-offs**:
- ⚠️ Manual updates required for Font Awesome
- ⚠️ Increases docker image size
- ⚠️ More storage required

### 9.2 Implement GraphQL

**Rationale**: Allow clients to request exactly what they need

**Benefits**:
- Eliminates over-fetching
- Single endpoint for all queries
- Strong typing
- Excellent developer experience

**Considerations**:
- Significant refactoring required
- Learning curve for team
- May be overkill for home inventory app

### 9.3 Implement Server-Sent Events (SSE)

**Rationale**: Real-time updates without polling

**Use Case**: Multiple family members editing inventory simultaneously

**Benefits**:
- Real-time sync across devices
- No polling overhead
- Simple implementation vs WebSockets

**Implementation**:
- Use actix-web SSE support
- Frontend listens for inventory/item updates
- Automatically refresh TanStack Query cache

### 9.4 Add Pagination

**Rationale**: Improve performance with 100+ inventories

**Implementation**:
- Backend: Add `LIMIT` and `OFFSET` to queries
- Frontend: Implement infinite scroll or page buttons
- TanStack Query: Use `useInfiniteQuery` hook

**Benefits**:
- Faster initial page load
- Reduced memory usage
- Better UX for large datasets

### 9.5 Implement Field-Level Permissions

**Rationale**: Fine-grained access control for shared inventories

**Example**:
- Share inventory with friend but hide purchase prices
- Allow viewing but not editing

**Implementation**:
- Add field visibility flags to sharing permissions
- Backend filters fields based on permissions
- Frontend conditionally renders fields

---

## 10. Conclusion & Recommendations

### 10.1 Summary

This specification addresses two critical issues affecting Home Registry:

1. **CSP Violations**: Resolved by whitelisting trusted CDN domains in CSP headers
2. **Rate Limiting**: Resolved through combination of:
   - Increased rate limits (immediate relief)
   - Parallelized API calls (frontend optimization)
   - Aggregated endpoint (backend optimization)
   - TanStack Query caching (future enhancement)

### 10.2 Recommended Implementation Order

**Immediate (Day 1)**: Phase 1 fixes
- Update CSP headers ✅ Critical
- Increase rate limits ✅ Critical

**Short-term (Week 1)**: Phase 2-3 optimizations
- Parallelize API calls ✅ High Priority
- Implement aggregated endpoint ✅ Recommended

**Long-term (Month 1)**: Phase 4 enhancements
- Integrate TanStack Query ⏳ Optional but valuable
- Self-host external resources ⏳ Nice-to-have

### 10.3 Success Metrics

**Immediate Goals**:
- ✅ Zero CSP violation errors
- ✅ Zero 429 rate limit errors
- ✅ Fonts and icons load correctly

**Performance Goals**:
- ✅ Page load time <500ms (from ~2-3s)
- ✅ API calls reduced from N+1 to 1 (or N parallel)
- ✅ Database queries <50ms

**User Experience Goals**:
- ✅ Instant page navigation (cached data)
- ✅ Smooth, responsive interface
- ✅ No error messages in console

### 10.4 Long-Term Vision

Transform Home Registry into a highly performant, scalable application by:
1. ✅ Eliminating N+1 query patterns across codebase
2. ✅ Implementing comprehensive caching strategy
3. ✅ Optimizing database queries with proper indexing
4. ✅ Adding real-time updates for multi-user scenarios
5. ✅ Improving offline capabilities with service workers

---

## Appendix A: Research Sources

1. **MDN Web Docs - Content Security Policy**
   - URL: https://developer.mozilla.org/en-US/docs/Web/HTTP/CSP
   - Key Takeaway: Specific domain whitelisting is preferred over broad wildcards

2. **Google Fonts CSP Requirements**
   - URL: https://developers.google.com/fonts/docs/getting_started#usage
   - Key Takeaway: Requires both fonts.googleapis.com (styles) and fonts.gstatic.com (fonts)

3. **OWASP CSP Cheat Sheet**
   - URL: https://cheatsheetseries.owasp.org/cheatsheets/Content_Security_Policy_Cheat_Sheet.html
   - Key Takeaway: Start restrictive, whitelist trusted sources, avoid 'unsafe-inline' when possible

4. **Actix-Web Documentation v4.11.0** (via Context7)
   - Source: /websites/rs_actix-web_4_11_0
   - Key Takeaway: DefaultHeaders middleware for adding security headers

5. **TanStack Query Documentation v5.84.1** (via Context7)
   - Source: /tanstack/query
   - Key Takeaway: Stale-while-revalidate caching reduces API calls significantly

6. **API Rate Limiting Best Practices**
   - General Industry Knowledge
   - Key Takeaway: Rate limits should be 3-5x typical usage for smooth UX

7. **PostgreSQL Query Optimization**
   - Official PostgreSQL Documentation
   - Key Takeaway: LEFT JOIN with GROUP BY is efficient for counts

8. **React Performance Patterns**
   - React Documentation & Community Best Practices
   - Key Takeaway: useCallback stability and parallel data fetching

---

## Appendix B: Configuration Reference

### B.1 Complete CSP Header (Recommended)

```
default-src 'self'; 
script-src 'self' 'unsafe-inline' 'unsafe-eval'; 
style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdnjs.cloudflare.com; 
img-src 'self' data: https:; 
font-src 'self' https://fonts.gstatic.com data:; 
connect-src 'self'; 
frame-ancestors 'none'
```

### B.2 Environment Variables Reference

```bash
# Database
DATABASE_URL=postgres://postgres:password@db:5432/home_inventory

# Server
PORT=8210
HOST=0.0.0.0
RUST_LOG=info
RUST_ENV=production

# Authentication
JWT_SECRET=<auto-generated-or-set>
JWT_TOKEN_LIFETIME_HOURS=24

# Rate Limiting
RATE_LIMIT_RPS=100         # Requests per second sustained
RATE_LIMIT_BURST=200       # Burst capacity (initial bucket size)
```

### B.3 TanStack Query Default Configuration

```typescript
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000,        // 5 minutes
      cacheTime: 10 * 60 * 1000,       // 10 minutes  
      refetchOnWindowFocus: false,
      refetchOnReconnect: true,
      retry: 1,
    },
    mutations: {
      retry: 0,
    },
  },
});
```

---

## Appendix C: Database Index Recommendations

```sql
-- Ensure efficient JOIN in aggregated endpoint
CREATE INDEX IF NOT EXISTS idx_items_inventory_id 
ON items(inventory_id);

-- Improve user-specific queries
CREATE INDEX IF NOT EXISTS idx_inventories_user_id 
ON inventories(user_id);

-- Optimize warranty notification queries
CREATE INDEX IF NOT EXISTS idx_items_warranty_expiry 
ON items(warranty_expiry) 
WHERE warranty_expiry IS NOT NULL;
```

---

**End of Specification Document**

_This document provides a comprehensive roadmap for resolving CSP violations and rate limiting issues in the Home Registry application. All solutions are implementable with existing technology stack and follow industry best practices._
