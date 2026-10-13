# Backup & Restore Feature Specification

**Date:** 2026-02-15  
**Feature:** Full Backup & Restore System for Home Registry  
**Status:** Draft Specification  
**Author:** Research Subagent

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current State Analysis](#2-current-state-analysis)
3. [Reference Implementation Analysis (Humidor)](#3-reference-implementation-analysis-humidor)
4. [Research & Best Practices](#4-research--best-practices)
5. [Architecture Design](#5-architecture-design)
6. [Backend Specification (Rust)](#6-backend-specification-rust)
7. [Frontend Specification (TypeScript/React)](#7-frontend-specification-typescriptreact)
8. [Data Format Specification](#8-data-format-specification)
9. [Safety & Error Handling](#9-safety--error-handling)
10. [Implementation Plan](#10-implementation-plan)
11. [Dependencies](#11-dependencies)
12. [Testing Strategy](#12-testing-strategy)
13. [Risks & Mitigations](#13-risks--mitigations)

---

## 1. Executive Summary

This specification defines a comprehensive Backup & Restore feature for the Home Registry application. The feature allows users to:

- **Create** full backups of all their data (inventories, items, organizers, settings) as JSON files
- **List** available server-side backups with metadata (date, size)
- **Download** backups to local machine
- **Upload** backup files from local machine to server
- **Restore** from either server-side or uploaded backups
- **Delete** server-side backups
- **Auto-backup** before any restore operation (safety net)

The implementation follows patterns established in the Humidor project (a sibling project with a working backup/restore system), adapted for the Home Registry's Actix-Web architecture, React/TypeScript frontend, and multi-user authentication model.

---

## 2. Current State Analysis

### 2.1 Database Tables (Data to Back Up)

The Home Registry database contains the following tables that must be included in backups:

| Table | Description | Dependencies |
|-------|-------------|-------------|
| `users` | User accounts (id, username, full_name, password_hash, is_admin, is_active) | None |
| `inventories` | Inventory collections (name, description, location, image_url, user_id) | users |
| `items` | Individual items in inventories | inventories |
| `categories` | Category definitions (name, description, color, icon) | None |
| `tags` | Tag definitions (name, color) | None |
| `item_tags` | Many-to-many relationship between items and tags | items, tags |
| `custom_fields` | Custom field definitions per category | categories |
| `item_custom_values` | Custom field values for items | items, custom_fields |
| `organizer_types` | Organizer type definitions per inventory (select/text) | inventories |
| `organizer_options` | Predefined options for select-type organizers | organizer_types |
| `item_organizer_values` | Item-to-organizer-value mappings | items, organizer_types, organizer_options |
| `user_settings` | User preferences (theme, currency, date_format, etc.) | users |
| `inventory_shares` | Per-inventory sharing records | inventories, users |
| `user_access_grants` | All-Access tier grants between users | users |
| `recovery_codes` | Account recovery codes (hashed) | users |
| `password_reset_tokens` | Password reset tokens | users |

### 2.2 Existing Backend Architecture

- **Web Framework:** Actix-Web 4.x with `#[get]`, `#[post]`, `#[put]`, `#[delete]` macros
- **Database:** PostgreSQL 16 via `deadpool-postgres` connection pool
- **API Pattern:** All routes registered in `api_scope()` function in `src/api/mod.rs`
- **Response Pattern:** `ApiResponse<T>` for success, `ErrorResponse` for failures
- **Auth Pattern:** `auth::get_auth_context_from_request()` extracts JWT auth from requests
- **Service Pattern:** `DatabaseService::new(pool)` wraps all DB operations
- **Dependencies:** Already has `serde`, `serde_json`, `chrono`, `tokio`, `uuid` pinned

### 2.3 Existing Frontend Architecture

- **Framework:** React with TypeScript, Vite build system
- **API Service:** Centralized in `frontend/src/services/api.ts` with `fetchWithRetry()` utility
- **Types:** All interfaces defined in `frontend/src/types/index.ts`
- **Components:** Reusable components including `ConfirmModal`, `Toast`, `Modal`, `Header`
- **Settings Page:** `SettingsPage.tsx` has sectioned layout with icons, uses `useAuth()` and `useApp()` contexts
- **Styling:** CSS classes follow `.settings-section`, `.settings-section-header`, `.setting-item` pattern

### 2.4 Auth Context

All backup/restore endpoints must require authentication. The existing pattern:
```rust
let auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
    Ok(a) => a,
    Err(e) => return Ok(e),
};
```
The `auth` object provides `auth.user_id` (UUID) and `auth.username` (String).

**Admin-only restriction:** Backup/restore operations should be restricted to **admin users only** (`auth.is_admin == true`) since they affect all data across all users.

---

## 3. Reference Implementation Analysis (Humidor)

### 3.1 Humidor Architecture Overview

The Humidor project implements backup/restore using the Warp framework (different from Home Registry's Actix-Web) but provides excellent patterns to adapt:

**Backend Structure:**
- `src/handlers/backups.rs` — HTTP handler functions (list, create, download, delete, restore, upload)
- `src/services/backup.rs` — Core backup logic (create_backup, restore_backup, list_backups, delete_backup, export_database, import_database)
- `src/routes/backups.rs` — Route definitions with auth middleware

**Key Design Decisions from Humidor:**
1. **ZIP format** — Backups stored as `.zip` files containing `metadata.json` + `database.json` + `uploads/` directory
2. **JSON database export** — Uses PostgreSQL's `json_agg(row_to_json(t))` for lossless table export
3. **JSON import** — Uses `json_populate_record(NULL::table_name, $1::json)` for importing rows
4. **Timestamped filenames** — Format: `humidor_YYYY.MM.DD.HH.MM.SS.zip`
5. **Directory-based storage** — All backups stored in `backups/` directory
6. **Metadata file** — Each backup includes version, created_at, database_type
7. **Security checks** — Path traversal prevention, file extension validation
8. **Transaction-like restore** — Truncates all tables first, then imports in dependency order

### 3.2 Humidor Frontend Patterns

The Humidor UI places the Backup & Restore section within the Settings page:
- **Create Backup** button with loading spinner animation
- **Upload Backup** button (file input, `.zip` only)
- **Backups Table** — Lists name, date, size, and action buttons (download, restore, delete)
- **Restore Confirmation Modal** — Warning text, checkbox confirmation ("I understand this is irreversible"), two buttons (Cancel / Restore)
- **Delete Confirmation Modal** — Simple confirmation dialog

### 3.3 Adaptation Notes

| Humidor Pattern | Home Registry Adaptation |
|----------------|--------------------------|
| Warp framework | Actix-Web 4.x with macros |
| ZIP with uploads dir | JSON-only (no file uploads in Home Registry currently) |
| `json_agg(row_to_json(t))` | Same approach — works with tokio-postgres |
| `json_populate_record()` | Same approach — PostgreSQL 16 supports this |
| `tracing::error!` logging | `log::error!` (Home Registry uses `log` crate) |
| No auto-backup before restore | **Add** auto-backup before restore (safety improvement) |
| No admin-only restriction | **Add** admin-only check for backup/restore endpoints |
| Single-user focused | Multi-user: backup includes ALL users' data |

---

## 4. Research & Best Practices

### 4.1 Actix-Web Multipart File Upload (from Context7)

Actix-Web provides `actix-multipart` for file upload handling. The recommended approach for Home Registry:

```rust
use actix_multipart::form::{tempfile::TempFile, MultipartForm};

#[derive(MultipartForm)]
struct BackupUploadForm {
    #[multipart(limit = "100MB")]
    file: TempFile,
}

#[post("/backup/upload")]
async fn upload_backup(
    MultipartForm(form): MultipartForm<BackupUploadForm>,
    pool: web::Data<Pool>,
    req: HttpRequest,
) -> Result<impl Responder> {
    // Process the uploaded file
}
```

**Note:** `actix-multipart` is a separate crate that needs to be added as a dependency.

### 4.2 File Download Pattern

For downloading backup files, use `actix_files::NamedFile`:

```rust
use actix_files::NamedFile;

#[get("/backup/download/{filename}")]
async fn download_backup(
    path: web::Path<String>,
) -> actix_web::Result<NamedFile> {
    let filename = path.into_inner();
    let filepath = format!("backups/{}", filename);
    Ok(NamedFile::open(filepath)?
        .set_content_disposition(actix_web::http::header::ContentDisposition {
            disposition: actix_web::http::header::DispositionType::Attachment,
            parameters: vec![
                actix_web::http::header::DispositionParam::Filename(filename)
            ],
        }))
}
```

### 4.3 Serde JSON Serialization

The project already uses `serde` and `serde_json` with pinned versions. The backup data will be serialized/deserialized using standard serde patterns with `#[derive(Serialize, Deserialize)]`.

### 4.4 Chrono Timestamps

Already in use. Backup filenames and metadata will use:
```rust
let timestamp = chrono::Utc::now().format("%Y.%m.%d.%H.%M.%S").to_string();
```

### 4.5 Tokio Async File Operations

Use `tokio::fs` for non-blocking file I/O:
```rust
tokio::fs::write(&path, &data).await?;
tokio::fs::read(&path).await?;
tokio::fs::remove_file(&path).await?;
tokio::fs::create_dir_all("backups").await?;
```

### 4.6 React File Upload/Download Patterns

- **Upload:** Use `<input type="file" accept=".json">` with `FormData` for multipart upload
- **Download:** Use `fetch()` → `response.blob()` → `URL.createObjectURL()` → programmatic `<a>` click
- **Progress:** Use button state management (disabled + spinner text)

---

## 5. Architecture Design

### 5.1 System Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Frontend (React)                      │
│                                                          │
│  SettingsPage.tsx                                        │
│    └── BackupRestoreSection (new component)              │
│         ├── Create Backup button                         │
│         ├── Upload Backup button                         │
│         ├── Backups Table (list, download, restore, del) │
│         └── Restore Confirmation Modal                   │
│                                                          │
│  services/api.ts                                         │
│    └── backupApi (new API service object)                │
│                                                          │
│  types/index.ts                                          │
│    └── BackupInfo, BackupMetadata interfaces             │
└────────────────────────┬────────────────────────────────┘
                         │ HTTP/JSON
                         ▼
┌─────────────────────────────────────────────────────────┐
│                    Backend (Rust/Actix-Web)               │
│                                                          │
│  src/api/mod.rs                                          │
│    └── Backup endpoints registered in api_scope()        │
│         POST   /api/backup/create                        │
│         GET    /api/backup/list                           │
│         GET    /api/backup/download/{filename}            │
│         POST   /api/backup/upload                        │
│         POST   /api/backup/restore/{filename}            │
│         DELETE /api/backup/{filename}                    │
│                                                          │
│  src/api/backup.rs (NEW)                                 │
│    └── Handler functions for backup endpoints            │
│                                                          │
│  src/db/mod.rs                                           │
│    └── DatabaseService methods:                          │
│         export_all_data()                                │
│         import_all_data()                                │
│                                                          │
│  src/models/mod.rs                                       │
│    └── BackupInfo, BackupMetadata, BackupData structs    │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│                    File System                           │
│                                                          │
│  backups/                                                │
│    ├── home_registry_2026.02.15.10.30.00.json            │
│    ├── home_registry_2026.02.14.08.15.22.json            │
│    └── home_registry_auto_pre_restore_2026.02.15.json    │
└─────────────────────────────────────────────────────────┘
```

### 5.2 Design Decisions

1. **JSON format (not ZIP):** Unlike Humidor which uses ZIP to include uploaded images, Home Registry does not currently have file uploads (images are URL references). JSON is simpler, human-readable, and sufficient.

2. **Admin-only access:** Since backup/restore affects ALL data across ALL users, these operations are restricted to admin users.

3. **Auto-backup before restore:** Every restore operation automatically creates a backup first, providing a safety net.

4. **Separate backup module:** Create `src/api/backup.rs` as a new module to keep backup logic organized and separate from the main API handlers.

5. **User-scoped vs Global backups:** Backups will be global (all data) since this is an admin-only feature. This matches the Humidor pattern and is simpler than per-user backups.

6. **Transaction-based restore:** Use PostgreSQL transaction to ensure atomic restore (all-or-nothing).

---

## 6. Backend Specification (Rust)

### 6.1 New File: `src/api/backup.rs`

This module contains all backup-related HTTP handler functions.

#### 6.1.1 Create Backup — `POST /api/backup/create`

**Auth:** Required (admin only)

```rust
#[post("/backup/create")]
pub async fn create_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
) -> Result<impl Responder> {
    // 1. Authenticate and verify admin
    // 2. Create backups/ directory if not exists
    // 3. Generate timestamped filename: home_registry_YYYY.MM.DD.HH.MM.SS.json
    // 4. Export all database tables to JSON via DatabaseService::export_all_data()
    // 5. Wrap in BackupData envelope with metadata
    // 6. Write JSON file to backups/ directory
    // 7. Return ApiResponse with BackupInfo
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "name": "home_registry_2026.02.15.10.30.00.json",
    "date": "2026-02-15T10:30:00Z",
    "size": "2.45 MB"
  },
  "message": "Backup created successfully"
}
```

#### 6.1.2 List Backups — `GET /api/backup/list`

**Auth:** Required (admin only)

```rust
#[get("/backup/list")]
pub async fn list_backups(
    pool: web::Data<Pool>,
    req: HttpRequest,
) -> Result<impl Responder> {
    // 1. Authenticate and verify admin
    // 2. Read backups/ directory
    // 3. For each .json file: extract name, modified date, file size
    // 4. Sort by date (newest first)
    // 5. Return ApiResponse with Vec<BackupInfo>
}
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "name": "home_registry_2026.02.15.10.30.00.json",
      "date": "2026-02-15T10:30:00Z",
      "size": "2.45 MB"
    },
    {
      "name": "home_registry_2026.02.14.08.15.22.json",
      "date": "2026-02-14T08:15:22Z",
      "size": "2.43 MB"
    }
  ],
  "message": "Retrieved 2 backups"
}
```

#### 6.1.3 Download Backup — `GET /api/backup/download/{filename}`

**Auth:** Required (admin only)

```rust
#[get("/backup/download/{filename}")]
pub async fn download_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<impl Responder> {
    // 1. Authenticate and verify admin
    // 2. Validate filename (no path traversal)
    // 3. Construct path: backups/{filename}
    // 4. Security check: path must start with backups/ and file must exist
    // 5. Read file and return as attachment with Content-Disposition header
    // 6. Content-Type: application/json
}
```

**Response:** Raw file download with headers:
```
Content-Type: application/json
Content-Disposition: attachment; filename="home_registry_2026.02.15.10.30.00.json"
```

#### 6.1.4 Upload Backup — `POST /api/backup/upload`

**Auth:** Required (admin only)

```rust
#[post("/backup/upload")]
pub async fn upload_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    MultipartForm(form): MultipartForm<BackupUploadForm>,
) -> Result<impl Responder> {
    // 1. Authenticate and verify admin
    // 2. Validate file extension (.json only)
    // 3. Read file content from TempFile
    // 4. Validate JSON structure (must be valid BackupData format)
    // 5. Create backups/ directory if not exists
    // 6. Save file to backups/ directory with original filename
    // 7. Security check: path must be within backups/ directory
    // 8. Return ApiResponse with success message
}
```

**Request:** Multipart form with `file` field containing `.json` backup file.

**Response:**
```json
{
  "success": true,
  "data": {
    "name": "home_registry_2026.02.15.10.30.00.json",
    "date": "2026-02-15T10:30:00Z",
    "size": "2.45 MB"
  },
  "message": "Backup uploaded successfully"
}
```

#### 6.1.5 Restore Backup — `POST /api/backup/restore/{filename}`

**Auth:** Required (admin only)

```rust
#[post("/backup/restore/{filename}")]
pub async fn restore_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<impl Responder> {
    // 1. Authenticate and verify admin
    // 2. Validate filename and verify file exists
    // 3. AUTO-BACKUP: Create automatic backup before restore
    //    - Filename: home_registry_auto_pre_restore_YYYY.MM.DD.HH.MM.SS.json
    // 4. Read and parse the backup JSON file
    // 5. Validate backup format (check version, required fields)
    // 6. Begin database transaction
    // 7. Disable foreign key constraints: SET CONSTRAINTS ALL DEFERRED
    // 8. Truncate all tables in reverse dependency order
    // 9. Import all data in dependency order
    // 10. Commit transaction (or rollback on error)
    // 11. Return ApiResponse with success message
}
```

**Response:**
```json
{
  "success": true,
  "data": null,
  "message": "Backup restored successfully. A pre-restore backup was created: home_registry_auto_pre_restore_2026.02.15.10.35.00.json"
}
```

#### 6.1.6 Delete Backup — `DELETE /api/backup/{filename}`

**Auth:** Required (admin only)

```rust
#[delete("/backup/{filename}")]
pub async fn delete_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<impl Responder> {
    // 1. Authenticate and verify admin
    // 2. Validate filename (no path traversal)
    // 3. Security check: path must be within backups/ directory
    // 4. Verify file exists
    // 5. Delete the file
    // 6. Return ApiResponse with success message
}
```

**Response:**
```json
{
  "success": true,
  "data": null,
  "message": "Backup home_registry_2026.02.15.10.30.00.json deleted successfully"
}
```

### 6.2 New Models: `src/models/mod.rs` Additions

```rust
// ==================== Backup & Restore Models ====================

/// Metadata about a backup file (for listing)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackupInfo {
    pub name: String,
    pub date: String,
    pub size: String,
}

/// Metadata embedded in the backup file itself
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackupMetadata {
    pub version: String,           // Backup format version (e.g., "1.0")
    pub app_version: String,       // Home Registry app version
    pub created_at: String,        // ISO 8601 timestamp
    pub database_type: String,     // "postgresql"
    pub description: Option<String>, // Optional user-provided description
}

/// The complete backup data envelope
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackupData {
    pub metadata: BackupMetadata,
    pub data: BackupDatabaseContent,
}

/// All database tables exported as JSON arrays
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackupDatabaseContent {
    pub users: serde_json::Value,
    pub inventories: serde_json::Value,
    pub items: serde_json::Value,
    pub categories: serde_json::Value,
    pub tags: serde_json::Value,
    pub item_tags: serde_json::Value,
    pub custom_fields: serde_json::Value,
    pub item_custom_values: serde_json::Value,
    pub organizer_types: serde_json::Value,
    pub organizer_options: serde_json::Value,
    pub item_organizer_values: serde_json::Value,
    pub user_settings: serde_json::Value,
    pub inventory_shares: serde_json::Value,
    pub user_access_grants: serde_json::Value,
    pub recovery_codes: serde_json::Value,
}

/// Multipart form for backup upload
#[derive(MultipartForm)]
pub struct BackupUploadForm {
    #[multipart(limit = "100MB")]
    pub file: TempFile,
}
```

### 6.3 New `DatabaseService` Methods: `src/db/mod.rs` Additions

#### 6.3.1 `export_all_data()`

```rust
/// Export all database tables as JSON values for backup
pub async fn export_all_data(&self) -> Result<BackupDatabaseContent, Box<dyn std::error::Error>> {
    let client = self.pool.get().await?;
    
    let tables = vec![
        ("users", "users"),
        ("inventories", "inventories"),
        ("items", "items"),
        ("categories", "categories"),
        ("tags", "tags"),
        ("item_tags", "item_tags"),
        ("custom_fields", "custom_fields"),
        ("item_custom_values", "item_custom_values"),
        ("organizer_types", "organizer_types"),
        ("organizer_options", "organizer_options"),
        ("item_organizer_values", "item_organizer_values"),
        ("user_settings", "user_settings"),
        ("inventory_shares", "inventory_shares"),
        ("user_access_grants", "user_access_grants"),
        ("recovery_codes", "recovery_codes"),
    ];
    
    // For each table, use json_agg(row_to_json(t)) to export as JSON
    // SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)::text FROM {table} t
    
    // Construct BackupDatabaseContent with each table's JSON array
}
```

#### 6.3.2 `import_all_data()`

```rust
/// Import all database tables from backup data (within a transaction)
pub async fn import_all_data(
    &self,
    data: &BackupDatabaseContent,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = self.pool.get().await?;
    let transaction = client.transaction().await?;
    
    // Defer foreign key constraints
    transaction.execute("SET CONSTRAINTS ALL DEFERRED", &[]).await?;
    
    // Truncate tables in reverse dependency order
    let truncate_order = vec![
        "recovery_codes",
        "user_access_grants",
        "inventory_shares",
        "user_settings",
        "item_organizer_values",
        "organizer_options",
        "organizer_types",
        "item_custom_values",
        "custom_fields",
        "item_tags",
        "tags",
        "categories",
        "items",
        "inventories",
        "users",
    ];
    
    for table in &truncate_order {
        transaction
            .execute(&format!("TRUNCATE TABLE {} RESTART IDENTITY CASCADE", table), &[])
            .await?;
    }
    
    // Import tables in dependency order (reverse of truncate)
    let import_order = vec![
        ("users", &data.users),
        ("inventories", &data.inventories),
        ("items", &data.items),
        ("categories", &data.categories),
        ("tags", &data.tags),
        ("item_tags", &data.item_tags),
        ("custom_fields", &data.custom_fields),
        ("item_custom_values", &data.item_custom_values),
        ("organizer_types", &data.organizer_types),
        ("organizer_options", &data.organizer_options),
        ("item_organizer_values", &data.item_organizer_values),
        ("user_settings", &data.user_settings),
        ("inventory_shares", &data.inventory_shares),
        ("user_access_grants", &data.user_access_grants),
        ("recovery_codes", &data.recovery_codes),
    ];
    
    for (table, rows_json) in import_order {
        if let Some(rows) = rows_json.as_array() {
            for row in rows {
                // Use json_populate_record for type-safe import
                let query = format!(
                    "INSERT INTO {} SELECT * FROM json_populate_record(NULL::{}, $1::json)",
                    table, table
                );
                transaction.execute(&query, &[&row]).await?;
            }
        }
    }
    
    // Reset all sequences to max(id) + 1 for tables with serial/identity columns
    let sequence_tables = vec![
        "items", "inventories", "categories", "tags",
        "custom_fields", "item_custom_values", "item_tags",
        "organizer_types", "organizer_options", "item_organizer_values",
    ];
    
    for table in sequence_tables {
        let query = format!(
            "SELECT setval(pg_get_serial_sequence('{}', 'id'), COALESCE(MAX(id), 0) + 1, false) FROM {}",
            table, table
        );
        // Ignore errors for tables that may not have sequences
        let _ = transaction.execute(&query, &[]).await;
    }
    
    transaction.commit().await?;
    Ok(())
}
```

### 6.4 Route Registration

In `src/api/mod.rs`, add to `api_scope()`:

```rust
pub fn api_scope() -> Scope {
    web::scope("/api")
        // ... existing routes ...
        // Backup & Restore routes
        .service(backup::create_backup)
        .service(backup::list_backups)
        .service(backup::download_backup)
        .service(backup::upload_backup)
        .service(backup::restore_backup)
        .service(backup::delete_backup)
        // ...
}
```

In `src/api/mod.rs`, add module declaration:
```rust
pub mod auth;
pub mod backup;  // NEW
```

### 6.5 Helper Functions

```rust
/// Validate backup filename to prevent path traversal attacks
fn validate_backup_filename(filename: &str) -> Result<(), String> {
    // Must end with .json
    if !filename.ends_with(".json") {
        return Err("Only .json backup files are allowed".to_string());
    }
    // Must not contain path separators or parent directory references
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err("Invalid filename".to_string());
    }
    // Must start with expected prefix
    if !filename.starts_with("home_registry_") {
        return Err("Invalid backup filename format".to_string());
    }
    Ok(())
}

/// Format file size in human-readable format
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Generate a timestamped backup filename
fn generate_backup_filename(prefix: &str) -> String {
    let timestamp = chrono::Utc::now().format("%Y.%m.%d.%H.%M.%S").to_string();
    format!("{}_{}.json", prefix, timestamp)
}
```

### 6.6 Admin Authorization Check

Create a reusable helper for admin-only endpoints:

```rust
/// Check if the authenticated user is an admin
/// Returns an error response if not admin
fn require_admin(auth: &AuthContext) -> Result<(), HttpResponse> {
    if !auth.is_admin {
        Err(HttpResponse::Forbidden().json(ErrorResponse {
            success: false,
            error: "Admin access required".to_string(),
            message: Some("Only administrators can manage backups".to_string()),
        }))
    } else {
        Ok(())
    }
}
```

### 6.7 New Dependency

Add to `Cargo.toml`:

```toml
actix-multipart = "=0.7.2"  # For file upload handling
```

**Note:** `actix-multipart` is MIT/Apache-2.0 licensed, compatible with the project's license policy.

---

## 7. Frontend Specification (TypeScript/React)

### 7.1 New Types: `frontend/src/types/index.ts` Additions

```typescript
// ==================== Backup & Restore Types ====================

export interface BackupInfo {
  name: string;
  date: string;
  size: string;
}

export interface BackupMetadata {
  version: string;
  app_version: string;
  created_at: string;
  database_type: string;
  description?: string;
}
```

### 7.2 New API Service: `frontend/src/services/api.ts` Additions

Add a new `backupApi` object following the existing pattern:

```typescript
// ==================== Backup & Restore API ====================

export const backupApi = {
  // Create a new backup
  async create(): Promise<ApiResponse<BackupInfo>> {
    const response = await fetchWithRetry(`${API_BASE}/backup/create`, {
      method: 'POST',
      headers: getHeaders(),
    });
    return handleResponse<BackupInfo>(response);
  },

  // List all available backups
  async list(): Promise<ApiResponse<BackupInfo[]>> {
    const response = await fetchWithRetry(`${API_BASE}/backup/list`, {
      headers: getHeaders(),
    });
    return handleResponse<BackupInfo[]>(response);
  },

  // Download a backup file
  async download(filename: string): Promise<void> {
    const token = getToken();
    const response = await fetch(`${API_BASE}/backup/download/${encodeURIComponent(filename)}`, {
      headers: {
        'Authorization': `Bearer ${token}`,
      },
    });
    
    if (!response.ok) {
      throw new Error('Download failed');
    }
    
    const blob = await response.blob();
    const link = document.createElement('a');
    link.href = URL.createObjectURL(blob);
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(link.href);
  },

  // Upload a backup file
  async upload(file: File): Promise<ApiResponse<BackupInfo>> {
    const token = getToken();
    const formData = new FormData();
    formData.append('file', file);
    
    const response = await fetch(`${API_BASE}/backup/upload`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${token}`,
      },
      body: formData,
    });
    return handleResponse<BackupInfo>(response);
  },

  // Restore from a backup
  async restore(filename: string): Promise<ApiResponse<null>> {
    const response = await fetchWithRetry(`${API_BASE}/backup/restore/${encodeURIComponent(filename)}`, {
      method: 'POST',
      headers: getHeaders(),
    });
    return handleResponse<null>(response);
  },

  // Delete a backup
  async delete(filename: string): Promise<ApiResponse<null>> {
    const response = await fetchWithRetry(`${API_BASE}/backup/${encodeURIComponent(filename)}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    return handleResponse<null>(response);
  },
};
```

**Note:** For `upload()`, do NOT set `Content-Type` header — the browser will automatically set it to `multipart/form-data` with the correct boundary. For `download()`, don't use `fetchWithRetry` since we need the raw blob response.

### 7.3 New Component: `frontend/src/components/BackupRestoreSection.tsx`

This component is integrated into the SettingsPage as a new section (admin only).

```typescript
import { useState, useEffect, useCallback } from 'react';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { backupApi } from '@/services/api';
import { ConfirmModal } from '@/components';
import type { BackupInfo } from '@/types';

export function BackupRestoreSection() {
  const { showToast } = useApp();
  const { user } = useAuth();
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isCreating, setIsCreating] = useState(false);
  const [isRestoring, setIsRestoring] = useState(false);
  const [restoreTarget, setRestoreTarget] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  // Load backups on mount
  const loadBackups = useCallback(async () => { /* ... */ }, []);
  
  // Create backup handler
  const handleCreateBackup = async () => { /* ... */ };
  
  // Upload backup handler
  const handleUploadBackup = async (e: React.ChangeEvent<HTMLInputElement>) => { /* ... */ };
  
  // Download backup handler
  const handleDownloadBackup = async (filename: string) => { /* ... */ };
  
  // Restore handlers
  const handleRestoreConfirm = async () => { /* ... */ };
  
  // Delete handlers
  const handleDeleteConfirm = async () => { /* ... */ };

  // Only show for admin users
  if (!user?.is_admin) {
    return null;
  }

  return (
    <>
      {/* Backup Actions */}
      <div className="backup-actions" style={{ display: 'flex', gap: '0.75rem', marginBottom: '1rem' }}>
        <button className="btn btn-primary" onClick={handleCreateBackup} disabled={isCreating}>
          {isCreating ? (
            <><span className="spinner-small"></span> Creating...</>
          ) : (
            <><i className="fas fa-plus-circle"></i> Create Backup</>
          )}
        </button>
        <label className="btn btn-secondary" style={{ cursor: 'pointer' }}>
          <i className="fas fa-upload"></i> Upload Backup
          <input
            type="file"
            accept=".json"
            style={{ display: 'none' }}
            onChange={handleUploadBackup}
          />
        </label>
      </div>

      {/* Backups Table */}
      <div className="backups-table-container">
        <table className="settings-table">
          <thead>
            <tr>
              <th>Backup Name</th>
              <th>Date Created</th>
              <th>Size</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {isLoading ? (
              <tr><td colSpan={4}>Loading backups...</td></tr>
            ) : backups.length === 0 ? (
              <tr><td colSpan={4}>No backups found. Create your first backup above.</td></tr>
            ) : (
              backups.map((backup) => (
                <tr key={backup.name}>
                  <td>{backup.name}</td>
                  <td>{formatBackupDate(backup.date)}</td>
                  <td>{backup.size}</td>
                  <td>
                    <button className="btn-icon" onClick={() => handleDownloadBackup(backup.name)} title="Download">
                      <i className="fas fa-download"></i>
                    </button>
                    <button className="btn-icon" onClick={() => setRestoreTarget(backup.name)} title="Restore">
                      <i className="fas fa-database"></i>
                    </button>
                    <button className="btn-icon btn-danger-icon" onClick={() => setDeleteTarget(backup.name)} title="Delete">
                      <i className="fas fa-trash"></i>
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {/* Restore Confirmation Modal */}
      <ConfirmModal
        isOpen={restoreTarget !== null}
        onClose={() => setRestoreTarget(null)}
        onConfirm={handleRestoreConfirm}
        title="Confirm Backup Restore"
        message={`This will replace ALL current data with the backup "${restoreTarget}". An automatic backup will be created first. This action is irreversible.`}
        confirmText="Restore Backup"
        confirmButtonClass="btn-danger"
        icon="fas fa-exclamation-triangle"
      />

      {/* Delete Confirmation Modal */}
      <ConfirmModal
        isOpen={deleteTarget !== null}
        onClose={() => setDeleteTarget(null)}
        onConfirm={handleDeleteConfirm}
        title="Delete Backup"
        message={`Are you sure you want to delete backup "${deleteTarget}"? This cannot be undone.`}
        confirmText="Delete"
        confirmButtonClass="btn-danger"
        icon="fas fa-trash"
      />
    </>
  );
}

function formatBackupDate(dateString: string): string {
  const date = new Date(dateString);
  return date.toLocaleString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}
```

### 7.4 Settings Page Integration

Add the `BackupRestoreSection` to `SettingsPage.tsx` as a new section (admin only), placed before the User Management section:

```tsx
{/* Backup & Restore (Admin Only) */}
{user?.is_admin && (
  <section className="settings-section">
    <div className="settings-section-header">
      <div className="settings-section-icon">
        <i className="fas fa-database"></i>
      </div>
      <div>
        <h2 className="settings-section-title">Backup & Restore</h2>
        <p className="settings-section-description">
          Create backups of all data including inventories, items, and settings. 
          <strong> Warning:</strong> Restoring a backup will replace all current data.
        </p>
      </div>
    </div>
    <BackupRestoreSection />
  </section>
)}
```

### 7.5 Component Export

Add to `frontend/src/components/index.ts`:
```typescript
export { BackupRestoreSection } from './BackupRestoreSection';
```

---

## 8. Data Format Specification

### 8.1 Backup File Structure

Each backup is a single JSON file with the following structure:

```json
{
  "metadata": {
    "version": "1.0",
    "app_version": "0.1.0",
    "created_at": "2026-02-15T10:30:00.000Z",
    "database_type": "postgresql",
    "description": null
  },
  "data": {
    "users": [
      {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "username": "admin",
        "full_name": "Admin User",
        "password_hash": "$argon2id$...",
        "is_admin": true,
        "is_active": true,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
      }
    ],
    "inventories": [ /* ... */ ],
    "items": [ /* ... */ ],
    "categories": [ /* ... */ ],
    "tags": [ /* ... */ ],
    "item_tags": [ /* ... */ ],
    "custom_fields": [ /* ... */ ],
    "item_custom_values": [ /* ... */ ],
    "organizer_types": [ /* ... */ ],
    "organizer_options": [ /* ... */ ],
    "item_organizer_values": [ /* ... */ ],
    "user_settings": [ /* ... */ ],
    "inventory_shares": [ /* ... */ ],
    "user_access_grants": [ /* ... */ ],
    "recovery_codes": [ /* ... */ ]
  }
}
```

### 8.2 Version Compatibility

- **Version field**: `metadata.version` tracks the backup format version
- **Current version**: `"1.0"`
- **App version**: `metadata.app_version` tracks which version of Home Registry created the backup
- **Forward compatibility**: Future versions should be able to read v1.0 backups (additive changes only)
- **Validation**: Restore must check `metadata.version` and reject unsupported versions

### 8.3 Data Integrity

- All table data is exported using PostgreSQL's native JSON serialization (`row_to_json`), ensuring exact type fidelity
- Import uses `json_populate_record` which handles type casting automatically
- UUIDs, timestamps, decimals, and booleans are all preserved in their native PostgreSQL representations
- Sequence values are reset after import to prevent ID conflicts with future inserts

---

## 9. Safety & Error Handling

### 9.1 Security Measures

| Threat | Mitigation |
|--------|-----------|
| **Path traversal** | Validate filenames, check paths start with `backups/`, reject `..`, `/`, `\` |
| **Unauthorized access** | All endpoints require JWT auth + admin role |
| **Malicious upload** | Validate JSON structure matches `BackupData` schema before saving |
| **File size DOS** | Limit upload size to 100MB via multipart config |
| **SQL injection via restore** | Use `json_populate_record` with parameterized queries (not string interpolation for values) |
| **Existing data loss** | Auto-backup before restore; confirmation dialog on frontend |

### 9.2 Error Responses

All errors use the existing `ErrorResponse` format:

```json
{
  "success": false,
  "error": "Error description",
  "message": "User-friendly message"
}
```

Error scenarios:

| Scenario | HTTP Status | Error Message |
|----------|-------------|---------------|
| Not authenticated | 401 | "Authentication required" |
| Not admin | 403 | "Admin access required" |
| Backup file not found | 404 | "Backup file not found" |
| Invalid filename | 400 | "Invalid backup filename" |
| Invalid backup format | 400 | "Invalid backup file format" |
| Unsupported backup version | 400 | "Unsupported backup version: X" |
| Upload not .json | 400 | "Only .json backup files are allowed" |
| Database error during restore | 500 | "Failed to restore backup: database error" |
| File system error | 500 | "Failed to create/read backup file" |
| Upload too large | 413 | "File too large (max 100MB)" |

### 9.3 Transaction Safety

The restore operation uses a PostgreSQL transaction:
1. `BEGIN` transaction
2. `SET CONSTRAINTS ALL DEFERRED` — allows foreign key checks at commit time
3. `TRUNCATE TABLE ... RESTART IDENTITY CASCADE` for each table
4. Insert all rows
5. Reset sequences
6. `COMMIT` — if any step fails, the entire transaction is rolled back

This ensures the database is never left in a partially restored state.

### 9.4 Auto-Backup Before Restore

Before any restore operation:
1. Create a backup with prefix `home_registry_auto_pre_restore_`
2. If auto-backup fails, abort the restore and return error
3. Include the auto-backup filename in the restore success message

---

## 10. Implementation Plan

### Phase 1: Backend Core (Priority: High)

| Step | File | Description |
|------|------|-------------|
| 1 | `Cargo.toml` | Add `actix-multipart` dependency |
| 2 | `src/models/mod.rs` | Add `BackupInfo`, `BackupMetadata`, `BackupData`, `BackupDatabaseContent` structs |
| 3 | `src/db/mod.rs` | Add `export_all_data()` and `import_all_data()` methods to `DatabaseService` |
| 4 | `src/api/backup.rs` | Create new module with all 6 handler functions |
| 5 | `src/api/mod.rs` | Register `backup` module and add routes to `api_scope()` |
| 6 | `src/lib.rs` | No changes needed (api module already exported) |

### Phase 2: Frontend Integration (Priority: High)

| Step | File | Description |
|------|------|-------------|
| 7 | `frontend/src/types/index.ts` | Add `BackupInfo`, `BackupMetadata` interfaces |
| 8 | `frontend/src/services/api.ts` | Add `backupApi` object with all endpoint functions |
| 9 | `frontend/src/components/BackupRestoreSection.tsx` | Create new component |
| 10 | `frontend/src/components/index.ts` | Export `BackupRestoreSection` |
| 11 | `frontend/src/pages/SettingsPage.tsx` | Add Backup & Restore section (admin only) |

### Phase 3: Testing & Validation (Priority: High)

| Step | File | Description |
|------|------|-------------|
| 12 | `tests/test_api_integration.rs` | Add backup API integration tests |
| 13 | `tests/test_db.rs` | Add `export_all_data` / `import_all_data` unit tests |
| 14 | Manual testing | End-to-end testing of create/download/upload/restore/delete |

---

## 11. Dependencies

### 11.1 New Backend Dependencies

| Crate | Version | License | Purpose |
|-------|---------|---------|---------|
| `actix-multipart` | `0.7.2` | MIT/Apache-2.0 | Multipart form handling for file upload |

### 11.2 Existing Dependencies (No Changes)

| Crate | Current Version | Used For |
|-------|----------------|----------|
| `actix-web` | `4.12.1` | Web framework, route handlers |
| `actix-files` | `0.6.10` | File download (NamedFile) |
| `serde` / `serde_json` | `1.0.220` / `1.0.138` | JSON serialization/deserialization |
| `chrono` | `0.4.39` | Timestamp generation, date formatting |
| `tokio` | `1.49.0` | Async file I/O operations |
| `tokio-postgres` | `0.7.12` | Database queries (json_agg, json_populate_record) |
| `deadpool-postgres` | `0.14.0` | Connection pooling, transactions |
| `log` | `0.4.22` | Logging (info!, error!) |
| `uuid` | `1.11.0` | UUID handling |

### 11.3 Frontend Dependencies (No Changes)

All frontend functionality uses standard Web APIs (`fetch`, `FormData`, `Blob`, `URL.createObjectURL`).

---

## 12. Testing Strategy

### 12.1 Unit Tests

- `BackupInfo` serialization/deserialization
- `BackupMetadata` validation
- `BackupData` round-trip (serialize → deserialize → compare)
- `validate_backup_filename()` with valid/invalid inputs
- `format_file_size()` with various byte counts
- `generate_backup_filename()` format validation

### 12.2 Integration Tests

- Create backup → list backups → verify backup appears
- Create backup → download backup → verify JSON structure
- Create backup → restore backup → verify data matches
- Upload backup → list backups → verify uploaded file appears
- Delete backup → list backups → verify backup removed
- Auto-backup before restore → verify auto-backup created
- Non-admin user → verify 403 Forbidden for all endpoints
- Invalid filename → verify 400 Bad Request
- Non-existent file → verify 404 Not Found
- Transaction rollback on restore failure

### 12.3 Frontend Tests

- Component renders correctly for admin users
- Component hidden for non-admin users
- Create backup button shows loading state
- Upload triggers file input
- Download triggers browser download
- Restore shows confirmation modal
- Delete shows confirmation modal
- Error states display toast notifications

---

## 13. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| **Large database causes timeout** | Medium | High | Implement streaming JSON write; add progress logging; set generous request timeout |
| **Restore leaves DB in bad state** | Low | Critical | Transaction-based restore; auto-backup before restore |
| **Password hashes exposed in backup** | Medium | High | Backup files are admin-only; document that backup files contain sensitive data; consider encrypting backups in future version |
| **Backup format changes break restore** | Low | Medium | Version field enables format migration; backwards-compatible changes only |
| **Disk space exhaustion from backups** | Low | Medium | Display backup sizes; provide delete functionality; consider auto-cleanup of old auto-backups in future |
| **Concurrent restore operations** | Low | High | Could add file-based lock or database lock to prevent concurrent restores |
| **`actix-multipart` version compatibility** | Low | Low | Pin exact version; test with current Actix-Web 4.12.1 |
| **Sequence reset fails for some tables** | Low | Medium | Wrap in try/catch; log warnings but don't fail restore |

---

## Appendix A: File Listing

### Files to Create
- `src/api/backup.rs` — Backend backup handler functions

### Files to Modify
- `Cargo.toml` — Add `actix-multipart` dependency
- `src/api/mod.rs` — Add `pub mod backup;`, register routes in `api_scope()`
- `src/models/mod.rs` — Add backup-related structs
- `src/db/mod.rs` — Add `export_all_data()` and `import_all_data()` methods
- `frontend/src/types/index.ts` — Add backup TypeScript interfaces
- `frontend/src/services/api.ts` — Add `backupApi` object
- `frontend/src/components/index.ts` — Export `BackupRestoreSection`
- `frontend/src/pages/SettingsPage.tsx` — Add Backup & Restore section

### Files to Create (Frontend)
- `frontend/src/components/BackupRestoreSection.tsx` — React component

### Directories Created at Runtime
- `backups/` — Created automatically when first backup is made

---

## Appendix B: API Endpoint Summary

| Method | Path | Auth | Admin | Description |
|--------|------|------|-------|-------------|
| `POST` | `/api/backup/create` | Yes | Yes | Create a new backup |
| `GET` | `/api/backup/list` | Yes | Yes | List all backups |
| `GET` | `/api/backup/download/{filename}` | Yes | Yes | Download a backup file |
| `POST` | `/api/backup/upload` | Yes | Yes | Upload a backup file |
| `POST` | `/api/backup/restore/{filename}` | Yes | Yes | Restore from a backup |
| `DELETE` | `/api/backup/{filename}` | Yes | Yes | Delete a backup |

---

## Appendix C: Context7 Research References

1. **actix-web** (`/actix/actix-web`) — Multipart form handling via `actix_multipart::form::MultipartForm` derive macro with `TempFile` for file uploads; `actix_files::NamedFile` for file downloads with `Content-Disposition` headers
2. **serde** (`/serde-rs/serde`) — Standard `#[derive(Serialize, Deserialize)]` patterns for JSON serialization; `serde_json::to_string_pretty()` for human-readable backup output
3. **tokio** (`/tokio-rs/tokio`) — `tokio::fs::write()`, `tokio::fs::read()`, `tokio::fs::create_dir_all()` for async file operations
4. **chrono** (`/chronotope/chrono`) — `Utc::now().format("%Y.%m.%d.%H.%M.%S")` for timestamped filenames; `to_rfc3339()` for ISO 8601 metadata timestamps
5. **Humidor project** (`analysis/humidor/`) — Complete reference implementation with backup/restore handlers, services, routes, and frontend UI patterns
6. **PostgreSQL JSON functions** — `json_agg(row_to_json(t))` for export, `json_populate_record(NULL::table, $1::json)` for import — proven pattern from Humidor implementation
