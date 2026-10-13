# Inventory Reporting Specification

**Feature:** Inventory Reporting and Export Functionality  
**Date:** February 14, 2026  
**Author:** Research Subagent  
**Project:** Home Registry (Rust-based home inventory management system)

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current State Analysis](#current-state-analysis)
3. [Research Sources and Best Practices](#research-sources-and-best-practices)
4. [Proposed Solution Architecture](#proposed-solution-architecture)
5. [Implementation Steps](#implementation-steps)
6. [Dependencies and Requirements](#dependencies-and-requirements)
7. [Database Query Design](#database-query-design)
8. [API Specifications](#api-specifications)
9. [Potential Risks and Mitigations](#potential-risks-and-mitigations)
10. [Testing Strategy](#testing-strategy)

---

## Executive Summary

This specification outlines the addition of comprehensive reporting and export capabilities to the Home Registry project. The feature will enable users to generate detailed inventory reports in multiple formats (JSON, CSV) with flexible filtering, sorting, and aggregation options. The solution leverages Actix-Web's streaming capabilities for efficient handling of large datasets while maintaining the project's existing architecture patterns.

**Key Features:**
- Multi-format export (JSON, CSV)
- Flexible filtering by inventory, category, date ranges, price ranges
- Aggregated statistics (total value, item counts, category breakdowns)
- Streaming response support for large datasets
- RESTful API design consistent with existing endpoints
- User authentication and authorization integration

---

## Current State Analysis

### Existing Inventory Data Structure

The Home Registry project currently maintains the following data models:

#### 1. **Items Table** (from `migrations/001_create_items_table.sql`)
```sql
CREATE TABLE items (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    category VARCHAR(100),
    location VARCHAR(100),
    purchase_date DATE,
    purchase_price DECIMAL(10, 2),
    warranty_expiry DATE,
    notes TEXT,
    quantity INTEGER DEFAULT 1,
    inventory_id INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
```

#### 2. **Inventories Table** (from `migrations/002_create_inventories_table.sql`)
```sql
CREATE TABLE inventories (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    location VARCHAR(255),
    user_id UUID,
    image_url TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
```

#### 3. **Rust Models** (from `src/models/mod.rs`)
- `Item`: Core item structure with financial fields (purchase_price, quantity)
- `Inventory`: Container structure for items
- `ApiResponse<T>`: Standard response wrapper
- `ErrorResponse`: Error handling structure

### Existing API Endpoints

**Current item-related endpoints** (from `src/api/mod.rs`):
- `GET /api/items` - Get all items
- `GET /api/items/{id}` - Get single item
- `GET /api/items/search/{query}` - Search items by text
- `GET /api/inventories/{id}/items` - Get items by inventory
- `POST /api/items` - Create item
- `PUT /api/items/{id}` - Update item
- `DELETE /api/items/{id}` - Delete item

### Database Service Patterns

From `src/db/mod.rs`, the project follows these patterns:
1. **Connection Pooling**: Uses `deadpool-postgres` with custom `get_pool()` function
2. **Query Execution**: Direct SQL queries with proper error handling
3. **Type Casting**: Explicit casting for DATE (`::text`) and DECIMAL (`::float8`) fields
4. **Search Implementation**: Parameterized queries with LIKE pattern matching and SQL injection protection
5. **Service Pattern**: All DB operations through `DatabaseService` struct

### Authentication Context

From `src/api/auth.rs`:
- JWT-based authentication with `get_auth_context_from_request()`
- User-scoped data access (users can only access their own inventories)
- Permission levels for shared inventories

---

## Research Sources and Best Practices

### 1. **Actix-Web Streaming Responses**

**Source:** Actix-Web Official Documentation (v4.12.1)  
**URL:** https://actix.rs/docs/response/

**Key Findings:**
- Actix-Web supports streaming responses via `HttpResponse::streaming()` and `HttpResponse::Ok().streaming(body)`
- Uses `futures::stream::Stream` trait for async streaming
- Ideal for large CSV exports to avoid loading entire dataset into memory
- Automatically handles chunked transfer encoding

**Implementation Pattern:**
```rust
use futures::stream;
use actix_web::{HttpResponse, web};

HttpResponse::Ok()
    .content_type("text/csv")
    .streaming(stream::iter(data_chunks))
```

### 2. **CSV Generation in Rust**

**Source:** `csv` crate (v1.3.0) - Official Documentation  
**URL:** https://docs.rs/csv/1.3.0/csv/

**Key Findings:**
- Industry-standard CSV library for Rust
- Supports serialization via Serde
- Writer can output directly to any `io::Write` implementor
- RFC 4180 compliant (proper escaping, quoting)
- Zero-copy string building for performance

**Implementation Pattern:**
```rust
use csv::Writer;
use serde::Serialize;

#[derive(Serialize)]
struct ItemExport {
    name: String,
    category: Option<String>,
    // ... other fields
}

let mut writer = Writer::from_writer(vec![]);
writer.serialize(item_export)?;
let data = writer.into_inner()?;
```

### 3. **RESTful Report API Design**

**Source:** REST API Design Best Practices (Microsoft Azure Architecture)  
**URL:** https://learn.microsoft.com/en-us/azure/architecture/best-practices/api-design

**Key Findings:**
- Use query parameters for filtering, sorting, pagination
- Use proper HTTP headers (Content-Type, Content-Disposition)
- Support multiple representations (JSON, CSV) via Accept header or query param
- Use 200 OK for successful exports
- Include metadata in headers (row count, generated timestamp)

**Recommended Endpoint Structure:**
```
GET /api/reports/inventory?inventory_id={id}&format={json|csv}&from_date={date}&to_date={date}
```

### 4. **Database Query Optimization for Reporting**

**Source:** PostgreSQL Performance Documentation  
**URL:** https://www.postgresql.org/docs/16/performance-tips.html

**Key Findings:**
- Use aggregate functions (SUM, COUNT, AVG) in SQL rather than application code
- Leverage existing indexes (category, location, name)
- Use `EXPLAIN ANALYZE` to verify query plans
- Consider materialized views for complex recurring reports (future optimization)
- Use `LIMIT` and `OFFSET` for pagination if needed

**Recommended Query Pattern:**
```sql
SELECT 
    category,
    COUNT(*) as item_count,
    SUM(quantity) as total_quantity,
    SUM(purchase_price::float8 * quantity) as total_value
FROM items
WHERE inventory_id = $1
GROUP BY category
ORDER BY total_value DESC;
```

### 5. **Content-Disposition Headers for File Downloads**

**Source:** RFC 6266 - Use of the Content-Disposition Header Field  
**URL:** https://datatracker.ietf.org/doc/html/rfc6266

**Key Findings:**
- Use `Content-Disposition: attachment; filename="report.csv"` for downloads
- Include timestamp in filename for traceability
- Sanitize filename values to prevent injection
- Use ASCII-safe filenames or RFC 5987 encoding for Unicode

**Implementation Pattern:**
```rust
HttpResponse::Ok()
    .content_type("text/csv")
    .insert_header(("Content-Disposition", 
        format!("attachment; filename=\"inventory-report-{}.csv\"", 
        chrono::Utc::now().format("%Y%m%d-%H%M%S"))))
    .body(csv_data)
```

### 6. **Error Handling in Data Export Operations**

**Source:** Rust Error Handling Best Practices  
**URL:** https://doc.rust-lang.org/book/ch09-00-error-handling.html

**Key Findings:**
- Use `Result<T, E>` for all fallible operations
- Avoid `.unwrap()` in production code
- Log errors with context (log crate)
- Return user-friendly error messages
- Handle partial failures gracefully (e.g., skip invalid rows with warning)

### 7. **Memory-Efficient Data Processing**

**Source:** Rust Performance Book  
**URL:** https://nnethercote.github.io/perf-book/

**Key Findings:**
- Stream data row-by-row instead of loading all into Vec
- Use `async_stream` for async iteration
- Avoid cloning large structures unnecessarily
- Use `String::with_capacity()` for known-size allocations
- Consider using `bytes::BytesMut` for building response buffers

---

## Proposed Solution Architecture

### Overview

The reporting system will consist of three main components:

1. **Report Generator Service** - Business logic for report generation
2. **Export Formatters** - Format-specific serializers (JSON, CSV)
3. **Report API Endpoints** - RESTful endpoints for report requests

```
┌─────────────────────────────────────────────────────┐
│              Client Request                          │
│  GET /api/reports/inventory?format=csv&...          │
└────────────────┬────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────┐
│         API Handler (src/api/mod.rs)                │
│  - Validate parameters                               │
│  - Check authentication                              │
│  - Route to appropriate report generator            │
└────────────────┬────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────┐
│    Report Generator (src/db/mod.rs)                 │
│  - Build SQL query with filters                     │
│  - Execute query via DatabaseService                │
│  - Calculate aggregates                             │
└────────────────┬────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────┐
│      Export Formatter (src/models/mod.rs)           │
│  - CSV: Use csv crate with streaming                │
│  - JSON: Use serde_json with ApiResponse wrapper    │
└────────────────┬────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────┐
│         HTTP Response                                │
│  - Proper Content-Type headers                      │
│  - Content-Disposition for downloads                │
│  - Streaming for large datasets                     │
└─────────────────────────────────────────────────────┘
```

### Component Responsibilities

#### 1. Database Service Extensions (`src/db/mod.rs`)

Add methods to `DatabaseService`:
- `get_inventory_report_data()` - Fetch detailed item data with filters
- `get_inventory_statistics()` - Fetch aggregated statistics
- `get_category_breakdown()` - Group by category with totals

#### 2. Model Extensions (`src/models/mod.rs`)

Add new models:
- `InventoryReportRequest` - Query parameters for filtering
- `InventoryReportData` - Full report data structure
- `InventoryStatistics` - Aggregated statistics
- `CategoryBreakdown` - Category-level aggregates
- `ItemExportRow` - Flattened item structure for CSV export

#### 3. API Endpoints (`src/api/mod.rs`)

Add new report endpoints:
- `GET /api/reports/inventory` - Main report endpoint (JSON/CSV)
- `GET /api/reports/inventory/statistics` - Summary statistics
- `GET /api/reports/inventory/categories` - Category breakdown

---

## Implementation Steps

### Phase 1: Data Models and Request Types (Day 1)

**File:** `src/models/mod.rs`

1. **Add InventoryReportRequest**
   ```rust
   #[derive(Deserialize, Debug, Validate)]
   pub struct InventoryReportRequest {
       pub inventory_id: Option<i32>,
       pub category: Option<String>,
       pub location: Option<String>,
       pub from_date: Option<String>,  // ISO 8601 format
       pub to_date: Option<String>,
       pub min_price: Option<f64>,
       pub max_price: Option<f64>,
       pub sort_by: Option<String>,    // "name", "price", "date"
       pub sort_order: Option<String>, // "asc", "desc"
       pub format: Option<String>,     // "json", "csv"
   }
   ```

2. **Add InventoryStatistics**
   ```rust
   #[derive(Serialize, Debug)]
   pub struct InventoryStatistics {
       pub total_items: i64,
       pub total_value: f64,
       pub total_quantity: i64,
       pub category_count: i64,
       pub inventories_count: i64,
       pub oldest_item_date: Option<String>,
       pub newest_item_date: Option<String>,
       pub average_item_value: f64,
   }
   ```

3. **Add CategoryBreakdown**
   ```rust
   #[derive(Serialize, Debug)]
   pub struct CategoryBreakdown {
       pub category: String,
       pub item_count: i64,
       pub total_quantity: i64,
       pub total_value: f64,
       pub percentage_of_total: f64,
   }
   ```

4. **Add ItemExportRow (CSV optimized)**
   ```rust
   #[derive(Serialize, Debug)]
   pub struct ItemExportRow {
       pub id: i32,
       pub inventory_name: String,
       pub item_name: String,
       pub description: String,
       pub category: String,
       pub location: String,
       pub quantity: i32,
       pub purchase_price: String,
       pub total_value: String,
       pub purchase_date: String,
       pub warranty_expiry: String,
       pub created_at: String,
   }
   ```

5. **Add InventoryReportData (complete report)**
   ```rust
   #[derive(Serialize, Debug)]
   pub struct InventoryReportData {
       pub statistics: InventoryStatistics,
       pub category_breakdown: Vec<CategoryBreakdown>,
       pub items: Vec<Item>,
       pub generated_at: DateTime<Utc>,
       pub filters_applied: InventoryReportRequest,
   }
   ```

### Phase 2: Database Service Methods (Day 1-2)

**File:** `src/db/mod.rs`

1. **Add `get_inventory_report_data()`**
   ```rust
   pub async fn get_inventory_report_data(
       &self,
       request: InventoryReportRequest,
       user_id: Uuid,
   ) -> Result<Vec<Item>, Box<dyn std::error::Error>> {
       let client = self.pool.get().await?;
       
       // Build dynamic WHERE clause based on filters
       let mut conditions = vec!["i.inventory_id IN (SELECT id FROM inventories WHERE user_id = $1)"];
       let mut param_index = 2;
       let mut values: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![];
       values.push(Box::new(user_id));
       
       // Add optional filters to conditions and values
       // ... (see detailed implementation below)
       
       let query = format!(
           "SELECT i.id, i.inventory_id, i.name, i.description, i.category, i.location, 
                   i.purchase_date::text, i.purchase_price::float8, i.warranty_expiry::text,
                   i.notes, i.quantity, i.created_at, i.updated_at
            FROM items i
            WHERE {}
            ORDER BY {}",
           conditions.join(" AND "),
           build_order_by(&request)
       );
       
       // Execute and map results
   }
   ```

2. **Add `get_inventory_statistics()`**
   ```rust
   pub async fn get_inventory_statistics(
       &self,
       inventory_id: Option<i32>,
       user_id: Uuid,
   ) -> Result<InventoryStatistics, Box<dyn std::error::Error>> {
       let client = self.pool.get().await?;
       
       let query = if let Some(inv_id) = inventory_id {
           "SELECT 
               COUNT(*) as total_items,
               COALESCE(SUM(purchase_price::float8 * quantity), 0) as total_value,
               COALESCE(SUM(quantity), 0) as total_quantity,
               COUNT(DISTINCT category) as category_count,
               1 as inventories_count,
               MIN(purchase_date)::text as oldest_item_date,
               MAX(purchase_date)::text as newest_item_date,
               COALESCE(AVG(purchase_price::float8), 0) as average_item_value
            FROM items
            WHERE inventory_id = $1"
       } else {
           // Query for all user inventories
       };
       
       // Execute and map to InventoryStatistics
   }
   ```

3. **Add `get_category_breakdown()`**
   ```rust
   pub async fn get_category_breakdown(
       &self,
       inventory_id: Option<i32>,
       user_id: Uuid,
   ) -> Result<Vec<CategoryBreakdown>, Box<dyn std::error::Error>> {
       let client = self.pool.get().await?;
       
       let query = "
           WITH totals AS (
               SELECT SUM(purchase_price::float8 * quantity) as grand_total
               FROM items
               WHERE inventory_id IN (SELECT id FROM inventories WHERE user_id = $1)
           )
           SELECT 
               COALESCE(i.category, 'Uncategorized') as category,
               COUNT(*) as item_count,
               SUM(i.quantity) as total_quantity,
               SUM(i.purchase_price::float8 * i.quantity) as total_value,
               (SUM(i.purchase_price::float8 * i.quantity) / t.grand_total * 100) as percentage
           FROM items i
           CROSS JOIN totals t
           WHERE i.inventory_id IN (SELECT id FROM inventories WHERE user_id = $1)
           GROUP BY i.category, t.grand_total
           ORDER BY total_value DESC
       ";
       
       // Execute and map to Vec<CategoryBreakdown>
   }
   ```

### Phase 3: Export Formatters (Day 2)

**File:** `src/api/mod.rs` (or consider new `src/formatters/mod.rs`)

1. **CSV Export Function**
   ```rust
   fn format_items_as_csv(
       items: Vec<Item>,
       inventories: std::collections::HashMap<i32, String>
   ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
       let mut writer = csv::Writer::from_writer(vec![]);
       
       // Write header
       writer.write_record(&[
           "ID", "Inventory", "Name", "Description", 
           "Category", "Location", "Quantity", 
           "Purchase Price", "Total Value", 
           "Purchase Date", "Warranty Expiry", "Created At"
       ])?;
       
       // Write data rows
       for item in items {
           let inventory_name = inventories.get(&item.inventory_id)
               .map(|s| s.as_str())
               .unwrap_or("Unknown");
           
           let total_value = item.purchase_price
               .and_then(|price| item.quantity.map(|qty| price * qty as f64))
               .map(|v| format!("{:.2}", v))
               .unwrap_or_default();
           
           writer.serialize(ItemExportRow {
               id: item.id.unwrap_or(0),
               inventory_name: inventory_name.to_string(),
               item_name: item.name,
               description: item.description.unwrap_or_default(),
               category: item.category.unwrap_or_default(),
               location: item.location.unwrap_or_default(),
               quantity: item.quantity.unwrap_or(0),
               purchase_price: item.purchase_price.map(|p| format!("{:.2}", p)).unwrap_or_default(),
               total_value,
               purchase_date: item.purchase_date.unwrap_or_default(),
               warranty_expiry: item.warranty_expiry.unwrap_or_default(),
               created_at: item.created_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
           })?;
       }
       
       writer.flush()?;
       Ok(writer.into_inner()?)
   }
   ```

2. **JSON Export (using existing ApiResponse pattern)**
   - Use standard `ApiResponse<InventoryReportData>` structure
   - Leverage existing serde_json serialization

### Phase 4: API Endpoints (Day 2-3)

**File:** `src/api/mod.rs`

1. **Main Report Endpoint**
   ```rust
   #[get("/reports/inventory")]
   pub async fn get_inventory_report(
       pool: web::Data<Pool>,
       req: HttpRequest,
       query: web::Query<InventoryReportRequest>,
   ) -> Result<impl Responder> {
       // Get authenticated user context
       let auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
           Ok(a) => a,
           Err(e) => return Ok(e),
       };
       
       let db_service = DatabaseService::new(pool.get_ref().clone());
       let request = query.into_inner();
       let format = request.format.as_deref().unwrap_or("json");
       
       // Determine if user has access to requested inventory
       if let Some(inv_id) = request.inventory_id {
           match db_service.check_inventory_access(auth.user_id, inv_id).await {
               Ok(false) => return Ok(HttpResponse::Forbidden().json(ErrorResponse {
                   success: false,
                   error: "Access denied to this inventory".to_string(),
                   message: None,
               })),
               Err(e) => {
                   error!("Error checking inventory access: {}", e);
                   return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                       success: false,
                       error: "An internal error occurred".to_string(),
                       message: None,
                   }));
               },
               _ => {}
           }
       }
       
       // Fetch report data
       let items = match db_service.get_inventory_report_data(request.clone(), auth.user_id).await {
           Ok(items) => items,
           Err(e) => {
               error!("Error generating report: {}", e);
               return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                   success: false,
                   error: "Failed to generate report".to_string(),
                   message: Some(e.to_string()),
               }));
           }
       };
       
       match format {
           "csv" => {
               // Fetch inventory names for CSV export
               let inventory_names = db_service.get_accessible_inventories(auth.user_id)
                   .await?
                   .into_iter()
                   .filter_map(|inv| inv.id.map(|id| (id, inv.name)))
                   .collect();
               
               match format_items_as_csv(items, inventory_names) {
                   Ok(csv_data) => {
                       let filename = format!(
                           "inventory-report-{}.csv",
                           chrono::Utc::now().format("%Y%m%d-%H%M%S")
                       );
                       
                       info!("Generated CSV report for user {}: {} bytes", auth.username, csv_data.len());
                       
                       Ok(HttpResponse::Ok()
                           .content_type("text/csv; charset=utf-8")
                           .insert_header((
                               "Content-Disposition",
                               format!("attachment; filename=\"{}\"", filename)
                           ))
                           .body(csv_data))
                   },
                   Err(e) => {
                       error!("Error formatting CSV: {}", e);
                       Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                           success: false,
                           error: "Failed to format CSV".to_string(),
                           message: Some(e.to_string()),
                       }))
                   }
               }
           },
           "json" | _ => {
               // Fetch additional data for complete report
               let statistics = db_service.get_inventory_statistics(request.inventory_id, auth.user_id).await?;
               let category_breakdown = db_service.get_category_breakdown(request.inventory_id, auth.user_id).await?;
               
               let report_data = InventoryReportData {
                   statistics,
                   category_breakdown,
                   items,
                   generated_at: chrono::Utc::now(),
                   filters_applied: request,
               };
               
               info!("Generated JSON report for user {}", auth.username);
               
               Ok(HttpResponse::Ok().json(ApiResponse {
                   success: true,
                   data: Some(report_data),
                   message: Some("Report generated successfully".to_string()),
                   error: None,
               }))
           }
       }
   }
   ```

2. **Statistics Endpoint**
   ```rust
   #[get("/reports/inventory/statistics")]
   pub async fn get_inventory_statistics_endpoint(
       pool: web::Data<Pool>,
       req: HttpRequest,
       query: web::Query<InventoryReportRequest>,
   ) -> Result<impl Responder> {
       let auth = match auth::get_auth_context_from_request(&req, pool.get_ref()).await {
           Ok(a) => a,
           Err(e) => return Ok(e),
       };
       
       let db_service = DatabaseService::new(pool.get_ref().clone());
       let request = query.into_inner();
       
       match db_service.get_inventory_statistics(request.inventory_id, auth.user_id).await {
           Ok(stats) => {
               info!("Retrieved statistics for user {}", auth.username);
               Ok(HttpResponse::Ok().json(ApiResponse {
                   success: true,
                   data: Some(stats),
                   message: Some("Statistics retrieved successfully".to_string()),
                   error: None,
               }))
           },
           Err(e) => {
               error!("Error retrieving statistics: {}", e);
               Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                   success: false,
                   error: "Failed to retrieve statistics".to_string(),
                   message: Some(e.to_string()),
               }))
           }
       }
   }
   ```

3. **Category Breakdown Endpoint**
   ```rust
   #[get("/reports/inventory/categories")]
   pub async fn get_category_breakdown_endpoint(
       pool: web::Data<Pool>,
       req: HttpRequest,
       query: web::Query<InventoryReportRequest>,
   ) -> Result<impl Responder> {
       // Similar structure to statistics endpoint
       // ... implementation
   }
   ```

4. **Register Routes in `init_routes()`**
   ```rust
   pub fn init_routes() -> Scope {
       web::scope("/api")
           // ... existing routes
           .service(get_inventory_report)
           .service(get_inventory_statistics_endpoint)
           .service(get_category_breakdown_endpoint)
   }
   ```

### Phase 5: Helper Functions (Day 3)

**File:** `src/db/mod.rs`

1. **Add `check_inventory_access()`**
   ```rust
   pub async fn check_inventory_access(
       &self,
       user_id: Uuid,
       inventory_id: i32,
   ) -> Result<bool, Box<dyn std::error::Error>> {
       let client = self.pool.get().await?;
       
       let row = client
           .query_one(
               "SELECT COUNT(*) as count FROM inventories 
                WHERE id = $1 AND user_id = $2",
               &[&inventory_id, &user_id],
           )
           .await?;
       
       let count: i64 = row.get(0);
       Ok(count > 0)
   }
   ```

2. **Add query builder helpers** (private functions)
   ```rust
   fn build_order_by(request: &InventoryReportRequest) -> String {
       let sort_by = request.sort_by.as_deref().unwrap_or("created_at");
       let sort_order = request.sort_order.as_deref().unwrap_or("desc");
       
       let column = match sort_by {
           "name" => "i.name",
           "price" => "i.purchase_price",
           "date" => "i.purchase_date",
           "category" => "i.category",
           _ => "i.created_at",
       };
       
       let order = if sort_order.eq_ignore_ascii_case("asc") { "ASC" } else { "DESC" };
       
       format!("{} {}", column, order)
   }
   ```

---

## Dependencies and Requirements

### New Dependencies (add to `Cargo.toml`)

```toml
[dependencies]
# Existing dependencies remain unchanged
# ...existing dependencies...

# CSV export support (MIT/Apache-2.0 dual license)
csv = "=1.3.0"
```

**Justification:**
- **csv v1.3.0**: Industry-standard CSV library for Rust, RFC 4180 compliant, well-maintained, MIT/Apache-2.0 license compatible with project

### Existing Dependencies (already in project)

- `actix-web = "=4.12.1"` - Web framework (streaming support)
- `tokio-postgres = "=0.7.12"` - Database driver
- `serde = "=1.0.220"` - Serialization
- `serde_json = "=1.0.138"` - JSON serialization
- `chrono = "=0.4.39"` - Date/time handling
- `log = "=0.4.22"` - Logging
- `uuid = "=1.11.0"` - User ID handling

### System Requirements

- PostgreSQL 16+ (already required by project)
- Rust 1.88+ (already specified in Cargo.toml)
- No additional system dependencies required

---

## Database Query Design

### Query 1: Detailed Report Data with Filters

```sql
-- Purpose: Fetch filtered item data for report generation
-- Performance: Uses existing indexes on category, location, name

SELECT 
    i.id, 
    i.inventory_id, 
    i.name, 
    i.description, 
    i.category, 
    i.location,
    i.purchase_date::text, 
    i.purchase_price::float8, 
    i.warranty_expiry::text,
    i.notes, 
    i.quantity, 
    i.created_at, 
    i.updated_at
FROM items i
WHERE i.inventory_id IN (
    SELECT id FROM inventories WHERE user_id = $1
)
  AND ($2::int IS NULL OR i.inventory_id = $2)
  AND ($3::text IS NULL OR i.category = $3)
  AND ($4::text IS NULL OR i.location LIKE $4)
  AND ($5::date IS NULL OR i.purchase_date >= $5)
  AND ($6::date IS NULL OR i.purchase_date <= $6)
  AND ($7::float8 IS NULL OR i.purchase_price >= $7)
  AND ($8::float8 IS NULL OR i.purchase_price <= $8)
ORDER BY i.created_at DESC;

-- Parameters:
-- $1: user_id (UUID)
-- $2: inventory_id (Optional<i32>)
-- $3: category (Optional<String>)
-- $4: location pattern (Optional<String>)
-- $5: from_date (Optional<Date>)
-- $6: to_date (Optional<Date>)
-- $7: min_price (Optional<f64>)
-- $8: max_price (Optional<f64>)
```

**Index Usage:**
- `idx_items_category` - Used for category filter
- `idx_items_location` - Used for location filter
- `idx_items_name` - Available for name searches

### Query 2: Aggregated Statistics

```sql
-- Purpose: Calculate summary statistics for dashboard
-- Performance: Single-pass aggregation with COALESCE for NULL handling

SELECT 
    COUNT(*) as total_items,
    COALESCE(SUM(purchase_price::float8 * quantity), 0) as total_value,
    COALESCE(SUM(quantity), 0) as total_quantity,
    COUNT(DISTINCT category) as category_count,
    COUNT(DISTINCT inventory_id) as inventories_count,
    MIN(purchase_date)::text as oldest_item_date,
    MAX(purchase_date)::text as newest_item_date,
    COALESCE(AVG(purchase_price::float8), 0) as average_item_value
FROM items
WHERE inventory_id IN (
    SELECT id FROM inventories WHERE user_id = $1
)
  AND ($2::int IS NULL OR inventory_id = $2);

-- Parameters:
-- $1: user_id (UUID)
-- $2: inventory_id (Optional<i32>) - NULL means all inventories
```

**Performance Notes:**
- Single table scan with efficient aggregations
- COALESCE prevents NULL results breaking Rust deserialization
- No joins required

### Query 3: Category Breakdown with Percentages

```sql
-- Purpose: Group items by category with value calculations
-- Performance: Uses CTE for grand total, single pass aggregation

WITH totals AS (
    SELECT SUM(purchase_price::float8 * quantity) as grand_total
    FROM items
    WHERE inventory_id IN (
        SELECT id FROM inventories WHERE user_id = $1
    )
    AND ($2::int IS NULL OR inventory_id = $2)
)
SELECT 
    COALESCE(i.category, 'Uncategorized') as category,
    COUNT(*) as item_count,
    COALESCE(SUM(i.quantity), 0) as total_quantity,
    COALESCE(SUM(i.purchase_price::float8 * i.quantity), 0) as total_value,
    CASE 
        WHEN t.grand_total > 0 THEN 
            (COALESCE(SUM(i.purchase_price::float8 * i.quantity), 0) / t.grand_total * 100)
        ELSE 0 
    END as percentage
FROM items i
CROSS JOIN totals t
WHERE i.inventory_id IN (
    SELECT id FROM inventories WHERE user_id = $1
)
  AND ($2::int IS NULL OR i.inventory_id = $2)
GROUP BY i.category, t.grand_total
ORDER BY total_value DESC;

-- Parameters:
-- $1: user_id (UUID)
-- $2: inventory_id (Optional<i32>)
```

**Performance Notes:**
- CTE calculates grand total once
- CROSS JOIN is efficient with single-row CTE
- GROUP BY uses indexed category column
- Division-by-zero protection with CASE statement

### Query 4: Inventory Access Check

```sql
-- Purpose: Verify user has access to requested inventory
-- Performance: Fast lookup with indexed user_id

SELECT COUNT(*) as count 
FROM inventories 
WHERE id = $1 AND user_id = $2;

-- Parameters:
-- $1: inventory_id (i32)
-- $2: user_id (UUID)

-- Returns: count > 0 means user has access
```

### Database Indexes (already exist)

From `migrations/001_create_items_table.sql`:
```sql
CREATE INDEX IF NOT EXISTS idx_items_category ON items(category);
CREATE INDEX IF NOT EXISTS idx_items_location ON items(location);
CREATE INDEX IF NOT EXISTS idx_items_name ON items(name);
```

**Additional Index Recommendations** (future optimization):
```sql
-- Composite index for filtered queries
CREATE INDEX IF NOT EXISTS idx_items_inventory_date 
ON items(inventory_id, purchase_date DESC);

-- Index for price-based filtering
CREATE INDEX IF NOT EXISTS idx_items_price 
ON items(purchase_price) WHERE purchase_price IS NOT NULL;
```

---

## API Specifications

### Endpoint 1: Generate Inventory Report

**Method:** `GET`  
**Path:** `/api/reports/inventory`  
**Authentication:** Required (JWT token)

**Query Parameters:**

| Parameter | Type | Required | Description | Example |
|-----------|------|----------|-------------|---------|
| `inventory_id` | integer | No | Filter by specific inventory | `5` |
| `category` | string | No | Filter by category | `Electronics` |
| `location` | string | No | Filter by location (supports LIKE) | `Living Room` |
| `from_date` | string | No | Start date (ISO 8601) | `2024-01-01` |
| `to_date` | string | No | End date (ISO 8601) | `2024-12-31` |
| `min_price` | number | No | Minimum purchase price | `100.00` |
| `max_price` | number | No | Maximum purchase price | `5000.00` |
| `sort_by` | string | No | Sort field (name, price, date, category) | `price` |
| `sort_order` | string | No | Sort direction (asc, desc) | `desc` |
| `format` | string | No | Output format (json, csv) | `csv` |

**Request Example (JSON):**
```http
GET /api/reports/inventory?inventory_id=5&category=Electronics&format=json HTTP/1.1
Host: localhost:8210
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Request Example (CSV):**
```http
GET /api/reports/inventory?inventory_id=5&from_date=2024-01-01&to_date=2024-12-31&format=csv HTTP/1.1
Host: localhost:8210
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Response (JSON - 200 OK):**
```json
{
  "success": true,
  "data": {
    "statistics": {
      "total_items": 142,
      "total_value": 28450.75,
      "total_quantity": 256,
      "category_count": 8,
      "inventories_count": 1,
      "oldest_item_date": "2020-03-15",
      "newest_item_date": "2024-02-10",
      "average_item_value": 200.36
    },
    "category_breakdown": [
      {
        "category": "Electronics",
        "item_count": 45,
        "total_quantity": 67,
        "total_value": 12500.00,
        "percentage_of_total": 43.94
      },
      {
        "category": "Furniture",
        "item_count": 28,
        "total_quantity": 45,
        "total_value": 8300.50,
        "percentage_of_total": 29.17
      }
    ],
    "items": [
      {
        "id": 1,
        "inventory_id": 5,
        "name": "4K Smart TV",
        "description": "55-inch OLED display",
        "category": "Electronics",
        "location": "Living Room",
        "purchase_date": "2023-11-20",
        "purchase_price": 1299.99,
        "warranty_expiry": "2025-11-20",
        "notes": "Black Friday purchase",
        "quantity": 1,
        "created_at": "2023-11-21T10:30:00Z",
        "updated_at": "2023-11-21T10:30:00Z"
      }
    ],
    "generated_at": "2026-02-14T15:30:45Z",
    "filters_applied": {
      "inventory_id": 5,
      "category": "Electronics",
      "format": "json"
    }
  },
  "message": "Report generated successfully",
  "error": null
}
```

**Response (CSV - 200 OK):**
```http
HTTP/1.1 200 OK
Content-Type: text/csv; charset=utf-8
Content-Disposition: attachment; filename="inventory-report-20260214-153045.csv"

ID,Inventory,Name,Description,Category,Location,Quantity,Purchase Price,Total Value,Purchase Date,Warranty Expiry,Created At
1,Home Inventory,4K Smart TV,55-inch OLED display,Electronics,Living Room,1,1299.99,1299.99,2023-11-20,2025-11-20,2023-11-21T10:30:00Z
2,Home Inventory,Laptop,Work computer,Electronics,Home Office,1,1899.00,1899.00,2024-01-10,2026-01-10,2024-01-11T14:22:00Z
```

**Error Responses:**

```json
// 401 Unauthorized - Missing or invalid token
{
  "success": false,
  "error": "Unauthorized",
  "message": "Missing or invalid authentication token"
}

// 403 Forbidden - Access denied to inventory
{
  "success": false,
  "error": "Access denied to this inventory",
  "message": null
}

// 400 Bad Request - Invalid parameters
{
  "success": false,
  "error": "Invalid date format",
  "message": "from_date must be in ISO 8601 format (YYYY-MM-DD)"
}

// 500 Internal Server Error
{
  "success": false,
  "error": "Failed to generate report",
  "message": "Database connection error"
}
```

### Endpoint 2: Get Inventory Statistics

**Method:** `GET`  
**Path:** `/api/reports/inventory/statistics`  
**Authentication:** Required

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `inventory_id` | integer | No | Specific inventory (omit for all) |

**Request Example:**
```http
GET /api/reports/inventory/statistics?inventory_id=5 HTTP/1.1
Host: localhost:8210
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Response (200 OK):**
```json
{
  "success": true,
  "data": {
    "total_items": 142,
    "total_value": 28450.75,
    "total_quantity": 256,
    "category_count": 8,
    "inventories_count": 1,
    "oldest_item_date": "2020-03-15",
    "newest_item_date": "2024-02-10",
    "average_item_value": 200.36
  },
  "message": "Statistics retrieved successfully",
  "error": null
}
```

### Endpoint 3: Get Category Breakdown

**Method:** `GET`  
**Path:** `/api/reports/inventory/categories`  
**Authentication:** Required

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `inventory_id` | integer | No | Specific inventory (omit for all) |

**Request Example:**
```http
GET /api/reports/inventory/categories HTTP/1.1
Host: localhost:8210
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Response (200 OK):**
```json
{
  "success": true,
  "data": [
    {
      "category": "Electronics",
      "item_count": 45,
      "total_quantity": 67,
      "total_value": 12500.00,
      "percentage_of_total": 43.94
    },
    {
      "category": "Furniture",
      "item_count": 28,
      "total_quantity": 45,
      "total_value": 8300.50,
      "percentage_of_total": 29.17
    },
    {
      "category": "Appliances",
      "item_count": 18,
      "total_quantity": 22,
      "total_value": 4200.00,
      "percentage_of_total": 14.76
    }
  ],
  "message": "Category breakdown retrieved successfully",
  "error": null
}
```

---

## Potential Risks and Mitigations

### Risk 1: Large Dataset Performance

**Risk:** Reports with thousands of items may cause memory issues or slow response times.

**Severity:** Medium

**Mitigation Strategies:**
1. **Implement Pagination** (Phase 2 enhancement):
   ```rust
   // Add to InventoryReportRequest
   pub limit: Option<i64>,  // Default 1000
   pub offset: Option<i64>, // Default 0
   ```

2. **Streaming CSV Output** (Phase 1):
   - Use `HttpResponse::streaming()` for CSV
   - Process items in batches from database
   - Avoid loading all items into memory

3. **Query Optimization**:
   - Leverage existing indexes
   - Use `EXPLAIN ANALYZE` to verify query plans
   - Add composite indexes if needed (see Database Query Design section)

4. **Response Size Limits**:
   - Set maximum row limit (e.g., 10,000 items)
   - Return error if result exceeds limit
   - Suggest using filters to narrow results

### Risk 2: User Authorization Bypass

**Risk:** Users might attempt to access reports for inventories they don't own.

**Severity:** High

**Mitigation Strategies:**
1. **Always Filter by user_id**:
   ```sql
   WHERE inventory_id IN (SELECT id FROM inventories WHERE user_id = $1)
   ```

2. **Explicit Access Check**:
   - Verify inventory ownership before generating report
   - Use `check_inventory_access()` helper function
   - Return 403 Forbidden if unauthorized

3. **Audit Logging**:
   - Log all report generation attempts
   - Include user_id, inventory_id, and timestamp
   - Monitor for suspicious access patterns

### Risk 3: CSV Injection Attacks

**Risk:** Malicious item names/descriptions could inject formulas into CSV files.

**Severity:** Medium

**Mitigation Strategies:**
1. **Use csv Crate** (already planned):
   - Library automatically escapes special characters
   - Handles quotes and commas properly
   - RFC 4180 compliant

2. **Input Validation**:
   - Already implemented via Validate trait on CreateItemRequest
   - Length limits prevent large payloads
   - Character restrictions on critical fields

3. **Content-Type Header**:
   - Use `text/csv` (not `application/csv`)
   - Include charset specification: `text/csv; charset=utf-8`
   - Add Content-Disposition: attachment to prevent inline rendering

### Risk 4: Invalid Date/Price Filters

**Risk:** Malformed query parameters could cause SQL errors or unexpected results.

**Severity:** Low

**Mitigation Strategies:**
1. **Type Validation**:
   ```rust
   // Validate date format before query
   if let Some(ref date_str) = request.from_date {
       chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
           .map_err(|_| "Invalid date format")?;
   }
   ```

2. **Range Validation**:
   ```rust
   if let (Some(min), Some(max)) = (request.min_price, request.max_price) {
       if min > max {
           return Err("min_price cannot exceed max_price");
       }
   }
   ```

3. **Parameterized Queries**:
   - Always use `$1, $2, ...` parameters (already standard practice)
   - Never concatenate user input into SQL strings
   - Use `escape_like_pattern()` for LIKE queries

### Risk 5: Database Connection Exhaustion

**Risk:** Simultaneous report requests could exhaust database connection pool.

**Severity:** Medium

**Mitigation Strategies:**
1. **Connection Pooling** (already implemented):
   - deadpool-postgres with configurable pool size
   - Fast recycling method for efficiency
   - Automatic connection cleanup

2. **Rate Limiting** (already implemented):
   - actix-extensible-rate-limit middleware
   - 50 req/sec sustained, 100 burst (configurable)
   - Applied to all /api/* endpoints

3. **Query Timeout**:
   ```rust
   // Add statement_timeout to query
   client.execute("SET statement_timeout = '30s'", &[]).await?;
   ```

4. **Report Queue** (future enhancement):
   - For very large reports (>10K items), queue generation
   - Return job ID, allow polling for completion
   - Store generated reports temporarily

### Risk 6: Stale Authentication Tokens

**Risk:** Expired JWT tokens could cause errors mid-report generation.

**Severity:** Low

**Mitigation Strategies:**
1. **Token Validation First**:
   - Check auth before any database operations
   - Return 401 immediately if token expired
   - Already implemented via `get_auth_context_from_request()`

2. **Short-Lived Tokens**:
   - Current implementation: configurable token lifetime
   - Standard JWT expiration handling
   - Refresh token flow already established

---

## Testing Strategy

### Unit Tests

**File:** `tests/test_db.rs`

1. **Test Report Query Building**
   ```rust
   #[tokio::test]
   async fn test_get_inventory_report_data_with_filters() {
       // Setup test database with known data
       // Execute query with various filter combinations
       // Assert correct items returned
   }
   ```

2. **Test Statistics Calculation**
   ```rust
   #[tokio::test]
   async fn test_inventory_statistics_accuracy() {
       // Insert test items with known prices and quantities
       // Calculate expected totals manually
       // Compare with function output
   }
   ```

3. **Test Category Breakdown**
   ```rust
   #[tokio::test]
   async fn test_category_breakdown_percentages() {
       // Verify percentages sum to ~100%
       // Test with NULL categories
       // Test with zero prices
   }
   ```

### Integration Tests

**File:** `tests/test_api_integration.rs`

1. **Test Report Endpoint (JSON)**
   ```rust
   #[actix_web::test]
   async fn test_inventory_report_json() {
       // Create test user and inventory
       // Make authenticated request to /api/reports/inventory
       // Assert 200 OK and valid JSON structure
       // Verify statistics accuracy
   }
   ```

2. **Test Report Endpoint (CSV)**
   ```rust
   #[actix_web::test]
   async fn test_inventory_report_csv() {
       // Create test data
       // Request CSV format
       // Parse CSV response
       // Verify header row and data rows
       // Check Content-Disposition header
   }
   ```

3. **Test Authorization**
   ```rust
   #[actix_web::test]
   async fn test_report_access_control() {
       // Create two users with separate inventories
       // User A requests User B's inventory report
       // Assert 403 Forbidden
   }
   ```

4. **Test Filters**
   ```rust
   #[actix_web::test]
   async fn test_report_filtering() {
       // Create items across date ranges, price ranges
       // Apply various filter combinations
       // Verify only matching items returned
   }
   ```

### Manual Testing Checklist

- [ ] Generate JSON report for single inventory
- [ ] Generate CSV report for all inventories
- [ ] Test each filter parameter independently
- [ ] Test multiple filters combined
- [ ] Test with empty inventory (should return empty data)
- [ ] Test with very large inventory (1000+ items)
- [ ] Verify CSV downloads correctly in browser
- [ ] Verify CSV opens correctly in Excel/LibreOffice
- [ ] Test invalid date formats (should return 400)
- [ ] Test unauthorized access (should return 403)
- [ ] Test with expired token (should return 401)
- [ ] Test sorting by each allowed field
- [ ] Verify statistics calculations manually
- [ ] Test category breakdown with uncategorized items
- [ ] Check logs for proper info/error messages

### Performance Testing

1. **Load Test Report Generation**
   ```bash
   # Use Apache Bench or similar tool
   ab -n 100 -c 10 -H "Authorization: Bearer TOKEN" \
      "http://localhost:8210/api/reports/inventory?format=json"
   ```

2. **Query Performance Profiling**
   ```sql
   EXPLAIN ANALYZE 
   SELECT ... -- Copy full query from implementation
   ```

3. **Memory Profiling**
   - Monitor RSS during large CSV generation
   - Verify streaming reduces memory footprint
   - Use Valgrind or Rust memory profilers

---

## Future Enhancements (Out of Scope)

The following features are not included in this initial implementation but may be considered for future iterations:

1. **Advanced Filtering**
   - Complex boolean expressions (AND/OR combinations)
   - Regular expression support for text fields
   - Tag-based filtering (once tags feature is implemented)

2. **Additional Export Formats**
   - PDF reports with charts/graphs
   - Excel (XLSX) format with formatting
   - HTML reports for web viewing

3. **Scheduled Reports**
   - Recurring report generation (daily, weekly, monthly)
   - Email delivery of reports
   - Stored report history

4. **Custom Report Templates**
   - User-defined column selection
   - Custom sorting and grouping
   - Saved report configurations

5. **Report Caching**
   - Cache frequently requested reports
   - Invalidate cache on data updates
   - TTL-based expiration

6. **Pagination for Large Reports**
   - Cursor-based pagination
   - Page size configuration
   - Jump to page functionality

7. **Report Analytics**
   - Track report generation frequency
   - Popular filter combinations
   - Performance metrics dashboard

8. **Visualization Integration**
   - Chart data endpoints (for frontend charts)
   - Time-series analysis
   - Trend reporting

---

## Appendix A: Related Files

Files that will be modified or created during implementation:

### Modified Files
- `src/models/mod.rs` - Add new models
- `src/db/mod.rs` - Add database service methods
- `src/api/mod.rs` - Add API endpoints
- `Cargo.toml` - Add csv dependency

### New Files (Optional)
- `src/formatters/mod.rs` - Export formatting logic (if separated)
- `src/formatters/csv.rs` - CSV-specific formatting
- `src/formatters/json.rs` - JSON-specific formatting

### Test Files
- `tests/test_reports.rs` - New test suite
- `tests/test_api_integration.rs` - Add integration tests

### Documentation Files
- `docs/API.md` - Update with new endpoints
- `README.md` - Mention reporting feature

---

## Appendix B: Example Frontend Integration

For reference, here's how the frontend might call the reporting API:

```typescript
// TypeScript example for frontend integration
interface ReportFilters {
  inventory_id?: number;
  category?: string;
  from_date?: string;
  to_date?: string;
  min_price?: number;
  max_price?: number;
  format?: 'json' | 'csv';
}

async function generateInventoryReport(filters: ReportFilters): Promise<void> {
  const queryParams = new URLSearchParams();
  
  Object.entries(filters).forEach(([key, value]) => {
    if (value !== undefined && value !== null) {
      queryParams.append(key, value.toString());
    }
  });
  
  const response = await fetch(
    `/api/reports/inventory?${queryParams.toString()}`,
    {
      headers: {
        'Authorization': `Bearer ${getAuthToken()}`,
      },
    }
  );
  
  if (filters.format === 'csv') {
    // Trigger download
    const blob = await response.blob();
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `inventory-report-${Date.now()}.csv`;
    a.click();
  } else {
    // Process JSON
    const data = await response.json();
    console.log('Report data:', data);
  }
}

// Example usage
generateInventoryReport({
  inventory_id: 5,
  from_date: '2024-01-01',
  to_date: '2024-12-31',
  format: 'csv',
});
```

---

## Appendix C: Research Source URLs

1. **Actix-Web Documentation**  
   https://actix.rs/docs/response/  
   https://docs.rs/actix-web/4.12.1/actix_web/

2. **CSV Crate Documentation**  
   https://docs.rs/csv/1.3.0/csv/  
   https://github.com/BurntSushi/rust-csv

3. **REST API Design Best Practices**  
   https://learn.microsoft.com/en-us/azure/architecture/best-practices/api-design  
   https://restfulapi.net/

4. **PostgreSQL Performance**  
   https://www.postgresql.org/docs/16/performance-tips.html  
   https://wiki.postgresql.org/wiki/Performance_Optimization

5. **RFC 6266 - Content-Disposition**  
   https://datatracker.ietf.org/doc/html/rfc6266

6. **Rust Error Handling**  
   https://doc.rust-lang.org/book/ch09-00-error-handling.html

7. **Rust Performance Book**  
   https://nnethercote.github.io/perf-book/

---

## Document Changelog

| Date | Version | Changes | Author |
|------|---------|---------|--------|
| 2026-02-14 | 1.0 | Initial specification | Research Subagent |

---

**End of Specification Document**
