# Docker Migration Bundling Specification

**Created:** 2026-02-15  
**Author:** Research Agent  
**Priority:** High - Deployment Experience Improvement

---

## Executive Summary

Research shows the optimal solution is to adopt the **Humidor project's approach**: use the `refinery` crate to embed migrations into the application binary and run them programmatically at startup. This eliminates the need for volume-mounting migrations and enables true one-command deployment.

**Key Finding:** Home Registry's migrations are ALREADY copied into the Docker image (Dockerfile line 133), but the current docker-compose.yml volume-mount **overwrites** them, forcing users to have a local migrations directory.

---

## Current State Analysis

### 1. How It Works Now

#### Dockerfile (Positive Discovery)
```dockerfile
# Line 133 - Migrations ARE bundled into image
COPY --chown=appuser:appgroup migrations ./migrations
```
✅ **Migrations are already in the Docker image at `/app/migrations`**

#### docker-compose.yml (Development - Problem Source)
```yaml
db:
  volumes:
    - ./migrations:/docker-entrypoint-initdb.d  # ❌ Overwrites bundled migrations
```
**Problem:** This volume mount requires users to have the migrations directory locally.

#### docker-compose.prod.yml (Production - Partially Working)
```yaml
db:
  volumes:
    - pgdata:/var/lib/postgresql/data  # ✅ No migrations mount
```
**Issue:** Migrations bundled in app image are NOT accessible to PostgreSQL container.

### 2. PostgreSQL docker-entrypoint-initdb.d Limitations

From documentation and testing:
- ❌ Only runs on **first database initialization** (when data directory is empty)
- ❌ Does NOT run if database already exists (persistent volume)
- ❌ Users with existing data volumes won't get new migrations
- ❌ No automatic migration tracking or rollback
- ✅ Works well for truly fresh deployments

### 3. User Experience Impact

**Current Deployment Requirements:**
```bash
# Users MUST:
1. git clone https://github.com/VictoryTek/home-registry.git
2. cd home-registry
3. docker compose up -d
```

**User Pain Points (from README):**
> "Prerequisites: You **MUST** have the `migrations/` directory available locally"

> "Troubleshooting: Error: 'relation users does not exist'"  
> "Cause: Database migrations were not applied during first initialization."  
> "Solution: `docker compose down -v` (⚠️ This deletes all data!)"

---

## Problem Statement

Users cannot deploy Home Registry without cloning the repository first. The goal is to enable deployment with just:

```bash
docker run -e DATABASE_URL=postgres://... ghcr.io/victorytek/home-registry:beta
```

Or with Docker Compose using only the image:
```yaml
services:
  app:
    image: ghcr.io/victorytek/home-registry:beta
    # No volume mounts needed!
```

---

## Research: Approaches Considered

### Approach 1: Refinery Crate (Programmatic Migrations) ⭐ RECOMMENDED

**How Humidor Does It:**

1. **Add dependency** - `Cargo.toml`:
   ```toml
   refinery = { version = "0.8", features = ["tokio-postgres"] }
   ```

2. **Embed migrations** - `src/main.rs`:
   ```rust
   use refinery::embed_migrations;
   
   // Embed migrations from the migrations directory at compile time
   embed_migrations!("migrations");
   ```

3. **Run at startup** - `src/main.rs`:
   ```rust
   // Get database client from pool
   let mut client = pool.get().await?;
   
   // Run database migrations using refinery (idempotent)
   log::info!("Running database migrations...");
   match migrations::runner().run_async(&mut **client).await {
       Ok(report) => {
           log::info!(
               "Database migrations completed successfully. Applied: {}",
               report.applied_migrations().len()
           );
       }
       Err(e) => {
           log::error!("Database migrations failed: {}", e);
           return Err(e.into());
       }
   }
   
   drop(client);  // Return to pool
   ```

4. **Result:**
   - Migrations embedded in binary at compile time
   - Runs automatically on every app startup
   - Idempotent: only applies missing migrations
   - Tracks applied migrations in `refinery_schema_history` table
   - Works with existing databases
   - No volume mounting required

**Pros:**
- ✅ Migrations bundled into app binary (no external files)
- ✅ Runs on every startup (not just first DB init)
- ✅ Idempotent and safe (tracks what's applied)
- ✅ Works with existing databases (applies only new migrations)
- ✅ Standard Rust pattern (used by Humidor and many projects)
- ✅ No PostgreSQL container changes needed
- ✅ Enables true `docker run` deployment
- ✅ Zero extra files or init containers

**Cons:**
- ⚠️ Migration files must exist at compile time
- ⚠️ Need to rebuild app image for new migrations (standard practice)
- ⚠️ Small code changes in `main.rs` (minimal - ~15 lines)

**Security:**
- ✅ Migrations read-only in binary
- ✅ No runtime file access needed
- ✅ Database credentials only in environment variables

---

### Approach 2: PostgreSQL Init Container Pattern

**How It Would Work:**

```yaml
services:
  db-init:
    image: ghcr.io/victorytek/home-registry:beta
    command: sh -c "cp -r /app/migrations/* /migrations/"
    volumes:
      - migrations:/migrations
    depends_on:
      - db
      
  db:
    volumes:
      - migrations:/docker-entrypoint-initdb.d
      
volumes:
  migrations:
```

**Pros:**
- ✅ Leverages existing bundled migrations in app image
- ✅ No code changes to application
- ✅ Uses PostgreSQL's native init mechanism

**Cons:**
- ❌ Only runs on first database initialization
- ❌ Doesn't work with existing databases
- ❌ Requires additional container/service in docker-compose
- ❌ Doesn't work for standalone `docker run` deployments
- ❌ Complex for users to set up manually
- ❌ Named volume still required for migrations

---

### Approach 3: Copy Migrations at Runtime (Entrypoint Script)

**How It Would Work:**

1. Create `/app/entrypoint.sh`:
   ```bash
   #!/bin/sh
   # Wait for database
   while ! pg_isready -h $DB_HOST -U $DB_USER; do sleep 1; done
   
   # Copy migrations to PostgreSQL container (requires shared volume)
   # This is problematic - can't write to another container's filesystem
   ```

**Pros:**
- ❌ None - fundamentally flawed approach

**Cons:**
- ❌ Cannot write to another container's filesystem
- ❌ Would require shared volumes (defeats the purpose)
- ❌ Complex and brittle
- ❌ Security concerns (writing to DB container)

---

### Approach 4: Bundle Migrations with PostgreSQL Custom Image

**How It Would Work:**

1. Create custom PostgreSQL Dockerfile:
   ```dockerfile
   FROM postgres:17
   COPY migrations /docker-entrypoint-initdb.d/
   ```

2. Build and push:
   ```bash
   docker build -t ghcr.io/victorytek/home-registry-db:beta .
   ```

3. Use in docker-compose:
   ```yaml
   db:
     image: ghcr.io/victorytek/home-registry-db:beta
   ```

**Pros:**
- ✅ No code changes to application
- ✅ Works with PostgreSQL's native init
- ✅ Migrations bundled in DB image

**Cons:**
- ❌ Only runs on first database initialization
- ❌ Doesn't work with existing databases
- ❌ Requires maintaining two Docker images
- ❌ Version management complexity (app vs migrations)
- ❌ Cannot use stock PostgreSQL image
- ❌ Breaks user expectation of standard PostgreSQL

---

### Approach 5: SQL-in-Code (Hardcoded Migrations)

**How It Would Work:**

```rust
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_create_items_table.sql"),
    include_str!("../migrations/002_create_inventories_table.sql"),
    // ...
];

async fn run_migrations(pool: &Pool) {
    for migration in MIGRATIONS {
        pool.get().await?.execute(migration, &[]).await?;
    }
}
```

**Pros:**
- ✅ No external dependencies
- ✅ Migrations embedded in binary

**Cons:**
- ❌ No migration tracking (doesn't know what's applied)
- ❌ Not idempotent (would fail on re-run)
- ❌ No rollback support
- ❌ Manual implementation of what refinery provides
- ❌ Would need custom tracking table logic
- ❌ Reinventing the wheel

---

## Recommended Solution: Refinery Crate

### Justification

**Refinery is the optimal choice because:**

1. **Already proven in this project** - Humidor uses it successfully
2. **Standard Rust pattern** - Used by many production Rust apps
3. **Idempotent** - Safe to run on every startup
4. **Works with existing databases** - Applies only new migrations
5. **Zero infrastructure overhead** - No extra containers or volumes
6. **Enables true one-command deployment** - `docker run` with DATABASE_URL only
7. **Battle-tested** - Mature library (v0.8+) with tokio-postgres integration
8. **Migration tracking built-in** - Maintains `refinery_schema_history` table
9. **Minimal code changes** - ~15 lines in main.rs

### Architecture Decision

**Before (Current):**
```
┌─────────────────┐      ┌──────────────────────┐
│   App Container │      │  PostgreSQL Container│
│                 │      │                      │
│ /app/migrations │◄──┐  │ /docker-entrypoint-  │
│ (bundled but    │   │  │  initdb.d/           │
│  not used)      │   └──┤ (volume-mounted)     │
└─────────────────┘      │                      │
                         │ Only runs on         │
         ┌──────────┐    │ FIRST init           │
         │ Host FS  │────►│                      │
         │ ./migrat │    └──────────────────────┘
         │  ions/   │    
         └──────────┘    Problem: Requires repo clone
```

**After (With Refinery):**
```
┌─────────────────────────────┐      ┌──────────────────────┐
│   App Container             │      │  PostgreSQL Container│
│                             │      │                      │
│ Binary with embedded ───┬───┼──────┤ Migrations applied   │
│ migrations (compile-    │   │      │ at EVERY startup     │
│ time)                   │   │      │                      │
│                         │   │      │ refinery_schema_     │
│ Runtime: refinery ◄─────┘   │      │ history table tracks │
│ runner applies missing      │      │ applied migrations   │
│ migrations to database ─────┼──────►│                      │
└─────────────────────────────┘      └──────────────────────┘

No host volume mount needed!
Deploy with: docker run -e DATABASE_URL=... ghcr.io/.../home-registry:beta
```

---

## Implementation Steps

### Phase 1: Add Refinery Dependency

**File:** `Cargo.toml`

**Changes:**
```toml
[dependencies]
# Add after existing database dependencies
refinery = { version = "0.8", features = ["tokio-postgres"] }
```

**Note:** Use version 0.8 to match Humidor's proven configuration.

---

### Phase 2: Modify Application Startup

**File:** `src/main.rs`

**Location:** After pool initialization (currently line 66), before rate limiting setup

**Add these imports:**
```rust
use refinery::embed_migrations;
```

**Add after imports (around line 10):**
```rust
// Embed migrations from the migrations directory at compile time
embed_migrations!("migrations");
```

**Add migration runner (after pool initialization, around line 70):**
```rust
// Run database migrations automatically at startup
log::info!("Running database migrations...");
let mut client = match pool.get().await {
    Ok(c) => c,
    Err(e) => {
        log::error!("Failed to get database connection for migrations: {}", e);
        std::process::exit(1);
    }
};

match migrations::runner().run_async(&mut **client).await {
    Ok(report) => {
        let applied_count = report.applied_migrations().len();
        if applied_count > 0 {
            log::info!(
                "Database migrations completed successfully. Applied {} new migration(s)",
                applied_count
            );
        } else {
            log::info!("Database schema is up to date. No new migrations to apply");
        }
    }
    Err(e) => {
        log::error!("Database migrations failed: {}", e);
        log::error!(
            "Cannot start application with outdated database schema. \
             Please check migration files and database connectivity."
        );
        std::process::exit(1);
    }
}

// Drop the migration client back to the pool
drop(client);
log::info!("Migration client returned to pool");
```

**Rationale:**
- Runs migrations before accepting HTTP requests
- Fails fast if migrations fail (cannot run with wrong schema)
- Logs clearly for debugging
- Returns connection to pool after migrations

---

### Phase 3: Update docker-compose.yml (Development)

**File:** `docker-compose.yml`

**Changes:**
Remove the migrations volume mount from db service:

**BEFORE:**
```yaml
db:
  volumes:
    - pgdata:/var/lib/postgresql/data
    - ./migrations:/docker-entrypoint-initdb.d  # ❌ REMOVE THIS
```

**AFTER:**
```yaml
db:
  volumes:
    - pgdata:/var/lib/postgresql/data
    # Migrations now run programmatically from app container
```

**Rationale:**
- App will run migrations on startup
- No need for PostgreSQL's init mechanism
- Consistent with production deployment (docker-compose.prod.yml already doesn't have this)

---

### Phase 4: Update Documentation

**File:** `README.md`

#### 4.1 Update Prerequisites Section

**BEFORE:**
```markdown
**Prerequisites:**
- You **MUST** have the `migrations/` directory available locally
- Two options to obtain it:
  1. **Clone the repository** (recommended)
  2. **Download just the migrations folder** from GitHub
```

**AFTER:**
```markdown
**Prerequisites:**
- Docker and Docker Compose installed
- PostgreSQL database (included in docker-compose.yml)

**✨ New:** No repository cloning required! Migrations are bundled in the image.
```

#### 4.2 Simplify Docker Compose Example

**BEFORE:**
```yaml
volumes:
  - ./migrations:/docker-entrypoint-initdb.d  # CRITICAL: Required for database schema
```

**AFTER:**
```yaml
volumes:
  - pgdata:/var/lib/postgresql/data
  # Migrations run automatically from app container - no volume mount needed!
```

#### 4.3 Update "Option 2: Docker Run Commands" Section

**BEFORE:**
```markdown
> **Note:** This method requires manual migration setup. Docker Compose (Option 1) is recommended.

# Start database with migrations volume
# IMPORTANT: Replace /path/to/migrations with your actual migrations directory path
docker run -d \
  -v /path/to/migrations:/docker-entrypoint-initdb.d \
  ...
```

**AFTER:**
```markdown
**Now simplified** - No migration volumes required:

```bash
# Create network
docker network create home-registry-net

# Start database (no migration volumes needed!)
docker run -d \
  --name home-registry-db \
  --network home-registry-net \
  -p 5432:5432 \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=homeregistry2026 \
  -e POSTGRES_DB=home_inventory \
  -v home-registry-pgdata:/var/lib/postgresql/data \
  --restart unless-stopped \
  postgres:17

# Start application (migrations run automatically!)
docker run -d \
  --name home-registry-app \
  --network home-registry-net \
  -p 8210:8210 \
  -e DATABASE_URL=postgres://postgres:homeregistry2026@home-registry-db:5432/home_inventory \
  -e PORT=8210 \
  -e RUST_LOG=info \
  -v home-registry-appdata:/app/data \
  -v home-registry-backups:/app/backups \
  --restart unless-stopped \
  ghcr.io/victorytek/home-registry:beta
```

**That's it!** The app will automatically run migrations on startup.
```

#### 4.4 Update Troubleshooting Section

**REMOVE:** The entire "Error: 'relation users does not exist'" section (no longer applicable)

**ADD:**
```markdown
### Migration Troubleshooting

**Error:** "Database migrations failed"

**Cause:** Application cannot connect to database or migration SQL has errors.

**Solution:**
1. Check database is running: `docker compose ps`
2. Verify DATABASE_URL is correct
3. Check application logs: `docker compose logs app`
4. Ensure database user has CREATE TABLE permissions

**Note:** Migrations run automatically on every startup. The app will not start if migrations fail, ensuring schema consistency.

### Upgrading from Previous Versions

**If you previously volume-mounted migrations:**

1. Remove the volume mount from your `docker-compose.yml`:
   ```yaml
   # REMOVE this line:
   - ./migrations:/docker-entrypoint-initdb.d
   ```

2. Restart the application:
   ```bash
   docker compose down
   docker compose pull  # Get latest image with refinery
   docker compose up -d
   ```

3. Check logs to confirm migrations ran:
   ```bash
   docker compose logs app | grep -i migration
   ```

**Expected output:**
```
home-registry-app | Running database migrations...
home-registry-app | Database schema is up to date. No new migrations to apply
```

Your existing data is preserved - refinery only applies NEW migrations.
```

---

### Phase 5: Update CHANGELOG.md

**File:** `CHANGELOG.md`

**Add under next version:**
```markdown
## [Unreleased]

### Changed
- **BREAKING (Deployment):** Migrations now run programmatically via refinery crate
  - No longer requires volume-mounting `./migrations:/docker-entrypoint-initdb.d`
  - Deploy with just Docker image - no repository clone needed
  - Migrations embedded in application binary at compile time
  - Automatically applied on every app startup (idempotent)
  
### Added
- Automatic database migration on application startup using refinery 0.8
- Migration status logging shows applied vs skipped migrations
- App fails fast if migrations fail (ensures schema consistency)

### Removed
- Requirement to volume-mount migrations directory in docker-compose
- Documentation references to cloning repository for migrations
- Troubleshooting section for "relation does not exist" errors (obsolete)

### Migration Guide
If upgrading from a version that required `./migrations:/docker-entrypoint-initdb.d`:
1. Remove that volume mount from your docker-compose.yml
2. Pull latest image: `docker compose pull`
3. Restart: `docker compose up -d`
4. Verify in logs: `docker compose logs app | grep migration`

Your existing data is preserved - only new migrations are applied.
```

---

### Phase 6: Test the Implementation

#### 6.1 Fresh Database Test

```bash
# Clean slate
docker compose down -v  # Remove all volumes
docker compose build    # Rebuild with refinery
docker compose up -d

# Verify migrations ran
docker compose logs app | grep -i migration

# Expected output:
# Running database migrations...
# Database migrations completed successfully. Applied 21 new migration(s)

# Verify schema exists
docker compose exec db psql -U postgres -d home_inventory -c "\dt"

# Should show: items, inventories, users, etc.
```

#### 6.2 Existing Database Test

```bash
# Keep existing database
docker compose stop app
docker compose rm -f app

# Rebuild and restart app only
docker compose build app
docker compose up -d app

# Verify migrations checked but not re-applied
docker compose logs app | grep -i migration

# Expected output:
# Running database migrations...
# Database schema is up to date. No new migrations to apply
```

#### 6.3 Production Image Test (docker-compose.prod.yml)

```bash
# Pull from GitHub Container Registry
docker compose -f docker-compose.prod.yml pull

# Start with fresh DB
docker compose -f docker-compose.prod.yml down -v
docker compose -f docker-compose.prod.yml up -d

# Verify
docker compose -f docker-compose.prod.yml logs app | grep -i migration
```

#### 6.4 Standalone Docker Run Test

```bash
# Start PostgreSQL
docker network create home-registry-net
docker run -d --name test-db --network home-registry-net \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=testpass \
  -e POSTGRES_DB=home_inventory \
  postgres:17

# Wait for DB ready
sleep 5

# Start app (no volumes, just env vars!)
docker run -d --name test-app --network home-registry-net \
  -p 8210:8210 \
  -e DATABASE_URL=postgres://postgres:testpass@test-db:5432/home_inventory \
  -e RUST_LOG=info \
  ghcr.io/victorytek/home-registry:beta

# Verify
docker logs test-app | grep -i migration

# Cleanup
docker stop test-app test-db
docker rm test-app test-db
docker network rm home-registry-net
```

#### 6.5 Integration Tests

**Update:** `scripts/preflight.sh` and `scripts/preflight.ps1`

Currently the scripts check if database is running and run integration tests. No changes needed to test scripts - they already test against a running database. The migrations will now run automatically when the app starts during testing.

---

## Dependencies and Requirements

### New Dependencies

1. **Cargo.toml:**
   - `refinery = "0.8"` with `tokio-postgres` feature
   - Already compatible with existing `tokio-postgres = "0.7"`

2. **Build Time:**
   - Migration files must exist in `migrations/` directory at compile time
   - Already satisfied - migrations are in repo and copied at Dockerfile line 133

### Infrastructure Requirements

**None!** That's the point - this eliminates infrastructure requirements.

❌ **No longer required:**
- Volume mounting from host
- Init containers
- Custom PostgreSQL images
- Shared volumes between containers

✅ **Only required:**
- PostgreSQL 17 container (already required)
- DATABASE_URL environment variable (already required)

---

## Dockerfile Changes Analysis

**Current Dockerfile Analysis:**

Line 133 already copies migrations:
```dockerfile
COPY --chown=appuser:appgroup migrations ./migrations
```

✅ **No Dockerfile changes required!**

The migrations are already bundled. We just need to use them programmatically instead of relying on PostgreSQL's init mechanism.

---

## docker-compose.yml Changes Summary

### docker-compose.yml (Development)

**Change:** Remove migrations volume mount from `db` service

**BEFORE:**
```yaml
db:
  volumes:
    - pgdata:/var/lib/postgresql/data
    - ./migrations:/docker-entrypoint-initdb.d
```

**AFTER:**
```yaml
db:
  volumes:
    - pgdata:/var/lib/postgresql/data
```

### docker-compose.prod.yml (Production)

✅ **No changes required!** Already doesn't have migrations volume mount.

---

## Testing Approach

### Unit Tests

No new unit tests needed - refinery is a mature library that handles migration logic internally.

### Integration Tests

**Existing tests in `tests/` already validate:**
- Database connection (test_db.rs)
- API integration with database (test_api_integration.rs)
- Auth flow with database (test_auth.rs)

**What changes:**
- Tests will now see migrations applied automatically when app starts
- No manual migration application needed in test fixtures

**Verify:**
```bash
# Run full integration test suite
cargo test -- --include-ignored

# All existing tests should still pass
# Database schema will be up-to-date via refinery
```

### Manual Testing Checklist

- [ ] Fresh deployment (`docker compose down -v && docker compose up -d`)
  - [ ] App starts successfully
  - [ ] Logs show migrations applied
  - [ ] Can create initial admin user
  - [ ] Can log in and use app

- [ ] Existing database (`docker compose restart app`)
  - [ ] App starts successfully  
  - [ ] Logs show "schema is up to date"
  - [ ] Existing data intact
  - [ ] App functions normally

- [ ] Production image (`docker-compose.prod.yml`)
  - [ ] Pulls from ghcr.io successfully
  - [ ] Migrations run automatically
  - [ ] No volume mount errors

- [ ] Standalone docker run
  - [ ] Can deploy with just `docker run` commands
  - [ ] No need for local files
  - [ ] Migrations run successfully

- [ ] Migration failure scenario
  - [ ] Stop database mid-startup
  - [ ] App should fail gracefully with clear error
  - [ ] App should not accept requests with failed migrations

---

## Potential Risks and Mitigations

### Risk 1: Migration Compilation Failures

**Risk:** If migration SQL files have syntax errors not caught until compile time.

**Impact:** Build will fail

**Mitigation:**
- Existing migrations are already validated (working in production)
- CI pipeline runs `cargo build` which will catch any issues
- Preflight scripts already test compilation

**Likelihood:** Low - migrations already work

---

### Risk 2: Migration Naming Conflicts

**Risk:** Refinery uses migration filenames to track applied migrations. File renames could cause issues.

**Impact:** Refinery might try to re-apply renamed migrations

**Mitigation:**
- Document: Never rename migration files after deployment
- Refinery uses checksums - will detect if content changed
- If renaming needed, create new migration instead

**Likelihood:** Very Low - standard practice is immutable migrations

---

### Risk 3: Database Permissions

**Risk:** Database user might lack CREATE TABLE permission (needed for `refinery_schema_history`).

**Impact:** Migration tracking table creation fails

**Mitigation:**
- Default PostgreSQL user is superuser (has all permissions)
- For custom users, document required permissions: `GRANT CREATE ON DATABASE home_inventory TO user;`
- Error message clearly indicates permission issue

**Likelihood:** Low - default setup works

---

### Risk 4: Slow Startup on Large Migrations

**Risk:** Running many migrations could slow app startup.

**Impact:** Health checks might fail during first startup

**Mitigation:**
- Current 21 migrations execute in <2 seconds (mostly CREATE TABLE)
- Health check has `start_period: 5s` buffer
- Increase if needed: `start_period: 10s`
- Migrations only run fully on first startup; subsequent starts check tracking table (<100ms)

**Likelihood:** Low - existing migrations are fast

---

### Risk 5: Concurrent Startup (Multiple App Replicas)

**Risk:** If multiple app containers start simultaneously, they might try to run migrations concurrently.

**Impact:** Race condition, duplicate migration attempts

**Mitigation:**
- Refinery uses database-level locks to prevent concurrent migration
- PostgreSQL's transaction system ensures atomic migration application
- Only one container will successfully apply each migration
- Other containers will see migration already applied and skip

**Likelihood:** Very Low - refinery handles this

---

### Risk 6: Breaking Change for Existing Deployments

**Risk:** Users with `./migrations:/docker-entrypoint-initdb.d` volume mount might have issues.

**Impact:** Confusion about where migrations come from

**Mitigation:**
- Clear documentation in CHANGELOG and README
- Volume mount is harmless if left in place (just unused)
- Migration guide in documentation
- Both methods can coexist during transition

**Likelihood:** Low - well-documented change

---

## Pros and Cons Summary

### Pros ✅

1. **One-Command Deployment** - `docker run` with DATABASE_URL only
2. **No Repository Clone** - Users don't need git or local files
3. **Works with Existing Databases** - Applies only new migrations
4. **Idempotent** - Safe to run repeatedly
5. **Standard Pattern** - Used by Humidor and many Rust projects
6. **Zero Infrastructure Overhead** - No extra containers or volumes
7. **Automatic Migration Tracking** - Built into refinery
8. **Fail Fast** - App won't start with wrong schema
9. **Clear Logging** - Easy to debug migration issues
10. **Already Bundled** - Migrations already in Docker image (Dockerfile line 133)

### Cons ⚠️

1. **Code Changes Required** - ~30 lines in main.rs (minimal)
2. **New Dependency** - Adds `refinery` crate (~small footprint)
3. **Compile-Time Requirement** - Migration files must exist at build (already do)
4. **Breaking Change** - Users must update docker-compose.yml (well-documented)
5. **Startup Delay** - Adds ~2s to first startup (acceptable)

### Comparison to Current Approach

| Aspect | Current (docker-entrypoint-initdb.d) | Proposed (Refinery) |
|--------|--------------------------------------|---------------------|
| Repository clone needed | ❌ Yes | ✅ No |
| Works with existing DB | ❌ No | ✅ Yes |
| Standalone docker run | ❌ No | ✅ Yes |
| Idempotent | ⚠️ Only on fresh DB | ✅ Always |
| Migration tracking | ❌ None | ✅ Built-in |
| Code changes | ✅ None | ⚠️ ~30 lines |
| New dependencies | ✅ None | ⚠️ refinery crate |

**Verdict:** Refinery provides significantly better user experience with minimal technical cost.

---

## Alternative Considered: Hybrid Approach

**Could we keep both methods?**

Theoretically yes:
- Keep volume mount for development (instant Migration file changes)
- Use refinery for production/docker run deployments

**Decision: No - Single method is clearer**

Reasons:
1. Refinery works for both development and production
2. Two methods = two potential points of failure
3. Developers can still edit migration files locally (cargo watch rebuilds)
4. Dockerfile already copies migrations (no dev convenience lost)

---

## Migration from Current Approach

### For End Users (Deploying from ghcr.io)

**Before (Required):**
```bash
git clone https://github.com/VictoryTek/home-registry.git
cd home-registry
docker compose up -d
```

**After (Simplified):**
```bash
# No git clone needed - just pull and run!
docker run -e DATABASE_URL=postgres://... ghcr.io/victorytek/home-registry:beta
```

Or with docker-compose (no local files needed):
```yaml
# No ./migrations volume mount
services:
  app:
    image: ghcr.io/victorytek/home-registry:beta
    environment:
      DATABASE_URL: postgres://postgres:password@db:5432/home_inventory
```

### For Developers

**Before (docker-compose.yml):**
```yaml
db:
  volumes:
    - ./migrations:/docker-entrypoint-initdb.d
```

**After (docker-compose.yml):**
```yaml
db:
  volumes:
    - pgdata:/var/lib/postgresql/data
    # No migrations mount - refinery handles it
```

**Development workflow:**
1. Edit migration file in `migrations/`
2. Rebuild: `docker compose build`
3. Restart: `docker compose up -d`
4. Refinery applies new migration automatically

**No change to:**
- Creating new migration files
- Migration file naming convention (001_, 002_, etc.)
- SQL syntax or content

---

## Documentation Updates Summary

### Files to Update

1. ✅ **README.md** - Major updates to deployment sections
2. ✅ **CHANGELOG.md** - Document breaking change
3. ✅ **docker-compose.yml** - Remove migration volume mount
4. ❌ **docker-compose.prod.yml** - No changes (already correct)
5. ❌ **Dockerfile** - No changes (migrations already copied)

### New Documentation Needed

None! Changes integrate into existing documentation structure.

---

## Success Criteria

### Functional Requirements

- [ ] Users can deploy with `docker run` command (no repo clone)
- [ ] Migrations run automatically on app startup
- [ ] Existing databases upgraded seamlessly
- [ ] New migrations applied without volume mounts
- [ ] App fails gracefully if migrations fail

### Non-Functional Requirements

- [ ] Startup time increase <5 seconds
- [ ] No negative impact on runtime performance
- [ ] Clear error messages for migration failures
- [ ] Existing integration tests pass
- [ ] Documentation clear and accurate

### Validation

**Test all three deployment modes:**
1. ✅ Development (docker-compose.yml)
2. ✅ Production (docker-compose.prod.yml with ghcr.io image)
3. ✅ Standalone (docker run commands only)

---

## Timeline Estimation

| Phase | Description | Estimated Time |
|-------|-------------|----------------|
| 1 | Add refinery dependency to Cargo.toml | 5 minutes |
| 2 | Modify src/main.rs (embed + runner) | 30 minutes |
| 3 | Update docker-compose.yml | 5 minutes |
| 4 | Update README.md documentation | 45 minutes |
| 5 | Update CHANGELOG.md | 15 minutes |
| 6 | Testing (fresh + existing + prod + standalone) | 1 hour |
| 7 | Documentation review and polish | 20 minutes |

**Total: ~3 hours** (actual implementation time)

**Confidence:** High - straightforward changes with proven pattern from Humidor.

---

## References

### Internal References

- **Humidor Implementation:**
  - `analysis/humidor/src/main.rs` lines 16, 28-29, 309-324
  - `analysis/humidor/Cargo.toml` line 23
  - `analysis/humidor/Dockerfile` line 63

- **Current Home Registry:**
  - `Dockerfile` line 133 (migrations already bundled)
  - `docker-compose.yml` lines 13-14 (current volume mount)
  - `README.md` lines 49-87 (deployment instructions)

### External References

1. **Refinery Documentation:**
   - GitHub: https://github.com/rust-db/refinery
   - Docs: https://docs.rs/refinery/
   - Features: https://github.com/rust-db/refinery#features

2. **PostgreSQL Docker Documentation:**
   - Initialization scripts: https://hub.docker.com/_/postgres (see "Initialization scripts" section)
   - Behavior: Only runs on first initialization

3. **Best Practices:**
   - Twelve-Factor App (XII. Admin processes): https://12factor.net/admin-processes
   - Database migrations should run as part of app startup for containerized apps

---

## Implementation Checklist

Use this during implementation phase:

### Code Changes
- [ ] Add refinery to Cargo.toml
- [ ] Add `use refinery::embed_migrations;` to main.rs
- [ ] Add `embed_migrations!("migrations");` after imports
- [ ] Add migration runner code after pool initialization
- [ ] Test compilation: `cargo build`

### Configuration Changes  
- [ ] Remove migrations volume mount from docker-compose.yml
- [ ] Verify docker-compose.prod.yml (no changes needed)
- [ ] Update docker-compose examples in README.md
- [ ] Update CHANGELOG.md with breaking change notice

### Testing
- [ ] Fresh database test (down -v, up -d)
- [ ] Existing database test (restart app)
- [ ] Production image test (docker-compose.prod.yml)
- [ ] Standalone docker run test
- [ ] Integration test suite (cargo test -- --include-ignored)

### Documentation
- [ ] Update README.md prerequisites
- [ ] Update README.md docker-compose examples
- [ ] Update README.md docker run commands
- [ ] Update README.md troubleshooting
- [ ] Add migration guide to CHANGELOG.md
- [ ] Review all documentation for accuracy

### Validation
- [ ] Build succeeds: `cargo build`
- [ ] Tests pass: `cargo test -- --include-ignored`
- [ ] Docker image builds: `docker compose build`
- [ ] Preflight passes: `./scripts/preflight.sh` (or .ps1 on Windows)
- [ ] Can deploy fresh: `docker compose down -v && docker compose up -d`
- [ ] Can upgrade existing: `docker compose restart app`

---

## Questions for Review

Before proceeding to implementation, consider:

1. **Version Compatibility:** Should we pin refinery to 0.8.x or allow 0.8+ ?
   - **Recommendation:** Pin to `0.8` to match Humidor's proven config

2. **Migration History Table Name:** Default is `refinery_schema_history` - acceptable?
   - **Recommendation:** Keep default - standard and recognizable

3. **Startup Failure Behavior:** Should app retry migrations or fail immediately?
   - **Recommendation:** Fail immediately - clear error is better than hanging

4. **Logging Level:** Should migration logs be INFO or DEBUG?
   - **Recommendation:** INFO for applied migrations, DEBUG for skipped ones

5. **Health Check Timing:** Current `start_period: 5s` - sufficient?
   - **Recommendation:** Keep 5s, document to increase if migrations slow

---

## Conclusion

**Recommended Action:** Proceed with Refinery implementation.

This solution:
- ✅ Solves the core problem (no repo clone required)
- ✅ Improves deployment experience significantly  
- ✅ Uses proven pattern from Humidor project
- ✅ Minimal code changes (~30 lines)
- ✅ No infrastructure overhead
- ✅ Works with existing databases
- ✅ Maintains all security properties

**Next Steps:**
1. Review this specification
2. Spawn implementation subagent with references to:
   - This spec file path
   - Implementation checklist
   - Code snippets provided
3. After implementation, spawn review subagent to validate:
   - Code quality
   - Documentation accuracy
   - Testing completeness

---

**End of Specification Document**
