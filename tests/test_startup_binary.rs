// End-to-end: start the real server binary against a database that is at migration V025 and
// check what it does at startup (backup, migrate, or refuse). Uses refinery itself to bring the
// scratch database to V025, so the server later sees a genuine migration history.

mod common;

use common::ScratchDb;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod embedded {
    refinery::embed_migrations!("migrations");
}

/// A running server process; killed when dropped.
struct Server {
    child: Child,
    stderr: Arc<Mutex<String>>,
}

impl Server {
    fn start(scratch: &ScratchDb, cwd: &Path) -> Self {
        // Any free port: the server only needs to start, nothing connects to it
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut child = Command::new(env!("CARGO_BIN_EXE_home-registry"))
            .current_dir(cwd)
            .env("DATABASE_URL", scratch.database_url())
            .env("HOST", "127.0.0.1")
            .env("PORT", port.to_string())
            .env("RUST_LOG", "info")
            .env("JWT_SECRET", "startup-test-secret-startup-test-secret")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the server binary");
        let stderr = Arc::new(Mutex::new(String::new()));
        let mut pipe = child.stderr.take().unwrap();
        let sink = Arc::clone(&stderr);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        });
        Self { child, stderr }
    }

    fn log(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }

    /// Wait until the server's log contains `needle` (it is written after the database change
    /// that tests poll for, and the reader thread adds a little delay of its own).
    async fn wait_for_log(&self, needle: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.log().contains(needle) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.log().contains(needle)
    }

    /// Wait for the process to exit on its own; `None` if it is still running at the deadline.
    fn wait_for_exit(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A scratch database brought to V025 by the real migration runner.
async fn database_at_v025() -> ScratchDb {
    let scratch = ScratchDb::create_bare().await;
    let mut client = scratch.pool.get().await.unwrap();
    embedded::migrations::runner()
        .set_target(refinery::Target::Version(25))
        .run_async(&mut **client)
        .await
        .expect("migrate to V025");
    drop(client);
    scratch
}

async fn seed_tags(scratch: &ScratchDb, count: usize) {
    let names: Vec<String> = (0..count).map(|i| format!("tag{i}")).collect();
    for name in names {
        scratch
            .pool
            .get()
            .await
            .unwrap()
            .execute("INSERT INTO tags (name) VALUES ($1)", &[&name])
            .await
            .unwrap();
    }
}

async fn migrated_to_v026(scratch: &ScratchDb) -> bool {
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .query_one(
            "SELECT to_regclass('public.tags') IS NULL
                AND EXISTS (SELECT 1 FROM refinery_schema_history WHERE version = 26)",
            &[],
        )
        .await
        .is_ok_and(|row| row.get::<_, bool>(0))
}

/// Poll until the server has applied V026, or it exits, or the timeout passes.
async fn wait_until_migrated(scratch: &ScratchDb, server: &mut Server) -> bool {
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        if migrated_to_v026(scratch).await {
            return true;
        }
        if server.child.try_wait().unwrap().is_some() {
            return migrated_to_v026(scratch).await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("hr_binary_test_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                self.0.join("backups"),
                std::fs::Permissions::from_mode(0o755),
            );
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pre_migration_backups(cwd: &Path) -> Vec<String> {
    std::fs::read_dir(cwd.join("backups"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|name| name.starts_with("home_registry_pre_migration_v026_"))
                .collect()
        })
        .unwrap_or_default()
}

#[actix_web::test]
async fn startup_with_data_in_the_old_tables_writes_the_backup_then_migrates() {
    let scratch = database_at_v025().await;
    seed_tags(&scratch, 2).await;
    let cwd = TempDir::new();

    let mut server = Server::start(&scratch, &cwd.0);
    let migrated = wait_until_migrated(&scratch, &mut server).await;
    assert!(
        migrated,
        "the server should migrate; log:\n{}",
        server.log()
    );
    server
        .wait_for_log(
            "Database migrations completed successfully",
            Duration::from_secs(10),
        )
        .await;
    let log = server.log();

    // A backup holding the rows was written first
    let backups = pre_migration_backups(&cwd.0);
    assert_eq!(backups.len(), 1, "log:\n{log}");
    let backup: home_registry::models::BackupData =
        serde_json::from_slice(&std::fs::read(cwd.0.join("backups").join(&backups[0])).unwrap())
            .unwrap();
    assert_eq!(backup.data.row_count("tags"), 2);
    assert_eq!(backup.metadata.schema_version, Some(25));

    // The counts and the file name reached the application log, before the migration ran
    let counts_at = log
        .find("tags=2")
        .unwrap_or_else(|| panic!("counts missing; log:\n{log}"));
    let wrote_at = log
        .find(&backups[0])
        .unwrap_or_else(|| panic!("file name missing; log:\n{log}"));
    let migrated_at = log
        .find("Database migrations completed successfully")
        .unwrap_or_else(|| panic!("completion missing; log:\n{log}"));
    assert!(
        counts_at < migrated_at && wrote_at < migrated_at,
        "log order:\n{log}"
    );

    // Does the SQL NOTICE reach the application log through refinery and tokio-postgres?
    assert!(
        log.contains("V026: dropping 2 rows from tags"),
        "NOTICE not found in the application log:\n{log}"
    );
}

#[actix_web::test]
async fn startup_with_empty_old_tables_just_migrates() {
    let scratch = database_at_v025().await;
    let cwd = TempDir::new();

    let mut server = Server::start(&scratch, &cwd.0);
    let migrated = wait_until_migrated(&scratch, &mut server).await;
    let log = server.log();
    assert!(migrated, "the server should migrate; log:\n{log}");
    assert!(
        pre_migration_backups(&cwd.0).is_empty(),
        "no backup is needed; log:\n{log}"
    );
    assert!(!log.contains("will delete data"), "{log}");
    assert!(
        !log.contains("V026: dropping"),
        "no rows, no notice:\n{log}"
    );
}

#[cfg(unix)]
#[actix_web::test]
async fn startup_refuses_to_migrate_when_the_backup_cannot_be_written() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = database_at_v025().await;
    seed_tags(&scratch, 1).await;
    let cwd = TempDir::new();
    let backups = cwd.0.join("backups");
    std::fs::create_dir_all(&backups).unwrap();
    std::fs::set_permissions(&backups, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(backups.join("probe"), b"x").is_ok() {
        eprintln!("skipping: directory permissions are not enforced (running as root?)");
        return;
    }

    let mut server = Server::start(&scratch, &cwd.0);
    let status = server
        .wait_for_exit(Duration::from_secs(60))
        .expect("the server must exit instead of running");
    assert!(!status.success(), "exit status: {status:?}");
    // Give the stderr reader a moment to drain the last lines
    tokio::time::sleep(Duration::from_millis(300)).await;
    let log = server.log();
    assert!(log.contains("Refusing to start"), "log:\n{log}");

    // The database was not touched: the tables and the migration state are as they were
    assert!(!migrated_to_v026(&scratch).await);
    let rows: i64 = scratch
        .pool
        .get()
        .await
        .unwrap()
        .query_one("SELECT COUNT(*) FROM tags", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(rows, 1);
}
