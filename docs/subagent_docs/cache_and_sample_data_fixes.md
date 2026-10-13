# Cache Control and Sample Data Fixes - Specification

**Date:** February 14, 2026  
**Status:** Research & Specification Complete  
**Implementation Status:** Pending

---

## Executive Summary

This document specifies solutions for two critical issues affecting the Home Registry application:
1. **Frontend assets requiring hard refresh after Docker rebuild** - Service worker and cache control issues
2. **Sample inventory data not visible to first admin user** - NULL user_id filtering in database queries

Both issues have straightforward solutions requiring targeted changes to cache headers and database migration patterns.

---

## Issue 1: Frontend Assets Not Loading Without Hard Refresh

### Problem Statement

After a fresh Docker rebuild (`docker compose down -v; docker compose build --no-cache; docker compose up -d`) and opening a new private browser window, the application fails to display icons and text correctly. Content only appears after performing a hard refresh (Ctrl+Shift+R), indicating aggressive client-side caching is preventing new assets from loading.

### Root Cause Analysis

**Primary Causes:**

1. **Missing Cache-Control Headers on Static Assets**
   - `src/main.rs` uses `actix_files::Files::new("/assets", "static/assets")` without explicit cache configuration
   - Actix-Web's default file serving may set permissive cache headers
   - No `Cache-Control` headers configured for versioned assets vs. index.html

2. **Service Worker Aggressive Precaching**
   - `static/sw.js` precaches all assets with revision hashes via Workbox
   - Service worker registered inline in `static/index.html`: `navigator.serviceWorker.register('/sw.js', { scope: '/' })`
   - Vite PWA plugin configured with `registerType: 'autoUpdate'` in `frontend/vite.config.ts`
   - Old service worker may continue serving stale cached assets after rebuild

3. **Dockerfile Build Process**
   - Frontend built in Stage 1: `RUN npm run build` in `/app/frontend`
   - Assets copied to backend in Stage 3: `COPY --from=frontend-builder /app/frontend/dist ./static`
   - New build generates new asset hashes (e.g., `index-Lx9RgoJs.js`, `index-Cg9wYj8j.css`)
   - Old service worker doesn't know about new asset hashes

**Secondary Factors:**

- No version query string or cache-busting mechanism on manifest/service worker files
- PWA `updateViaCache` not explicitly configured (defaults to 'imports')
- Index.html itself may be cached, preventing new service worker script from being detected

### Research: Best Practices

**Source 1: Actix-Web Static File Serving Documentation**
- URL: https://docs.rs/actix-files/latest/actix_files/
- **Finding:** `actix_files::Files` supports `disable_content_disposition()`, `use_etag()`, `use_last_modified()` methods
- **Recommendation:** Use `Files::new().use_etag(false).use_last_modified(false)` for index.html to prevent caching
- **Note:** For versioned assets (with content hashes), caching is desirable; for entry points, it's not

**Source 2: MDN Web Docs - HTTP Caching**
- URL: https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching
- **Finding:** Recommended cache control patterns:
  - `Cache-Control: no-cache, no-store, must-revalidate` for HTML entry points
  - `Cache-Control: public, max-age=31536000, immutable` for versioned assets (with content hashes)
  - `Cache-Control: no-cache` for service worker scripts (mandatory per spec)
- **Recommendation:** Apply different cache policies based on file type

**Source 3: Service Worker Cookbook - Handling Updates**
- URL: https://serviceworke.rs/
- **Finding:** Service worker updates require:
  - Browser checks for sw.js updates on page load
  - New service worker enters "waiting" state if byte-different from old
  - `skipWaiting()` and `clients.claim()` force immediate activation
  - Service worker script must have `Cache-Control: no-cache` to ensure updates are detected
- **Recommendation:** Add `SKIP_WAITING` message handler (already exists in sw.js)

**Source 4: Vite PWA Plugin Documentation**
- URL: https://vite-pwa-org.netlify.app/
- **Finding:** `registerType: 'autoUpdate'` automatically calls `skipWaiting()` when new SW detected
- **Current Status:** Already configured in `frontend/vite.config.ts`
- **Issue:** Server must serve service worker with `Cache-Control: no-cache` header
- **Recommendation:** Explicitly set cache headers for `/sw.js` route

**Source 5: Workbox Precaching Strategies**
- URL: https://developer.chrome.com/docs/workbox/modules/workbox-precaching/
- **Finding:** Precache manifest includes revision hashes for cache invalidation
- **Current Issue:** Old service worker with old precache manifest continues serving stale assets
- **Solution:** Ensure service worker updates properly (requires no-cache headers)

**Source 6: Google Developers - Service Worker Lifecycle**
- URL: https://developers.google.com/web/fundamentals/primers/service-workers/lifecycle
- **Finding:** Service worker update algorithm:
  1. Browser fetches `/sw.js` on navigation (respects Cache-Control)
  2. If byte-different, new SW installs in parallel
  3. New SW waits until old SW no longer controlling pages
  4. With `skipWaiting()`, new SW activates immediately
- **Critical:** Step 1 fails if `sw.js` is cached with long max-age
- **Recommendation:** Serve `sw.js` with `Cache-Control: no-cache, max-age=0`

**Source 7: Actix-Web Middleware for Headers**
- URL: https://docs.rs/actix-web/latest/actix_web/middleware/struct.DefaultHeaders.html
- **Finding:** `DefaultHeaders` middleware applies to all responses (already used in main.rs for security headers)
- **Limitation:** Cannot selectively apply per-route without custom middleware
- **Alternative:** Use route-specific `NamedFile::open_async()` with `.customize()` method to set headers
- **Recommendation:** Wrap static file routes with custom header middleware or use NamedFile customization

**Source 8: NamedFile Customization in Actix-Web**
- URL: https://docs.rs/actix-files/latest/actix_files/struct.NamedFile.html#method.customize
- **Finding:** `NamedFile::customize()` returns `actix_web::CustomizeResponder` for header modification
- **Implementation:**
  ```rust
  fs::NamedFile::open_async("static/index.html")
      .await?
      .customize()
      .insert_header(("Cache-Control", "no-cache, no-store, must-revalidate"))
  ```
- **Recommendation:** Use this pattern for index.html, sw.js, and manifest routes

### Proposed Solution Architecture

**Multi-Layered Caching Strategy:**

| File Type | Cache Policy | Rationale |
|-----------|--------------|-----------|
| `index.html` | `no-cache, no-store, must-revalidate` | Entry point must always fetch latest to detect updates |
| `sw.js` | `no-cache, max-age=0` | Required for service worker update mechanism |
| `manifest.webmanifest` | `no-cache, max-age=0` | May reference service worker, should update promptly |
| `/assets/*` (with hash) | `public, max-age=31536000, immutable` | Content-hashed, safe to cache indefinitely |
| Logo files | `public, max-age=86400` | Static but not versioned, 24-hour cache |

**Implementation Plan:**

1. **Modify Static File Route Handlers in `src/main.rs`:**
   - Wrap individual file routes (`/`, `/sw.js`, `/manifest.json`) with `.customize().insert_header()`
   - Keep `/assets` route unchanged (benefits from long-term caching)
   - Add explicit cache headers to logo routes

2. **Keep Vite PWA Configuration:**
   - No changes needed to `frontend/vite.config.ts`
   - `registerType: 'autoUpdate'` already handles skipWaiting

3. **Service Worker Inline Registration:**
   - Current inline registration in index.html is acceptable
   - Browser will check for updates on navigation

### Dependencies and Requirements

**Existing Dependencies (No Changes Required):**
- `actix-files = "0.6"` (already in Cargo.toml)
- `vite-plugin-pwa` (already in frontend/package.json)

**Configuration Changes:**
- Modify route handlers in `src/main.rs`
- No new dependencies required

### Potential Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking existing caching behavior | Users experience slower loads initially | Acceptable tradeoff for correctness; hashed assets still cached |
| Service worker update delays | Users may see old version briefly | `autoUpdate` config + skipWaiting minimizes delay |
| Browser compatibility | Older browsers may ignore Cache-Control | All modern browsers (Chrome 40+, Firefox 39+, Safari 10+) support properly |
| Increased server load | More requests for index.html | Minimal impact; index.html is tiny (<10KB) |

---

## Issue 2: Sample Data Not Available to Admin User

### Problem Statement

After creating an admin account via the first-time setup wizard (`/auth/setup`), sample inventories (IDs 100-104) created by migration `019_add_sample_inventory_data.sql` are not visible in the application. These inventories were intentionally created with `user_id = NULL` to avoid conflicts before users exist.

### Root Cause Analysis

**Database Query Filter:**

The `get_accessible_inventories()` function in `src/db/mod.rs` (line 1848) uses this query:

```sql
SELECT DISTINCT i.id, i.name, i.description, i.location, i.image_url, i.user_id, i.created_at, i.updated_at 
FROM inventories i
LEFT JOIN inventory_shares s ON i.id = s.inventory_id AND s.shared_with_user_id = $1
LEFT JOIN user_access_grants g ON i.user_id = g.grantor_user_id AND g.grantee_user_id = $1
WHERE i.user_id = $1 
   OR s.shared_with_user_id = $1
   OR g.grantee_user_id = $1
ORDER BY i.name ASC
```

**Analysis:**
- Condition 1: `i.user_id = $1` → NULL ≠ user_id (fails)
- Condition 2: `s.shared_with_user_id = $1` → No shares exist for sample data (fails)
- Condition 3: `g.grantee_user_id = $1` → No access grants exist (fails)
- **Result:** NULL user_id inventories are invisible to all users

**Migration Intent:**

From `migrations/019_add_sample_inventory_data.sql`:

```sql
-- IMPORTANT: After creating your user account, run this command to assign the sample inventories to your user:
--   UPDATE inventories SET user_id = (SELECT id FROM users WHERE username = 'YOUR_USERNAME') WHERE user_id IS NULL;
--
-- Or assign to the first admin user:
--   UPDATE inventories SET user_id = (SELECT id FROM users WHERE is_admin = true ORDER BY created_at LIMIT 1) WHERE user_id IS NULL;
```

The migration includes manual SQL commands but no automated mechanism. The first-time setup endpoint (`POST /auth/setup`) doesn't execute these UPDATE statements.

### Research: Best Practices

**Source 1: PostgreSQL Row-Level Security Documentation**
- URL: https://www.postgresql.org/docs/current/ddl-rowsecurity.html
- **Finding:** NULL as a special value in filters requires explicit handling
- **SQL Standard:** `NULL = NULL` evaluates to NULL (falsy), not TRUE
- **Recommendation:** Use `IS NULL` checks or COALESCE for nullable ownership columns

**Source 2: Database Migration Patterns (Flyway/Liquibase Best Practices)**
- URL: https://flywaydb.org/documentation/concepts/migrations
- **Finding:** Migration patterns for "seed data before users exist":
  - Option A: Create temporary sentinel user, assign data, delete on first real user
  - Option B: Use NULL with post-creation trigger to auto-assign
  - Option C: Create data in same transaction as first user (breaks migration ordering)
  - Option D: Add application logic to detect and claim unassigned data
- **Recommendation:** Option D (application-level assignment) is safest for this use case

**Source 3: PostgreSQL Triggers for Auto-Assignment**
- URL: https://www.postgresql.org/docs/current/sql-createtrigger.html
- **Finding:** Triggers can detect "first user created" and auto-update related data
- **Implementation:**
  ```sql
  CREATE TRIGGER assign_sample_data_on_first_user
  AFTER INSERT ON users
  FOR EACH ROW
  WHEN (NEW.is_admin = true AND (SELECT COUNT(*) FROM users) = 1)
  EXECUTE FUNCTION assign_sample_inventories();
  ```
- **Pros:** Automated, works across any setup path
- **Cons:** Complex, requires function definition, hard to test/debug
- **Recommendation:** Use for production environments with strict audit requirements

**Source 4: Application-Level Assignment Patterns**
- URL: https://martinfowler.com/articles/evodb.html (Evolutionary Database Design)
- **Finding:** Application logic can handle special cases during setup flows
- **Pattern:** "Setup Wizard Post-Creation Hook"
  - After user creation succeeds in `POST /auth/setup`
  - Execute: `UPDATE inventories SET user_id = $new_user_id WHERE user_id IS NULL`
  - Idempotent (running twice is safe)
  - Easy to test and maintain
- **Recommendation:** Implement in `src/api/auth.rs::initial_setup()` function

**Source 5: Actix-Web Transaction Patterns with Tokio-Postgres**
- URL: https://docs.rs/tokio-postgres/latest/tokio_postgres/struct.Client.html#method.transaction
- **Finding:** Transactions ensure atomicity of user creation + data assignment
- **Implementation:**
  ```rust
  let mut client = pool.get().await?;
  let transaction = client.transaction().await?;
  
  // Create user
  let user = transaction.query_one("INSERT INTO users...", &[...]).await?;
  
  // Assign sample data
  transaction.execute("UPDATE inventories SET user_id = $1 WHERE user_id IS NULL", &[&user.id]).await?;
  
  transaction.commit().await?;
  ```
- **Pros:** Atomic, no partial state
- **Cons:** Requires refactoring DatabaseService methods to accept Transaction instead of Client
- **Recommendation:** Use if refactoring budget allows; otherwise simple non-transactional update is acceptable

**Source 6: Alternative Query Pattern - Include NULL Inventories**
- URL: SQL Stack Overflow best practices
- **Finding:** Modify query to explicitly include NULL user_id inventories:
  ```sql
  WHERE (i.user_id = $1 OR i.user_id IS NULL)
     OR s.shared_with_user_id = $1
     OR g.grantee_user_id = $1
  ```
- **Pros:** Simple query change, works immediately
- **Cons:** ALL users can see NULL inventories (not just first admin), breaks multi-tenancy
- **Security Risk:** If sample data remains NULL, all users see same inventories (data leak)
- **Recommendation:** **DO NOT USE** - Assignment approach is safer

**Source 7: Idempotent Migration Patterns**
- URL: https://www.brunton-spall.co.uk/post/2014/05/06/database-migrations-done-right/
- **Finding:** Migrations should be idempotent where possible
- **Pattern:** Create assignment migration that runs after user creation:
  - `020_assign_sample_data_to_first_admin.sql`
  - Content: `UPDATE inventories SET user_id = (SELECT id FROM users WHERE is_admin = true ORDER BY created_at LIMIT 1) WHERE user_id IS NULL AND EXISTS (SELECT 1 FROM users WHERE is_admin = true);`
- **Pros:** Database-level, runs automatically, idempotent
- **Cons:** Doesn't run until next docker-compose restart (migrations run on startup)
- **Recommendation:** Use as complementary approach (belt-and-suspenders)

**Source 8: PostgreSQL Transaction Isolation Levels**
- URL: https://www.postgresql.org/docs/current/transaction-iso.html
- **Finding:** Default isolation is READ COMMITTED
- **Race Condition:** Two first-time setups simultaneously could both see user_count=0
- **Mitigation:** Use `SELECT FOR UPDATE` lock when checking user count:
  ```sql
  SELECT COUNT(*) FROM users FOR UPDATE;
  ```
- **Recommendation:** Low risk in single-instance home app, but good practice

### Proposed Solution Architecture

**Two-Part Solution:**

#### Part A: Application-Level Auto-Assignment (Primary Solution)

**Modify `src/api/auth.rs::initial_setup()` function:**

After successful user creation (around line 210), add:

```rust
// Auto-assign sample inventories (with NULL user_id) to this first admin
match db_service.assign_sample_inventories_to_user(user.id).await {
    Ok(assigned_count) => {
        if assigned_count > 0 {
            info!(
                "Assigned {} sample inventories to first admin user: {}",
                assigned_count, user.username
            );
        }
    }
    Err(e) => {
        // Non-fatal: log warning but don't fail setup
        warn!("Failed to assign sample inventories: {}", e);
    }
}
```

**Add new method to `src/db/mod.rs`:**

```rust
/// Assign all inventories with NULL user_id to the specified user
/// Used during initial setup to assign sample data to first admin
pub async fn assign_sample_inventories_to_user(
    &self,
    user_id: Uuid,
) -> Result<u64, Box<dyn std::error::Error>> {
    let client = self.pool.get().await?;
    
    let result = client
        .execute(
            "UPDATE inventories SET user_id = $1, updated_at = NOW() WHERE user_id IS NULL",
            &[&user_id],
        )
        .await?;
    
    Ok(result)
}
```

**Behavior:**
- Runs automatically during first admin setup
- Assigns all NULL inventories to that admin
- Non-blocking (warning logged on failure, setup continues)
- Idempotent (re-running has no effect if no NULL inventories remain)

#### Part B: Migration-Based Assignment (Defensive Backup)

**Create new migration: `020_assign_sample_data_to_first_admin.sql`**

```sql
-- Auto-assign sample inventories (with NULL user_id) to the first admin user
-- This migration is idempotent and safe to run multiple times
-- It only assigns inventories that still have NULL user_id when an admin exists

UPDATE inventories 
SET user_id = (
    SELECT id 
    FROM users 
    WHERE is_admin = true 
    ORDER BY created_at 
    LIMIT 1
),
updated_at = NOW()
WHERE user_id IS NULL 
  AND EXISTS (SELECT 1 FROM users WHERE is_admin = true);

-- Log result for audit trail
DO $$ 
DECLARE 
    assigned_count INT;
BEGIN
    GET DIAGNOSTICS assigned_count = ROW_COUNT;
    IF assigned_count > 0 THEN
        RAISE NOTICE 'Assigned % sample inventories to first admin user', assigned_count;
    END IF;
END $$;
```

**Behavior:**
- Runs on next `docker-compose up` after restart
- Catches any cases where Part A didn't execute (e.g., manual SQL user creation)
- Idempotent: safe to run multiple times
- Has no effect if Part A already assigned inventories

### Implementation Steps

1. **Add `assign_sample_inventories_to_user()` method to `DatabaseService`** (src/db/mod.rs)
2. **Modify `initial_setup()` endpoint to call assignment method** (src/api/auth.rs)
3. **Create migration 020** to provide defensive backup (migrations/020_assign_sample_data_to_first_admin.sql)
4. **Update migration 019 comment** to reference automated assignment
5. **Test setup flow:**
   - Fresh database, no users
   - Create first admin via `/auth/setup`
   - Verify sample inventories visible in `/api/inventories`
6. **Test re-run safety:**
   - CreateRun setup again (should fail with "already completed")
   - Restart container (migration 020 should have no effect)

### Dependencies and Requirements

**Existing Dependencies (No Changes Required):**
- `tokio-postgres` (already in use via deadpool-postgres)
- `uuid = { version = "1", features = ["v4"] }` (already in Cargo.toml)

**Database Schema:**
- No schema changes required
- Works with existing `inventories` table structure from migration 014

### Potential Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Sample data assigned to wrong user | User sees unexpected inventories | Only runs during initial setup (user_count == 0 check exists) |
| Race condition in multi-threaded setup | Two admins claim same inventories | Low risk: home app, single instance; could add SELECT FOR UPDATE if needed |
| Migration 020 runs before app assigns | Harmless duplicate logic | Both approaches are idempotent; last-wins semantics |
| Sample data deleted before assignment | Assignment fails silently | Acceptable: sample data is optional, failure logged as warning |
| User creates admin via SQL directly | Sample inventories not assigned | Migration 020 catches this case on next restart |

---

## Testing Strategy

### Issue 1: Cache Control Headers

**Manual Testing:**

1. **Verify Cache Headers:**
   ```powershell
   # Test index.html
   curl -I http://localhost:8210/
   # Expected: Cache-Control: no-cache, no-store, must-revalidate
   
   # Test service worker
   curl -I http://localhost:8210/sw.js
   # Expected: Cache-Control: no-cache, max-age=0
   
   # Test hashed asset
   curl -I http://localhost:8210/assets/index-Lx9RgoJs.js
   # Expected: Cache-Control: public, max-age=31536000, immutable (or default Actix behavior)
   ```

2. **Verify Service Worker Update:**
   - Build and deploy v1
   - Open app, note loaded
   - Modify frontend, rebuild Docker image (v2)
   - Refresh page (normal F5)
   - Expected: New service worker detected, skipWaiting triggered, new assets load without hard refresh
   - Verify in DevTools → Application → Service Workers: see "waiting" then "activated"

3. **Browser Testing:**
   - Chrome: DevTools → Network → Disable cache OFF
   - Firefox: Network Monitor → check headers
   - Safari: Develop → Show Web Inspector → Network

**Automated Testing (Rust Integration Test):**

```rust
#[actix_rt::test]
async fn test_static_file_cache_headers() {
    let app = test::init_service(App::new().configure(configure_routes)).await;
    
    // Test index.html has no-cache
    let req = test::TestRequest::get().uri("/").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let cache_control = resp.headers().get("cache-control").unwrap();
    assert!(cache_control.to_str().unwrap().contains("no-cache"));
    
    // Test service worker has no-cache
    let req = test::TestRequest::get().uri("/sw.js").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let cache_control = resp.headers().get("cache-control").unwrap();
    assert!(cache_control.to_str().unwrap().contains("no-cache"));
}
```

### Issue 2: Sample Data Assignment

**Manual Testing:**

1. **Fresh Setup Test:**
   ```powershell
   # Reset database
   docker-compose down -v
   docker-compose up -d
   
   # Wait for healthy state
   docker-compose ps
   
   # Create first admin via setup wizard (or curl):
   curl -X POST http://localhost:8210/api/auth/setup \
     -H "Content-Type: application/json" \
     -d '{"username":"admin","password":"SecurePass123!","full_name":"Admin User","inventory_name":"My First Inventory"}'
   
   # Query inventories
   curl http://localhost:8210/api/inventories \
     -H "Authorization: Bearer <token_from_setup_response>"
   
   # Expected: See 6 inventories (5 sample + 1 "My First Inventory")
   ```

2. **Verify Database State:**
   ```sql
   -- Connect to database
   docker exec -it home-registry-db-1 psql -U postgres -d home_inventory
   
   -- Check sample inventories assigned
   SELECT id, name, user_id FROM inventories WHERE id BETWEEN 100 AND 104;
   -- Expected: All have user_id matching first admin
   
   -- Check first admin created
   SELECT id, username, is_admin FROM users ORDER BY created_at LIMIT 1;
   ```

3. **Idempotency Test:**
   - Restart container: `docker-compose restart app`
   - Migration 020 runs again
   - Query inventories: should see same 6 inventories (no duplicates)

**Automated Testing (Rust Integration Test):**

```rust
#[tokio::test]
async fn test_initial_setup_assigns_sample_inventories() {
    let pool = test_helpers::setup_test_database().await;
    let db_service = DatabaseService::new(pool.clone());
    
    // Simulate initial setup
    let setup_request = InitialSetupRequest {
        username: "testadmin".to_string(),
        password: "Test123!".to_string(),
        full_name: "Test Admin".to_string(),
        inventory_name: Some("Test Inventory".to_string()),
    };
    
    // Call setup (via API or directly)
    let user = db_service.create_user(
        &setup_request.username,
        &setup_request.full_name,
        &hash_password(setup_request.password).await.unwrap(),
        true,
        true,
    ).await.unwrap();
    
    // Auto-assignment should trigger
    let assigned_count = db_service
        .assign_sample_inventories_to_user(user.id)
        .await
        .unwrap();
    
    assert!(assigned_count > 0, "Sample inventories should be assigned");
    
    // Verify accessible
    let inventories = db_service
        .get_accessible_inventories(user.id)
        .await
        .unwrap();
    
    assert!(inventories.len() >= 5, "Should see at least 5 sample inventories");
    
    // Verify all have user_id set
    for inv in inventories {
        assert!(inv.user_id.is_some(), "All inventories should have user_id");
    }
}
```

---

## Implementation Checklist

### Issue 1: Cache Control Headers

- [ ] Modify `src/main.rs`:
  - [ ] Update `/` route to serve index.html with `no-cache, no-store, must-revalidate`
  - [ ] Update `/sw.js` route with `no-cache, max-age=0`
  - [ ] Update `/manifest.json` route with `no-cache, max-age=0`
  - [ ] Update logo routes with `public, max-age=86400`
  - [ ] Keep `/assets` route unchanged (defaults are acceptable for hashed files)
- [ ] Build and test locally with `cargo build && cargo run`
- [ ] Verify cache headers with curl/browser DevTools
- [ ] Test service worker update flow
- [ ] Commit changes

### Issue 2: Sample Data Assignment

- [ ] Add `assign_sample_inventories_to_user()` method to `src/db/mod.rs`
- [ ] Modify `initial_setup()` in `src/api/auth.rs` to call assignment after user creation
- [ ] Create `migrations/020_assign_sample_data_to_first_admin.sql`
- [ ] Update comment in `migrations/019_add_sample_inventory_data.sql` to reference automatic assignment
- [ ] Test fresh setup flow:
  - [ ] `docker-compose down -v`
  - [ ] `docker-compose up -d`
  - [ ] Create admin via setup wizard
  - [ ] Verify sample inventories visible
- [ ] Test idempotency (restart container, check no duplicates)
- [ ] Commit changes

### Documentation Updates

- [ ] Update `README.md` with:
  - [ ] Note about cache headers for production deployments
  - [ ] Explanation of sample data auto-assignment
- [ ] Update `docs/ADMIN_GUIDE.md` (if exists) with:
  - [ ] First-time setup wizard behavior
  - [ ] Sample data ownership
- [ ] Add comment in code explaining cache strategy rationale

---

## Conclusion

Both issues have well-researched solutions with minimal risk:

1. **Cache Control Fix:** Targeted header changes in static route handlers prevent aggressive browser/service worker caching of entry points while preserving performance benefits for hashed assets.

2. **Sample Data Assignment:** Two-layer approach (application-level + migration backup) ensures first admin automatically receives sample inventories without manual SQL intervention.

Implementation should take approximately 2-3 hours for both fixes combined, with low risk of regression due to isolated scope of changes.

---

**Next Steps:** Proceed to Implementation Phase with dedicated subagent following this specification.
