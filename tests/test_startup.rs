// The safety net that runs before migration V026: if the tables it drops hold data, a backup
// is written first, and startup must not proceed when that backup cannot be written.

mod common;

use common::ScratchDb;
use home_registry::api::backup::is_valid_backup_filename;
use home_registry::models::BackupData;
use home_registry::startup::{find_legacy_data, protect_legacy_data_before_migration};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

// ---------------------------------------------------------------- log capture

struct CaptureLogger;

static LOGS: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        LOGS.lock()
            .unwrap()
            .push((record.level(), record.args().to_string()));
    }
    fn flush(&self) {}
}

fn install_logger() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&CaptureLogger).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Info);
    });
}

/// Tests that read the captured log run one at a time so they do not see each other's lines.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn take_logs() -> Vec<(log::Level, String)> {
    std::mem::take(&mut *LOGS.lock().unwrap())
}

// ---------------------------------------------------------------- helpers

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("hr_startup_test_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn backup_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

async fn user_and_item(pool: &deadpool_postgres::Pool) -> i32 {
    let username = common::test_username("startup");
    common::create_test_user(pool, &username).await;
    let user_id: uuid::Uuid = pool
        .get()
        .await
        .unwrap()
        .query_one("SELECT id FROM users WHERE username = $1", &[&username])
        .await
        .unwrap()
        .get(0);
    let inventory = common::create_test_inventory(pool, user_id, "Startup")
        .await
        .unwrap();
    common::create_test_item(pool, inventory, "Thing")
        .await
        .unwrap()
}

async fn exec(pool: &deadpool_postgres::Pool, sql: &str) {
    pool.get().await.unwrap().batch_execute(sql).await.unwrap();
}

// ---------------------------------------------------------------- tests

#[actix_web::test]
async fn data_in_the_tables_to_be_dropped_is_backed_up_and_logged_before_migrating() {
    install_logger();
    let _serial = SERIAL.lock().await;
    let scratch = ScratchDb::create_up_to(25).await;
    let item = user_and_item(&scratch.pool).await;
    exec(
        &scratch.pool,
        "INSERT INTO tags (name) VALUES ('red'), ('blue');
         INSERT INTO categories (name) VALUES ('Tools');",
    )
    .await;
    exec(
        &scratch.pool,
        &format!(
            "UPDATE items SET category_id = (SELECT id FROM categories LIMIT 1) WHERE id = {item}"
        ),
    )
    .await;
    let dir = TempDir::new();
    let backups = dir.path().join("backups");
    take_logs();

    let name = protect_legacy_data_before_migration(&scratch.pool, &backups)
        .await
        .expect("backup is written")
        .expect("there was data to protect");

    // A recognisable, valid, loadable backup
    assert!(
        name.starts_with("home_registry_pre_migration_v026_"),
        "{name}"
    );
    assert!(is_valid_backup_filename(&name), "{name}");
    let backup: BackupData =
        serde_json::from_slice(&std::fs::read(backups.join(&name)).unwrap()).unwrap();
    assert_eq!(backup.metadata.version, "2.0");
    assert_eq!(
        backup.metadata.schema_version,
        Some(25),
        "taken before V026"
    );
    assert_eq!(backup.data.row_count("tags"), 2);
    assert_eq!(backup.data.row_count("categories"), 1);
    assert_eq!(backup_files(&backups), vec![name.clone()]);

    // Counts and the file name were logged at WARN
    let logs = take_logs();
    let warns: Vec<&String> = logs
        .iter()
        .filter(|(level, _)| *level == log::Level::Warn)
        .map(|(_, message)| message)
        .collect();
    let counts = warns
        .iter()
        .find(|m| m.contains("will delete data"))
        .expect("a WARN with the counts");
    assert!(counts.contains("categories=1"), "{counts}");
    assert!(counts.contains("tags=2"), "{counts}");
    assert!(counts.contains("items.category_id=1"), "{counts}");
    assert!(
        warns.iter().any(|m| m.contains(&name)),
        "a WARN naming the backup file: {warns:?}"
    );

    // Nothing was migrated: this only protects the data
    let still_there = find_legacy_data(&scratch.pool).await.unwrap();
    assert_eq!(still_there.items_with_category_id, 1);
}

#[actix_web::test]
async fn empty_tables_need_no_backup() {
    install_logger();
    let _serial = SERIAL.lock().await;
    let scratch = ScratchDb::create_up_to(25).await;
    user_and_item(&scratch.pool).await;
    let dir = TempDir::new();
    let backups = dir.path().join("backups");
    take_logs();

    let result = protect_legacy_data_before_migration(&scratch.pool, &backups).await;
    assert_eq!(result, Ok(None));
    assert_eq!(backup_files(&backups).len(), 0);
    assert!(
        !take_logs()
            .iter()
            .any(|(level, _)| *level == log::Level::Warn),
        "nothing to warn about"
    );
}

#[actix_web::test]
async fn a_single_row_in_any_one_table_is_enough() {
    install_logger();
    let _serial = SERIAL.lock().await;
    for (table, sql) in [
        ("categories", "INSERT INTO categories (name) VALUES ('c')"),
        ("tags", "INSERT INTO tags (name) VALUES ('t')"),
        (
            "password_reset_tokens",
            "INSERT INTO password_reset_tokens (user_id, token)
             SELECT id, 'tok' FROM users LIMIT 1",
        ),
    ] {
        let scratch = ScratchDb::create_up_to(25).await;
        user_and_item(&scratch.pool).await;
        exec(&scratch.pool, sql).await;
        let dir = TempDir::new();

        let name = protect_legacy_data_before_migration(&scratch.pool, dir.path())
            .await
            .unwrap_or_else(|e| panic!("{table}: {e}"));
        assert!(name.is_some(), "{table} alone must trigger a backup");
        assert_eq!(backup_files(dir.path()).len(), 1, "{table}");
    }
}

#[actix_web::test]
async fn a_database_that_is_already_migrated_or_brand_new_is_left_alone() {
    install_logger();
    let _serial = SERIAL.lock().await;
    let dir = TempDir::new();

    // Already at V026: the tables are gone
    let migrated = ScratchDb::create().await;
    assert_eq!(
        protect_legacy_data_before_migration(&migrated.pool, dir.path()).await,
        Ok(None)
    );

    // Nothing created yet (first start on an empty database)
    let bare = ScratchDb::create_bare().await;
    assert_eq!(
        protect_legacy_data_before_migration(&bare.pool, dir.path()).await,
        Ok(None)
    );

    // Even if V026 is recorded as applied, leftover rows are not this check's business
    let odd = ScratchDb::create_up_to(25).await;
    user_and_item(&odd.pool).await;
    exec(&odd.pool, "INSERT INTO tags (name) VALUES ('t')").await;
    exec(
        &odd.pool,
        "INSERT INTO refinery_schema_history (version, name, applied_on, checksum)
         VALUES (26, 'x', 'x', 'x')",
    )
    .await;
    assert_eq!(
        protect_legacy_data_before_migration(&odd.pool, dir.path()).await,
        Ok(None)
    );
    assert_eq!(backup_files(dir.path()).len(), 0);
}

#[cfg(unix)]
#[actix_web::test]
async fn if_the_backup_cannot_be_written_the_caller_is_told_not_to_proceed() {
    use std::os::unix::fs::PermissionsExt;

    install_logger();
    let _serial = SERIAL.lock().await;
    let scratch = ScratchDb::create_up_to(25).await;
    user_and_item(&scratch.pool).await;
    exec(&scratch.pool, "INSERT INTO tags (name) VALUES ('red')").await;
    let dir = TempDir::new();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(dir.path().join("probe"), b"x").is_ok() {
        eprintln!("skipping: directory permissions are not enforced (running as root?)");
        return;
    }
    take_logs();

    let err = protect_legacy_data_before_migration(&scratch.pool, dir.path())
        .await
        .expect_err("no backup, no migration");
    assert!(err.contains("V026"), "{err}");
    assert!(err.contains("tags=1"), "{err}");
    assert!(
        take_logs()
            .iter()
            .any(|(level, _)| *level == log::Level::Error),
        "the failure is logged as an error"
    );
    // The data is untouched
    assert_eq!(
        find_legacy_data(&scratch.pool).await.unwrap().tables.len(),
        1
    );
}

#[actix_web::test]
async fn legacy_data_is_counted_per_table() {
    let scratch = ScratchDb::create_up_to(25).await;
    user_and_item(&scratch.pool).await;
    assert!(find_legacy_data(&scratch.pool).await.unwrap().is_empty());

    exec(
        &scratch.pool,
        "INSERT INTO tags (name) VALUES ('a'), ('b'), ('c');
         INSERT INTO categories (name) VALUES ('x');",
    )
    .await;
    let found = find_legacy_data(&scratch.pool).await.unwrap();
    assert_eq!(
        found.tables,
        vec![("categories".to_string(), 1), ("tags".to_string(), 3)]
    );
    assert_eq!(found.summary(), "categories=1, tags=3");
}
