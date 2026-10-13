# Service Worker/Workbox 404 Error - Research & Specification

**Date:** February 12, 2026  
**Issue:** Service worker (`sw.js`) fails to load `workbox-57649e2b.js` from `http://localhost:8210/`, receiving 404 error.

---

## Executive Summary

The service worker 404 error is caused by a **mismatch between where VitePWA generates service worker files (frontend/dist/) and where the Rust backend expects to serve them (static/)**. In the Docker build, this works because the Dockerfile copies frontend/dist/ → static/. However, in local development, the static/ directory doesn't exist, causing all service worker requests to fail.

**Root Cause:** Development workflow gap - service worker files generated in one location, served from another.

**Proposed Solution:** Create a build script to sync frontend/dist/ to static/ for local development, and improve the development experience with better error messages and documentation.

---

## Current State Analysis

### 1. Frontend Configuration

#### VitePWA Configuration ([vite.config.ts](c:\Projects\home-registry\frontend\vite.config.ts))

```typescript
VitePWA({
  registerType: 'autoUpdate',
  injectRegister: 'inline',
  devOptions: {
    enabled: true  // ⚠️ Service worker enabled in development
  },
  includeAssets: ['logo_icon.png', 'logo_full.png'],
  manifest: { /* ... */ },
  workbox: {
    globPatterns: ['**/*.{js,css,html,ico,png,svg,woff,woff2}'],
    runtimeCaching: [ /* ... */ ]
  }
})
```

**Key Findings:**
- ✅ VitePWA plugin installed (`vite-plugin-pwa@0.21.1`)
- ✅ Service worker generation configured with Workbox
- ✅ Development mode enabled (`devOptions.enabled: true`)
- ✅ Global file patterns include all necessary asset types
- ✅ Runtime caching configured for fonts, CDN resources, and API calls
- ⚠️ Service worker files generated in `frontend/dist/` during build

#### Build Output Location
- **Build directory:** `frontend/dist/`
- **Generated files:**
  - `dist/sw.js` - Main service worker file
  - `dist/workbox-57649e2b.js` - Workbox runtime (hashed filename)
  - `dist/manifest.webmanifest` - PWA manifest
  - `dist/assets/*` - Application bundle and assets

**Evidence from previous builds:**
```
dist/sw.js
dist/workbox-57649e2b.js
dist/manifest.webmanifest
dist/index.html
dist/assets/index-Ck3jpsTa.css
dist/assets/index-BP9SvQAK.js
```

#### Dependencies ([package.json](c:\Projects\home-registry\frontend\package.json))

```json
{
  "devDependencies": {
    "vite-plugin-pwa": "0.21.1",
    "vite": "6.4.1"
  },
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build"
  }
}
```

### 2. Rust Backend Static File Serving

#### Service Worker Routes ([src/main.rs](c:\Projects\home-registry\src\main.rs))

```rust
// Service Worker files for PWA
.route("/sw.js", web::get().to(|| async {
    fs::NamedFile::open_async("static/sw.js").await
}))
.route("/workbox-{filename:.*}.js", web::get().to(|path: web::Path<String>| async move {
    let filename = path.into_inner();
    fs::NamedFile::open_async(format!("static/workbox-{filename}")).await
}))
```

**Key Findings:**
- ✅ Routes configured for `/sw.js` and `/workbox-{filename}.js`
- ❌ Both routes expect files in `static/` directory
- ❌ `static/` directory doesn't exist in local development
- ✅ Pattern matching correctly handles hashed Workbox filenames

#### Other Static File Routes

```rust
// Serve static assets (js, css, images, etc.)
.service(fs::Files::new("/assets", "static/assets"))

// Root route - serve index.html
.route("/", web::get().to(|| async {
    fs::NamedFile::open_async("static/index.html").await
}))

// Logo files at root level
.route("/logo_icon.png", web::get().to(|| async {
    fs::NamedFile::open_async("static/logo_icon.png").await
}))
// ... more static routes
```

**Complete static file structure expected:**
```
static/
├── index.html
├── manifest.json (or manifest.webmanifest)
├── sw.js
├── workbox-{hash}.js
├── logo_icon.png
├── logo_full.png
├── favicon.ico
└── assets/
    ├── *.js (app bundles)
    ├── *.css (stylesheets)
    └── *.{png,svg,woff,woff2} (images, fonts)
```

### 3. Docker Build Process

#### Dockerfile Multi-Stage Build ([Dockerfile](c:\Projects\home-registry\Dockerfile))

```dockerfile
# Stage 1: Build React Frontend
FROM node:20.18-alpine3.20 AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci --ignore-scripts && npm cache clean --force
COPY frontend/ ./
RUN npm run build  # ← Builds to frontend/dist/

# Stage 2: Build Rust Backend
FROM rust:1.85-bookworm AS backend-builder
# ... Rust build steps ...

# Stage 3: Final Production Image
FROM debian:bookworm-20241223-slim AS runtime
WORKDIR /app
COPY --from=backend-builder /app/target/release/home-registry ./
COPY --from=frontend-builder /app/frontend/dist ./static  # ← KEY: dist → static
COPY migrations ./migrations
```

**Key Findings:**
- ✅ Frontend builds to `frontend/dist/`
- ✅ Docker copies `frontend/dist/` → `static/` in final image
- ✅ Works perfectly in containerized deployment
- ❌ No equivalent process for local development

### 4. Current Directory Structure

**Local Development (Problem State):**
```
home-registry/
├── frontend/
│   ├── dist/              # ← Generated by `npm run build`
│   │   ├── index.html
│   │   ├── sw.js
│   │   ├── workbox-57649e2b.js
│   │   ├── manifest.webmanifest
│   │   └── assets/
│   ├── public/            # ← Source assets (logos)
│   │   └── logo_icon.png
│   ├── src/
│   ├── package.json
│   └── vite.config.ts
├── src/                   # ← Rust backend
│   ├── main.rs
│   └── api/
├── static/                # ❌ DOES NOT EXIST locally
└── docker-compose.yml
```

**Docker Container (Working State):**
```
/app/
├── home-registry         # Rust binary
├── static/               # ✅ Created from frontend/dist/
│   ├── index.html
│   ├── sw.js
│   ├── workbox-57649e2b.js
│   ├── manifest.webmanifest
│   ├── logo_icon.png
│   └── assets/
└── migrations/
```

### 5. Service Worker Registration

The service worker is registered inline by VitePWA (`injectRegister: 'inline'`), which means the registration code is injected directly into the entry HTML file at build time. The registration attempts to load the service worker from the **backend server** (port 8210), not the Vite dev server (port 3000).

**Expected Registration Flow:**
1. User loads `http://localhost:8210/` (Rust backend)
2. Backend serves `static/index.html` (which includes React app)
3. React app initializes, includes inline SW registration code
4. Browser attempts to register service worker from `http://localhost:8210/sw.js`
5. Backend tries to serve `static/sw.js` → **404 because static/ doesn't exist**

---

## Root Cause Analysis

### Primary Issue: Directory Mismatch

| Environment | Build Output | Backend Serves From | Status |
|-------------|--------------|---------------------|--------|
| **Docker** | `frontend/dist/` | `static/` (copied in Dockerfile) | ✅ Works |
| **Local Dev** | `frontend/dist/` | `static/` (doesn't exist) | ❌ 404 Error |

### Contributing Factors

1. **No Build Artifact Sync**
   - Frontend builds to `dist/`, but no mechanism copies files to `static/` locally
   - Developers must manually run `npm run build` and copy files

2. **Development Workflow Gap**
   - Vite dev server runs on port 3000 (serves from memory/dist)
   - Rust backend runs on port 8210 (expects files in static/)
   - Service worker registration happens from backend, not Vite dev server

3. **Gitignore Configuration**
   - `frontend/dist/` is gitignored (correct)
   - `static/` directory is also gitignored (correct for generated files)
   - But new developers have no way to populate `static/` locally

4. **Documentation Gap**
   - README only mentions Docker Compose setup
   - No instructions for local development without Docker
   - Missing build/deployment workflow documentation

### Secondary Issues

1. **Service Worker in Development**
   - `devOptions.enabled: true` means SW is active during development
   - Can cause caching issues and make debugging harder
   - May not be necessary if developers primarily use Docker

2. **Error Messages**
   - 404 errors don't explain why files are missing
   - No fallback or helpful error message for developers

---

## Research: Best Practices & Patterns

### Source 1: Vite PWA Plugin Documentation - Official Docs
**URL:** [vite-pwa/vite-plugin-pwa GitHub](https://github.com/vite-pwa/vite-plugin-pwa)  
**Key Insights:**

- **Build Output:** VitePWA generates SW files in Vite's build output directory (dist/)
- **Development Mode:** `devOptions.enabled: true` enables SW in dev mode but serves via Vite dev server
- **Deployment Pattern:** After build, copy all dist/ contents to your backend's static directory
- **Common Issue:** "Service worker files must be served from the same origin and path where they're registered"

**Recommendation:** Use build scripts or npm tasks to automate copying dist/ → static/

### Source 2: Workbox Documentation - Google Chrome Docs
**URL:** [developer.chrome.com/docs/workbox/](https://developer.chrome.com/docs/workbox/)  
**Key Insights:**

- **File Generation:** Workbox generates hashed filenames for cache busting (workbox-{hash}.js)
- **Service Worker Scope:** SW must be served from root or appropriate scope for app to work
- **Common 404 Issues:**
  - Files not in expected location
  - Incorrect MIME types
  - Missing HTTPS in production
  - Incorrect base path in service worker registration

**Recommendation:** Ensure all Workbox runtime files are accessible from the same directory as sw.js

### Source 3: Vite PWA Deployment Guide
**URL:** [vite-pwa-org.netlify.app/deployment/](https://vite-pwa-org.netlify.app/deployment/)  
**Key Insights:**

- **Cache-Control Headers:** Don't cache `/`, `/sw.js`, `/index.html`, or `/manifest.webmanifest` with immutable
- **Workbox Files:** All workbox-*.js files should be cached with `max-age=31536000, immutable` (hash-based)
- **Static File Serving:** Backend must serve all files from build output directory
- **Testing:** Use WebPageTest or Lighthouse to verify PWA configuration

**Recommendation:** Configure appropriate cache headers in Actix-Web for different file types

### Source 4: Actix-Web Static Files Documentation
**URL:** (Context7 library docs)  
**Key Insights:**

- **`actix_files::Files`:** Service for serving entire directories
- **`actix_files::NamedFile`:** For serving individual files with custom routes
- **Pattern Matching:** Route parameters can handle dynamic filenames (workbox-{hash}.js)
- **MIME Types:** Actix-files automatically sets correct MIME types based on file extension

**Recommendation:** Use `Files::new("/", "static")` for simpler configuration (serves entire directory)

### Source 5: PWA Deployment Best Practices - web.dev
**URL:** [web.dev/explore/progressive-web-apps](https://web.dev/explore/progressive-web-apps)  
**Key Insights:**

- **Manifest Location:** manifest.webmanifest must be served with `application/manifest+json` MIME type
- **HTTPS Requirement:** Service workers only work on HTTPS (or localhost)
- **Scope Rules:** Service worker scope determines what URLs it can control
- **Update Strategy:** Use `registerType: 'autoUpdate'` for immediate updates

**Recommendation:** Ensure manifest served with correct MIME type (Actix handles this automatically)

### Source 6: Developer Experience - Service Worker Development
**URL:** [developer.chrome.com/docs/workbox/improving-development-experience](https://developer.chrome.com/docs/workbox/improving-development-experience)  
**Key Insights:**

- **Development Mode:** Can be problematic due to caching
- **Skip Waiting:** Use skipWaiting() for immediate activation during dev
- **Unregister SW:** Developer tools can unregister SW for testing
- **Common Issues:** Cached responses, stale content, debugging difficulties

**Recommendation:** Consider disabling SW in local dev or provide clear instructions to unregister

---

## Proposed Solution Architecture

### Design Principles

1. **Docker-First Approach:** Maintain Docker as primary development environment (already working)
2. **Local Dev Support:** Provide optional workflow for non-Docker development
3. **Automation:** Use build scripts to eliminate manual file copying
4. **Documentation:** Clear instructions for both workflows
5. **Error Handling:** Better error messages when static files are missing

### Solution Overview

```
┌─────────────────────────────────────────────────────────────┐
│ DEVELOPER WORKFLOW                                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Option A: Docker Compose (Recommended - Already Works)    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ 1. docker-compose up -d                            │    │
│  │ 2. Access http://localhost:8210                    │    │
│  │ ✅ Service worker works (dist → static in Docker)  │    │
│  └────────────────────────────────────────────────────┘    │
│                                                             │
│  Option B: Local Development (New - Requires Setup)        │
│  ┌────────────────────────────────────────────────────┐    │
│  │ 1. npm run build:full (builds + syncs to static)  │    │
│  │ 2. cargo run (starts backend on 8210)             │    │
│  │ 3. Access http://localhost:8210                    │    │
│  │ ✅ Service worker works (files in static/)         │    │
│  └────────────────────────────────────────────────────┘    │
│                                                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ BUILD PROCESS                                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Frontend Build (npm run build)                            │
│  ┌────────────────────────────────────────────────────┐    │
│  │ TypeScript → JavaScript                            │    │
│  │ VitePWA generates service worker                   │    │
│  │ Workbox runtime bundled                            │    │
│  │ Output: frontend/dist/                             │    │
│  │   ├── index.html                                   │    │
│  │   ├── sw.js                                        │    │
│  │   ├── workbox-{hash}.js                            │    │
│  │   ├── manifest.webmanifest                         │    │
│  │   └── assets/                                      │    │
│  └────────────────────────────────────────────────────┘    │
│                          │                                  │
│                          ▼                                  │
│  Sync Script (npm run sync)                                │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Copy frontend/dist/ → static/                      │    │
│  │ Preserve directory structure                       │    │
│  │ Cross-platform (Windows/Linux/Mac)                 │    │
│  └────────────────────────────────────────────────────┘    │
│                          │                                  │
│                          ▼                                  │
│  Backend Serves (cargo run)                                │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Actix-Web serves from static/                      │    │
│  │ Routes: /, /sw.js, /workbox-*.js, /assets/*        │    │
│  │ Port: 8210                                         │    │
│  └────────────────────────────────────────────────────┘    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### File Structure After Fix

```
home-registry/
├── frontend/
│   ├── dist/                # ← Generated by Vite (gitignored)
│   │   ├── index.html
│   │   ├── sw.js
│   │   ├── workbox-57649e2b.js
│   │   ├── manifest.webmanifest
│   │   └── assets/
│   ├── public/              # ← Source assets
│   ├── src/
│   ├── package.json         # ← Updated with sync script
│   └── vite.config.ts
├── scripts/
│   └── sync-frontend.js     # ← NEW: Cross-platform sync script
├── static/                  # ← NEW: Generated by sync script (gitignored)
│   ├── index.html           # ← Copied from frontend/dist/
│   ├── sw.js
│   ├── workbox-57649e2b.js
│   ├── manifest.webmanifest
│   ├── logo_icon.png        # ← Copied from frontend/public/
│   └── assets/
├── src/                     # ← Rust backend (no changes needed)
├── Cargo.toml
├── Dockerfile               # ← Already correct
├── docker-compose.yml       # ← Already correct
└── README.md                # ← Updated documentation
```

---

## Step-by-Step Implementation Plan

### Phase 1: Create Sync Script

**File:** `scripts/sync-frontend.js`

Create a Node.js script to copy frontend/dist/ → static/

```javascript
#!/usr/bin/env node
const fs = require('fs-extra');
const path = require('path');

const distDir = path.join(__dirname, '..', 'frontend', 'dist');
const staticDir = path.join(__dirname, '..', 'static');
const publicDir = path.join(__dirname, '..', 'frontend', 'public');

async function syncFrontend() {
  console.log('🔄 Syncing frontend build to static directory...');
  
  try {
    // Check if dist directory exists
    if (!fs.existsSync(distDir)) {
      console.error('❌ frontend/dist/ does not exist. Run "npm run build" first.');
      process.exit(1);
    }

    // Remove existing static directory
    if (fs.existsSync(staticDir)) {
      console.log('🗑️  Removing old static directory...');
      await fs.remove(staticDir);
    }

    // Create static directory
    await fs.ensureDir(staticDir);

    // Copy dist contents to static
    console.log('📦 Copying dist/ → static/...');
    await fs.copy(distDir, staticDir);

    // Copy logos and other public assets to static root
    console.log('🖼️  Copying public assets (logos, favicon)...');
    const publicAssets = [
      'logo_icon.png',
      'logo_full.png',
      'logo_icon3.png',
      'favicon.ico'
    ];

    for (const asset of publicAssets) {
      const src = path.join(publicDir, asset);
      const dest = path.join(staticDir, asset);
      if (fs.existsSync(src)) {
        await fs.copy(src, dest);
        console.log(`   ✓ ${asset}`);
      }
    }

    // Copy manifest.json if it exists in public (fallback)
    const manifestSrc = path.join(publicDir, 'manifest.json');
    const manifestDest = path.join(staticDir, 'manifest.json');
    if (fs.existsSync(manifestSrc) && !fs.existsSync(manifestDest)) {
      await fs.copy(manifestSrc, manifestDest);
      console.log('   ✓ manifest.json');
    }

    console.log('✅ Frontend synced successfully!');
    console.log(`   📁 Static directory: ${staticDir}`);
    
    // List key files
    const keyFiles = ['index.html', 'sw.js', 'manifest.webmanifest'];
    console.log('\n📋 Key files:');
    for (const file of keyFiles) {
      const filePath = path.join(staticDir, file);
      if (fs.existsSync(filePath)) {
        console.log(`   ✓ ${file}`);
      } else {
        console.log(`   ⚠️  ${file} (not found)`);
      }
    }

    // Check for workbox files
    const files = await fs.readdir(staticDir);
    const workboxFiles = files.filter(f => f.startsWith('workbox-') && f.endsWith('.js'));
    if (workboxFiles.length > 0) {
      console.log(`   ✓ ${workboxFiles[0]} (Workbox runtime)`);
    } else {
      console.log('   ⚠️  No workbox-*.js files found');
    }

  } catch (error) {
    console.error('❌ Sync failed:', error.message);
    process.exit(1);
  }
}

syncFrontend();
```

**Why this script:**
- ✅ Cross-platform (Node.js works on Windows/Linux/Mac)
- ✅ Uses `fs-extra` for robust file operations
- ✅ Handles missing directories gracefully
- ✅ Copies both dist/ contents and public/ assets
- ✅ Provides clear feedback and error messages

### Phase 2: Update Frontend Package.json

**File:** `frontend/package.json`

Add sync script and dependency:

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "sync": "node ../scripts/sync-frontend.js",
    "build:full": "npm run build && npm run sync",
    "clean": "rm -rf dist ../static",
    "lint": "eslint . --max-warnings 0",
    "preview": "vite preview"
  },
  "devDependencies": {
    "vite-plugin-pwa": "0.21.1",
    "vite": "6.4.1",
    "fs-extra": "^11.2.0"  // ← NEW dependency
  }
}
```

**New Scripts:**
- `npm run sync` - Copy dist/ → static/
- `npm run build:full` - Build frontend + sync to static (one command)
- `npm run clean` - Remove build artifacts

### Phase 3: Update .gitignore

**File:** `.gitignore`

Ensure static/ directory is gitignored:

```gitignore
# Frontend build output
frontend/dist/
frontend/dist-ssr/
frontend/node_modules/

# Backend build output
target/

# Generated static directory (created by sync script)
static/

# Environment files
.env
.env.local

# IDE
.vscode/
.idea/

# OS
.DS_Store
Thumbs.db
```

### Phase 4: Update README.md Documentation

**File:** `README.md`

Add clear documentation for both workflows:

```markdown
## Development Setup

### Option A: Docker Compose (Recommended)

The easiest way to develop with Home Registry is using Docker Compose:

\`\`\`bash
# Clone and start
git clone https://github.com/VictoryTek/home-registry.git
cd home-registry
docker-compose up -d

# Access application
open http://localhost:8210
\`\`\`

**Pros:** No local setup needed, service worker works out of the box  
**Cons:** Slower rebuild times, container overhead

### Option B: Local Development (Without Docker)

For active frontend/backend development:

**Prerequisites:**
- Rust 1.85+
- Node.js 18+
- PostgreSQL 16+

**Setup Steps:**

1. **Start PostgreSQL:**
   \`\`\`bash
   # Using Docker (just database)
   docker run -d -p 5432:5432 \
     -e POSTGRES_USER=postgres \
     -e POSTGRES_PASSWORD=password \
     -e POSTGRES_DB=home_inventory \
     postgres:17
   
   # Or use your local PostgreSQL installation
   \`\`\`

2. **Configure Environment:**
   \`\`\`bash
   export DATABASE_URL="postgres://postgres:password@localhost:5432/home_inventory"
   export RUST_LOG=info
   \`\`\`

3. **Build Frontend:**
   \`\`\`bash
   cd frontend
   npm install
   npm run build:full  # Builds and syncs to ../static/
   cd ..
   \`\`\`

4. **Run Backend:**
   \`\`\`bash
   cargo run
   # Server starts on http://localhost:8210
   \`\`\`

5. **Access Application:**
   \`\`\`
   open http://localhost:8210
   \`\`\`

**Frontend Development Workflow:**

When making frontend changes:

\`\`\`bash
cd frontend

# Option 1: Full rebuild + sync
npm run build:full

# Option 2: Build then sync separately
npm run build
npm run sync

cd ..
cargo run  # Restart backend to serve new files
\`\`\`

**Backend Development Workflow:**

Backend changes don't require frontend rebuild:

\`\`\`bash
cargo run  # Just restart the backend
\`\`\`

**Troubleshooting:**

- **Service Worker 404 Error:** Run `cd frontend && npm run build:full` to sync files to static/
- **Stale Content:** Clear browser cache or use Incognito mode
- **Database Connection Error:** Verify DATABASE_URL and PostgreSQL is running

## Service Worker Development

The application uses VitePWA with Workbox for offline support. Key files:

- `frontend/vite.config.ts` - VitePWA configuration
- `static/sw.js` - Generated service worker (DO NOT EDIT)
- `static/workbox-{hash}.js` - Workbox runtime (DO NOT EDIT)

**Service worker is enabled in development** (`devOptions.enabled: true`). To disable:

1. Edit `frontend/vite.config.ts`
2. Set `devOptions.enabled: false`
3. Run `npm run build:full`
4. Restart backend

**To unregister service worker in browser:**
1. Open DevTools → Application → Service Workers
2. Click "Unregister" next to sw.js
3. Refresh page
```

### Phase 5: Add Cache Headers (Optional Enhancement)

**File:** `src/main.rs`

Add proper cache control headers for different file types:

```rust
use actix_web::http::header::{CacheControl, CacheDirective};

// Service Worker files - no caching (must always be fresh)
.route("/sw.js", web::get().to(|| async {
    let file = fs::NamedFile::open_async("static/sw.js").await?;
    Ok::<_, Error>(file
        .set_cache_control(CacheControl(vec![
            CacheDirective::Public,
            CacheDirective::MaxAge(0),
            CacheDirective::MustRevalidate,
        ]))
    )
}))

// Workbox files - aggressive caching (hashed filenames)
.route("/workbox-{filename:.*}.js", web::get().to(|path: web::Path<String>| async move {
    let filename = path.into_inner();
    let file = fs::NamedFile::open_async(format!("static/workbox-{filename}")).await?;
    Ok::<_, Error>(file
        .set_cache_control(CacheControl(vec![
            CacheDirective::Public,
            CacheDirective::MaxAge(31536000), // 1 year
            CacheDirective::Extension("immutable".to_owned(), None),
        ]))
    )
}))

// Manifest - no caching
.route("/manifest.webmanifest", web::get().to(|| async {
    let file = fs::NamedFile::open_async("static/manifest.webmanifest").await?;
    Ok::<_, Error>(file
        .set_cache_control(CacheControl(vec![
            CacheDirective::Public,
            CacheDirective::MaxAge(0),
            CacheDirective::MustRevalidate,
        ]))
    )
}))
```

**Rationale:**
- Service worker files: No caching (must check for updates)
- Workbox/hashed assets: Aggressive caching (filename changes = new content)
- Manifest: No caching (may contain dynamic data)

---

## Dependencies and Requirements

### New Dependencies

#### Frontend

**Package:** `fs-extra@^11.2.0`  
**Purpose:** File system operations for sync script  
**Why:** More robust than native fs module, better error handling

```bash
cd frontend
npm install --save-dev fs-extra
```

**Alternative:** Could use native `fs` module, but fs-extra provides:
- Promise-based API
- Better error messages
- Cross-platform compatibility
- Atomic operations

#### Backend

**No new dependencies required** - all file serving functionality already exists in actix-files.

### Current Dependency Versions (from Context7)

#### vite-plugin-pwa@0.21.1 (Currently Installed)

**Latest Version:** 0.21.1 ✅ (Already up to date)  
**Key Features:**
- Workbox 7.x integration
- TypeScript support
- Development mode support
- Framework-agnostic

**Configuration Best Practices:**
```typescript
VitePWA({
  registerType: 'autoUpdate',        // Automatic updates
  injectRegister: 'inline',          // Inline registration
  workbox: {
    globPatterns: ['**/*.{js,css,html,ico,png,svg,woff,woff2}'],
    runtimeCaching: [ /* ... */ ]    // Network strategies
  }
})
```

#### Workbox 7.x (via vite-plugin-pwa)

**Current:** Workbox 7.x (bundled with vite-plugin-pwa)  
**Key Features:**
- generateSW strategy (used by VitePWA)
- Runtime caching strategies
- Background sync
- Push notifications

**Caching Strategies Used:**
- `CacheFirst` - Fonts, external CDN resources
- `NetworkFirst` - API calls

#### Actix-Web 4.x (Currently Installed)

**Current:** Actix-Web 4.x (from Cargo.toml)  
**Key Features:**
- actix-files for static serving
- Pattern-based routing
- Async/await support
- Built-in MIME type detection

**Static File Serving Patterns:**
```rust
use actix_files as fs;

// Individual files
.route("/sw.js", web::get().to(|| async {
    fs::NamedFile::open_async("static/sw.js").await
}))

// Pattern matching
.route("/workbox-{filename:.*}.js", web::get().to(|path: web::Path<String>| async move {
    let filename = path.into_inner();
    fs::NamedFile::open_async(format!("static/workbox-{filename}")).await
}))

// Directory serving
.service(fs::Files::new("/assets", "static/assets"))
```

---

## Testing Plan

### Test 1: Docker Compose (Existing Workflow)

**Objective:** Verify Docker build still works as expected

```bash
# Clean build
docker-compose down -v
docker-compose build --no-cache
docker-compose up -d

# Verify
curl -I http://localhost:8210/sw.js
# Expected: 200 OK

curl -I http://localhost:8210/workbox-57649e2b.js
# Expected: 200 OK (or new hash)

# Browser test
open http://localhost:8210
# Check DevTools → Application → Service Workers
# Status should be "Activated and running"
```

**Success Criteria:**
- ✅ Container builds without errors
- ✅ Service worker files return 200 OK
- ✅ Service worker registers successfully in browser
- ✅ PWA manifest loads correctly
- ✅ Offline functionality works

### Test 2: Local Development (New Workflow)

**Objective:** Verify sync script and local development setup

```bash
# Start fresh
rm -rf static frontend/dist

# Build frontend
cd frontend
npm install
npm run build:full

# Verify static directory created
ls -la ../static/
# Expected: index.html, sw.js, workbox-*.js, manifest.webmanifest, assets/

# Start backend
cd ..
cargo run

# Test service worker files
curl -I http://localhost:8210/sw.js
# Expected: 200 OK

curl -I http://localhost:8210/workbox-57649e2b.js
# Expected: 200 OK

# Browser test
open http://localhost:8210
# Check DevTools → Console for any errors
# Check DevTools → Application → Service Workers
```

**Success Criteria:**
- ✅ Sync script completes without errors
- ✅ static/ directory created with all files
- ✅ Backend serves files from static/
- ✅ Service worker registers successfully
- ✅ No 404 errors in browser console

### Test 3: Frontend Change Workflow

**Objective:** Verify development workflow when making frontend changes

```bash
# Make a small change to frontend
echo "// Test comment" >> frontend/src/App.tsx

# Rebuild and sync
cd frontend
npm run build:full
cd ..

# Restart backend
cargo run

# Browser test
open http://localhost:8210
# Refresh and verify change appears
```

**Success Criteria:**
- ✅ Changes appear after rebuild
- ✅ Service worker updates correctly
- ✅ No stale content served

### Test 4: Cache Headers (If Implemented)

**Objective:** Verify proper cache control headers

```bash
# Service worker (no cache)
curl -I http://localhost:8210/sw.js
# Expected: Cache-Control: public, max-age=0, must-revalidate

# Workbox (aggressive cache)
curl -I http://localhost:8210/workbox-57649e2b.js
# Expected: Cache-Control: public, max-age=31536000, immutable

# Manifest (no cache)
curl -I http://localhost:8210/manifest.webmanifest
# Expected: Cache-Control: public, max-age=0, must-revalidate

# Assets (default Actix-files behavior)
curl -I http://localhost:8210/assets/index-BP9SvQAK.js
# Expected: appropriate cache headers from Actix
```

**Success Criteria:**
- ✅ Service worker files not cached
- ✅ Hashed files cached aggressively
- ✅ Manifest not cached

### Test 5: PWA Installation

**Objective:** Verify PWA can be installed on devices

```bash
# Desktop Chrome
open http://localhost:8210
# Look for "Install Home Registry" button in address bar
# Click and verify installation

# Mobile (if accessible)
# Open in mobile browser
# Check for "Add to Home Screen" prompt
```

**Success Criteria:**
- ✅ Install prompt appears
- ✅ PWA installs successfully
- ✅ Works offline after installation
- ✅ App icon displays correctly

---

## Potential Risks and Mitigations

### Risk 1: Build Script Compatibility

**Risk:** Node.js sync script may fail on different platforms (Windows, Linux, Mac)

**Impact:** Medium - Developers on affected platform can't use local development

**Mitigation:**
- ✅ Use `fs-extra` library (cross-platform)
- ✅ Use `path.join()` for platform-agnostic paths
- ✅ Test on Windows (PowerShell and WSL), macOS, and Linux
- ✅ Provide fallback: Manual copy instructions in README
- ✅ Add npm script that checks Node version compatibility

**Fallback Instructions:**
```bash
# Windows (PowerShell)
Copy-Item -Path frontend\dist\* -Destination static\ -Recurse -Force

# Linux/Mac
cp -r frontend/dist/* static/
cp frontend/public/logo_*.png static/
```

### Risk 2: Forgetting to Sync After Frontend Changes

**Risk:** Developer makes frontend changes but forgets to run sync, sees stale content

**Impact:** Low - Confusing but not breaking

**Mitigation:**
- ✅ Use `npm run build:full` (single command)
- ✅ Clear error messages in documentation
- ✅ Add note in frontend README
- ⚠️ Consider adding file watcher (advanced solution)

**Advanced Solution (Future):**
```json
{
  "scripts": {
    "dev:watch": "concurrently \"vite build --watch\" \"npm run watch:sync\"",
    "watch:sync": "nodemon --watch frontend/dist --exec npm run sync"
  }
}
```

### Risk 3: Service Worker Caching Issues During Development

**Risk:** Service worker caches old content, developer sees stale app

**Impact:** Medium - Confusing debugging experience

**Mitigation:**
- ✅ Document how to unregister service worker
- ✅ Document using Incognito mode for testing
- ✅ Consider disabling SW in development by default
- ✅ Add "Update on reload" checkbox instruction (DevTools)

**Documentation:**
```markdown
### Troubleshooting Stale Content

If you see old content after rebuilding:

1. Open DevTools (F12)
2. Navigate to Application tab
3. Service Workers section
4. Check "Update on reload"
5. Click "Unregister" to clear SW
6. Hard refresh (Ctrl+Shift+R or Cmd+Shift+R)
```

### Risk 4: Large Build Artifacts

**Risk:** Repeated builds fill up disk space with multiple copies of static/

**Impact:** Low - Disk space usage

**Mitigation:**
- ✅ Add `npm run clean` script
- ✅ Ensure static/ is gitignored
- ✅ Document cleanup process
- ✅ Sync script removes old static/ before copying

### Risk 5: Breaking Docker Build

**Risk:** Changes to directory structure break existing Docker build

**Impact:** Critical - Would break production deployment

**Mitigation:**
- ✅ **No changes to Dockerfile** - Docker build already works correctly
- ✅ **No changes to docker-compose.yml** - Already configured properly
- ✅ Test Docker build before committing
- ✅ Add CI check to ensure Docker build succeeds

**Docker Build Test:**
```bash
# Always test before committing
docker-compose build
docker-compose up -d
docker-compose logs app
# Verify no errors
```

### Risk 6: Workbox File Hash Changes

**Risk:** Workbox filename hash changes on each build, old routes 404

**Impact:** Low - Pattern matching handles this

**Mitigation:**
- ✅ Route pattern `/workbox-{filename:.*}.js` handles any hash
- ✅ Service worker references correct filename automatically
- ✅ No hardcoded filenames in backend

**Verification:**
```bash
# After build, check what file was generated
ls static/workbox-*.js

# Backend route will match regardless of hash
curl -I http://localhost:8210/workbox-ANYHASH.js
```

### Risk 7: fs-extra Dependency Bloat

**Risk:** Adding fs-extra increases node_modules size

**Impact:** Very Low - Acceptable tradeoff

**Mitigation:**
- ✅ fs-extra is small (~500KB)
- ✅ Only dev dependency (not in production bundle)
- ✅ Provides significant value (cross-platform robustness)
- ✅ Alternative: Use native fs if size is critical

---

## Success Metrics

### Immediate Success Criteria

- [x] Service worker files generate in frontend/dist/
- [ ] Sync script successfully copies files to static/
- [ ] Backend serves service worker files without 404
- [ ] Service worker registers in browser DevTools
- [ ] PWA manifest loads correctly
- [ ] No console errors related to service worker

### Developer Experience Metrics

- [ ] New developers can set up local environment in < 15 minutes
- [ ] Clear error messages when files are missing
- [ ] Documentation explains both Docker and local workflows
- [ ] Single command rebuilds frontend and syncs files

### Production Readiness Metrics

- [ ] Docker build completes successfully
- [ ] Service worker works in production deployment
- [ ] PWA passes Lighthouse audit (installable, offline)
- [ ] Proper cache headers configured
- [ ] No 404 errors in production logs

---

## Alternative Approaches Considered

### Alternative 1: Serve Frontend from Separate Server

**Approach:** Run Vite dev server (port 3000) and Rust backend (port 8210) separately, use proxy

**Pros:**
- Hot module reload (HMR) during development
- Faster frontend rebuild times
- Standard Vite development workflow

**Cons:**
- ❌ Service worker scope issues (different origins)
- ❌ CORS complications
- ❌ Doesn't match production architecture
- ❌ More complex setup for new developers

**Verdict:** Rejected - Breaks service worker, increases complexity

### Alternative 2: Backend Serves from frontend/dist/ in Development

**Approach:** Configure Rust backend to serve from different directory in dev vs prod

**Pros:**
- ✅ No file copying needed
- ✅ Always serves latest build

**Cons:**
- ❌ Requires environment-specific configuration
- ❌ Backend code diverges from production
- ❌ Mixing source and build artifacts
- ❌ Potential security issues (serving from frontend/ directory)

**Verdict:** Rejected - Creates dev/prod parity issues

### Alternative 3: Symbolic Link static/ → frontend/dist/

**Approach:** Create symlink so static/ points to frontend/dist/

**Pros:**
- ✅ No file copying
- ✅ Always in sync

**Cons:**
- ❌ Windows symlink support issues (requires admin)
- ❌ Git doesn't handle symlinks well
- ❌ Docker build breaks (can't copy symlink)
- ❌ Confusing for new developers

**Verdict:** Rejected - Windows compatibility issues

### Alternative 4: Disable Service Worker in Development

**Approach:** Set `devOptions.enabled: false`, only use SW in production

**Pros:**
- ✅ Simplifies development (no caching issues)
- ✅ Faster page loads
- ✅ No sync script needed

**Cons:**
- ❌ Can't test PWA features locally
- ❌ Offline functionality not testable
- ❌ Production bugs harder to catch

**Verdict:** Partial - Recommend as optional configuration, not default

### Alternative 5: Use Build Tool Directly (Cargo Build Script)

**Approach:** Add build.rs to Cargo to build frontend automatically

**Pros:**
- ✅ Single build command (`cargo build`)
- ✅ Integrated workflow

**Cons:**
- ❌ Requires Node.js installed wherever Cargo runs
- ❌ Slows down Rust compilation
- ❌ Mixing build systems
- ❌ CI/CD complications

**Verdict:** Rejected - Overly complex, tight coupling

---

## Chosen Solution: Sync Script

**Why this approach:**

1. ✅ **Simplicity:** Single npm script syncs files
2. ✅ **Cross-platform:** Node.js works everywhere
3. ✅ **Docker-compatible:** Doesn't affect existing Docker build
4. ✅ **Explicit:** Developer knows when sync happens
5. ✅ **Maintainable:** Easy to understand and modify
6. ✅ **Flexible:** Can be enhanced with watch mode later

**Trade-offs accepted:**
- ⚠️ Developer must remember to run sync after frontend changes
- ⚠️ Extra build step compared to HMR development server
- ⚠️ Duplication of files on disk (dist/ and static/)

**Why trade-offs are acceptable:**
- Docker workflow (recommended) already works without sync
- Local development is optional for those who prefer it
- Clear documentation reduces confusion
- `build:full` script makes it one command

---

## Documentation Requirements

### Files to Update

1. **README.md** - Add both development workflows, troubleshooting
2. **frontend/README.md** (NEW) - Frontend-specific development guide
3. **CONTRIBUTING.md** (if exists) - Add workflow for contributors
4. **.github/docs/** - Add service worker architecture doc

### Documentation Checklist

- [ ] Quick start for both Docker and local development
- [ ] Prerequisites clearly listed (Node, Rust, Postgres versions)
- [ ] Step-by-step setup instructions
- [ ] Common error messages and solutions
- [ ] Service worker development guide
- [ ] Build and deployment workflow
- [ ] Architecture diagram (Docker build flow)
- [ ] Troubleshooting section

---

## Deployment Considerations

### Docker Deployment (Production)

**Current State:** ✅ Already working correctly

```dockerfile
# Frontend build
RUN npm run build  # → frontend/dist/

# Copy to static
COPY --from=frontend-builder /app/frontend/dist ./static
```

**No changes needed** - sync script not used in Docker

### Local Production Build

**New Workflow:**
```bash
# Build frontend for production
cd frontend
npm run build:full

# Build backend for production
cd ..
cargo build --release

# Run production binary
./target/release/home-registry
```

### CI/CD Considerations

**GitHub Actions (if exists):**
- ✅ Frontend build step already exists
- ⚠️ May need to add sync step for artifacts
- ✅ Docker build unchanged

**Example CI addition:**
```yaml
- name: Build Frontend
  run: |
    cd frontend
    npm ci
    npm run build:full

- name: Upload Static Artifacts
  uses: actions/upload-artifact@v3
  with:
    name: static-files
    path: static/
```

---

## Future Enhancements (Out of Scope)

### 1. Watch Mode for Development

**Goal:** Automatically sync on frontend changes

```json
{
  "scripts": {
    "dev:watch": "concurrently \"vite build --watch\" \"nodemon --watch frontend/dist --exec npm run sync\""
  }
}
```

**Complexity:** Medium  
**Value:** High for active frontend development

### 2. Hybrid Development Server

**Goal:** Use Vite dev server for frontend, proxy API to Rust backend

```typescript
// vite.config.ts
server: {
  port: 3000,
  proxy: {
    '/api': 'http://localhost:8210'
  }
}
```

**Complexity:** High (service worker scope issues)  
**Value:** Medium (faster HMR)

### 3. Automated Cache Invalidation

**Goal:** Service worker auto-updates when backend restarts

**Complexity:** High  
**Value:** Low (autoUpdate already handles this)

### 4. Development vs Production Service Worker

**Goal:** Different SW strategies for dev vs prod

**Complexity:** Medium  
**Value:** Medium (better dev experience)

---

## Timeline & Effort Estimate

### Phase 1: Core Implementation (High Priority)
- [ ] Create sync script - **1 hour**
- [ ] Update package.json - **15 minutes**
- [ ] Test sync script on Windows/Linux/Mac - **1 hour**
- [ ] Update .gitignore - **5 minutes**
- **Total: ~2.5 hours**

### Phase 2: Documentation (High Priority)
- [ ] Update README.md - **1 hour**
- [ ] Create frontend/README.md - **30 minutes**
- [ ] Add troubleshooting section - **30 minutes**
- **Total: ~2 hours**

### Phase 3: Testing (High Priority)
- [ ] Test Docker build - **30 minutes**
- [ ] Test local development workflow - **1 hour**
- [ ] Test on different platforms - **1 hour**
- **Total: ~2.5 hours**

### Phase 4: Optional Enhancements (Low Priority)
- [ ] Add cache headers - **1 hour**
- [ ] Add npm clean script - **15 minutes**
- [ ] Create architecture diagram - **1 hour**
- **Total: ~2 hours**

**Grand Total: 9-11 hours** (Phases 1-3 are ~7 hours, Phase 4 optional)

---

## Conclusion

The service worker 404 error is a **directory mismatch issue** between where VitePWA generates files (frontend/dist/) and where the Rust backend expects to serve them (static/). This mismatch exists only in local development; the Docker build already handles it correctly.

**Recommended Solution:** Implement a sync script that copies frontend/dist/ → static/ for local development, while maintaining the existing Docker workflow unchanged. This provides both:
1. ✅ Working Docker Compose setup (already functional)
2. ✅ Working local development setup (new capability)

The sync script approach is simple, cross-platform, maintainable, and doesn't affect production deployment. Combined with clear documentation, it provides a great developer experience for both workflows.

**Implementation Priority:**
1. **HIGH:** Sync script + package.json updates (enables local dev)
2. **HIGH:** README documentation (helps developers understand workflows)
3. **HIGH:** Testing on multiple platforms (ensures it works for everyone)
4. **LOW:** Cache headers (nice-to-have optimization)

---

## Next Steps

1. **Review this specification** - Ensure all stakeholders agree on approach
2. **Create implementation subagent** - Build sync script and update files
3. **Test thoroughly** - Verify on Windows, Linux, and macOS
4. **Update documentation** - Clear instructions for both workflows
5. **Deploy and monitor** - Ensure no regressions in existing Docker workflow

**Questions for Review:**
- Is the sync script approach acceptable, or prefer alternative?
- Should service worker be disabled in development by default?
- Any additional file types that need special handling?
- Docker-first vs local-first development priority?

---

**Specification Complete** ✅  
**Document Location:** `.github/docs/SubAgent docs/service_worker_fix_spec.md`  
**Ready for Implementation:** Yes
