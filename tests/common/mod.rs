// Common test utilities

use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use std::env;
use tokio_postgres::NoTls;

/// Connection settings for the test database server, pointing at `dbname`.
fn test_db_config(dbname: Option<&str>) -> Config {
    let database_url = env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/home_inventory_test".to_string()
        })
    });

    let mut cfg = Config::new();
    let parts: Vec<&str> = database_url
        .trim_start_matches("postgres://")
        .split('@')
        .collect();

    if parts.len() == 2 {
        let user_pass: Vec<&str> = parts[0].split(':').collect();
        if user_pass.len() == 2 {
            cfg.user = Some(user_pass[0].to_string());
            cfg.password = Some(user_pass[1].to_string());
        }

        let host_db: Vec<&str> = parts[1].split('/').collect();
        if host_db.len() == 2 {
            let host_port: Vec<&str> = host_db[0].split(':').collect();
            if host_port.len() == 2 {
                cfg.host = Some(host_port[0].to_string());
                cfg.port = Some(host_port[1].parse().unwrap_or(5432));
            }
            cfg.dbname = Some(host_db[1].to_string());
        }
    }
    if let Some(dbname) = dbname {
        cfg.dbname = Some(dbname.to_string());
    }

    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg
}

/// Create a test database pool
/// Uses `TEST_DATABASE_URL` env var if set, otherwise falls back to default test DB
#[allow(dead_code)]
pub fn create_test_pool() -> Pool {
    test_db_config(None)
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("Failed to create test pool")
}

/// A database that exists only for one test.
///
/// Restore truncates every table, so tests of it must never run against the shared test
/// database. This creates `home_registry_scratch_<random>` on the same server, applies every
/// migration from `migrations/` (plus the migration bookkeeping table, with each version
/// recorded), and drops the database again when the value goes out of scope, even if the test
/// panics.
#[allow(dead_code)]
pub struct ScratchDb {
    pub name: String,
    pub pool: Pool,
    /// Highest migration version applied
    pub schema_version: i64,
}

/// Migration files from `migrations/`, sorted by version: (version, name, path).
#[allow(dead_code)]
fn migration_files() -> Vec<(i32, String, std::path::PathBuf)> {
    let mut files: Vec<_> = std::fs::read_dir("migrations")
        .expect("migrations directory")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .map(|path| {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let (version, name) = stem
                .trim_start_matches('V')
                .split_once("__")
                .expect("migration file name");
            (
                version.parse().expect("migration version"),
                name.to_string(),
                path,
            )
        })
        .collect();
    files.sort_by_key(|(version, _, _)| *version);
    files
}

#[allow(dead_code)]
impl ScratchDb {
    /// A scratch database with every migration applied.
    pub async fn create() -> Self {
        Self::create_up_to(i32::MAX).await
    }

    /// A scratch database with no tables at all (not even migration bookkeeping), for tests
    /// that let the real migration runner build the schema.
    pub async fn create_bare() -> Self {
        let name = format!("home_registry_scratch_{}", uuid::Uuid::new_v4().simple());
        let admin = test_db_config(Some("postgres"))
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .expect("admin pool");
        admin
            .get()
            .await
            .expect("admin connection")
            .batch_execute(&format!("CREATE DATABASE {name}"))
            .await
            .expect("create scratch database");

        let pool = test_db_config(Some(&name))
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .expect("scratch pool");
        Self {
            name,
            pool,
            schema_version: 0,
        }
    }

    /// `DATABASE_URL` that points a server process at this scratch database.
    pub fn database_url(&self) -> String {
        let base = env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/home_inventory_test".to_string()
        });
        let (server, _) = base
            .rsplit_once('/')
            .expect("database url has a database name");
        format!("{server}/{}", self.name)
    }

    /// A scratch database migrated only up to and including `max_version`, for testing a
    /// migration against the state just before it. Apply later ones with `apply_migration`.
    pub async fn create_up_to(max_version: i32) -> Self {
        let mut scratch = Self::create_bare().await;
        // Same bookkeeping table the migration runner keeps
        scratch
            .pool
            .get()
            .await
            .expect("scratch connection")
            .batch_execute(
                "CREATE TABLE refinery_schema_history (
                     version INT4 PRIMARY KEY, name VARCHAR(255),
                     applied_on VARCHAR(255), checksum VARCHAR(255))",
            )
            .await
            .expect("bookkeeping table");
        for (version, _, _) in migration_files() {
            if version <= max_version {
                scratch.apply_migration(version).await;
            }
        }
        scratch
    }

    /// Apply one migration file and record it in the bookkeeping table.
    pub async fn apply_migration(&mut self, version: i32) {
        let (_, name, path) = migration_files()
            .into_iter()
            .find(|(v, _, _)| *v == version)
            .unwrap_or_else(|| panic!("no migration V{version}"));
        let sql = std::fs::read_to_string(&path).expect("read migration");
        let client = self.pool.get().await.expect("scratch connection");
        client
            .batch_execute(&sql)
            .await
            .unwrap_or_else(|e| panic!("applying V{version}: {e}"));
        client
            .execute(
                "INSERT INTO refinery_schema_history (version, name, applied_on, checksum)
                 VALUES ($1, $2, 'test', 'test')",
                &[&version, &name],
            )
            .await
            .expect("record migration");
        self.schema_version = i64::from(version);
    }
}

impl Drop for ScratchDb {
    fn drop(&mut self) {
        // Runs on its own thread and runtime so it works while the test's runtime is shutting
        // down or unwinding from a panic. FORCE disconnects any pooled connections.
        let name = self.name.clone();
        let _ = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async {
                let Ok(admin) =
                    test_db_config(Some("postgres")).create_pool(Some(Runtime::Tokio1), NoTls)
                else {
                    return;
                };
                if let Ok(client) = admin.get().await {
                    let _ = client
                        .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
                        .await;
                }
            });
        })
        .join();
    }
}

/// Generate a unique test username
/// Ensures the username is under 50 characters to fit DB constraint
#[allow(dead_code)]
pub fn test_username(prefix: &str) -> String {
    // Use first 8 characters of UUID to keep total length under 50
    let short_id = uuid::Uuid::new_v4()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>();
    // Truncate prefix if needed to ensure total is under 50 chars
    let max_prefix_len = 40; // Leave room for underscore and short_id
    let truncated_prefix = if prefix.len() > max_prefix_len {
        &prefix[..max_prefix_len]
    } else {
        prefix
    };
    format!("{truncated_prefix}_{short_id}")
}

/// Generate a test password
#[allow(dead_code)]
pub fn test_password() -> String {
    "TestPassword123!".to_string()
}

/// Create a test user and return their credentials
#[allow(dead_code)]
pub async fn create_test_user(pool: &Pool, username: &str) -> (String, String) {
    use home_registry::db::DatabaseService;

    let password = test_password();
    let password_hash = home_registry::auth::hash_password(password.clone())
        .await
        .expect("Failed to hash password");

    let db = DatabaseService::new(pool.clone());
    let user = db
        .create_user(
            username,
            "Test User",    // full_name
            &password_hash, // password_hash
            false,          // is_admin
            true,           // is_active (must be true!)
        )
        .await
        .expect("Failed to create test user");

    (user.username, password)
}

/// Get a JWT token for a test user
#[allow(dead_code)]
pub async fn get_test_token(pool: &Pool, username: &str) -> String {
    use home_registry::auth::{generate_token, get_or_init_jwt_secret};
    use home_registry::db::DatabaseService;

    // Initialize JWT secret (critical for tests)
    get_or_init_jwt_secret();

    let db = DatabaseService::new(pool.clone());
    let user = db
        .get_user_by_username(username)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    generate_token(&user).expect("Failed to create token")
}

/// Set how `POST /api/auth/register` behaves (`open`, `invite` or `closed`). New installs and
/// upgrades both default to `invite`, so tests that register users without an invitation open it
/// first. Changing the shared test database's mode affects every test that uses it, so tests of the
/// other modes use their own scratch database.
#[allow(dead_code)]
pub async fn set_registration_mode(pool: &Pool, mode: &str) {
    pool.get()
        .await
        .expect("connection")
        .execute(
            "INSERT INTO app_settings (key, value) VALUES ('registration_mode', $1)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            &[&mode],
        )
        .await
        .expect("set registration mode");
}

/// Create an admin test user and return their credentials
#[allow(dead_code)]
pub async fn create_admin_user(pool: &Pool, username: &str) -> (String, String) {
    use home_registry::db::DatabaseService;

    let password = test_password();
    let password_hash = home_registry::auth::hash_password(password.clone())
        .await
        .expect("Failed to hash password");

    let db = DatabaseService::new(pool.clone());
    let user = db
        .create_user(
            username,
            "Test Admin",   // full_name
            &password_hash, // password_hash
            true,           // is_admin
            true,           // is_active (must be true!)
        )
        .await
        .expect("Failed to create admin user");

    (user.username, password)
}

/// Delete a test user by username
#[allow(dead_code)]
pub async fn delete_test_user(
    pool: &Pool,
    username: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use home_registry::db::DatabaseService;

    let db = DatabaseService::new(pool.clone());
    if let Some(user) = db.get_user_by_username(username).await? {
        db.delete_user(user.id).await?;
    }
    Ok(())
}

/// Create a test inventory and return its ID
#[allow(dead_code)]
pub async fn create_test_inventory(
    pool: &Pool,
    user_id: uuid::Uuid,
    name: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    use home_registry::db::DatabaseService;
    use home_registry::models::CreateInventoryRequest;

    let db = DatabaseService::new(pool.clone());
    let request = CreateInventoryRequest {
        name: name.to_string(),
        description: Some("Test inventory description".to_string()),
        location: Some("Test location".to_string()),
        image_url: None,
    };

    let inventory = db.create_inventory(request, user_id).await?;

    Ok(inventory.id.expect("Inventory should have ID"))
}

/// Delete a test inventory by ID
#[allow(dead_code)]
pub async fn delete_test_inventory(pool: &Pool, id: i32) -> Result<(), Box<dyn std::error::Error>> {
    use home_registry::db::DatabaseService;

    let db = DatabaseService::new(pool.clone());
    db.delete_inventory(id).await?;
    Ok(())
}

/// Create a test item and return its ID
#[allow(dead_code)]
pub async fn create_test_item(
    pool: &Pool,
    inventory_id: i32,
    name: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    use home_registry::db::DatabaseService;
    use home_registry::models::CreateItemRequest;

    let db = DatabaseService::new(pool.clone());
    let request = CreateItemRequest {
        inventory_id,
        name: name.to_string(),
        description: Some("Test item description".to_string()),
        category: None,
        location: None,
        purchase_date: None,
        purchase_price: None,
        warranty_expiry: None,
        notes: None,
        quantity: Some(1),
    };

    let item = db.create_item(request).await?;

    Ok(item.id.expect("Item should have ID"))
}

/// Delete a test item by ID
#[allow(dead_code)]
pub async fn delete_test_item(pool: &Pool, id: i32) -> Result<(), Box<dyn std::error::Error>> {
    use home_registry::db::DatabaseService;

    let db = DatabaseService::new(pool.clone());
    db.delete_item(id).await?;
    Ok(())
}

/// Cleanup all test data for a given username prefix
#[allow(dead_code)]
pub async fn cleanup_test_data(
    pool: &Pool,
    username_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use home_registry::db::DatabaseService;

    let db = DatabaseService::new(pool.clone());

    // Find and delete all test users matching the prefix
    let conn = pool.get().await?;
    let stmt = conn
        .prepare("SELECT id FROM users WHERE username LIKE $1")
        .await?;
    let rows = conn.query(&stmt, &[&format!("{username_prefix}%")]).await?;

    for row in rows {
        let user_id: uuid::Uuid = row.get(0);
        db.delete_user(user_id).await.ok(); // Ignore errors
    }

    Ok(())
}
