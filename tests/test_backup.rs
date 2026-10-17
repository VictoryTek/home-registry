// Backup export and restore. Restore truncates every table, so each test works in its own
// scratch database (see tests/common::ScratchDb) and never touches the shared test database.

mod common;

use actix_web::{http::StatusCode, test, web, App};
use common::ScratchDb;
use home_registry::api;
use home_registry::api::backup::{
    create_backup_file, restore_backup_file, store_uploaded_backup, RestoreError, UploadError,
};
use home_registry::auth::totp::totp_key_fingerprint;
use home_registry::db::backup::{export_tables, topological_order};
use home_registry::db::DatabaseService;
use home_registry::models::{
    BackupData, BackupDatabaseContent, CreateOrganizerOptionRequest, CreateOrganizerTypeRequest,
    PermissionLevel, RestoreRequest,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

// ---------------------------------------------------------------- helpers

/// A fresh temporary directory for backup files
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("hr_backup_test_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Restore permissions first so a read-only directory can be removed
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every table's rows as JSON, sorted, for comparing database states. `token_version` is left
/// out of `users`: a restore deliberately raises it.
async fn snapshot(pool: &deadpool_postgres::Pool) -> BTreeMap<String, Vec<Value>> {
    let client = pool.get().await.unwrap();
    let tables: Vec<String> = client
        .query(
            "SELECT table_name::text FROM information_schema.tables
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
               AND table_name <> 'refinery_schema_history' ORDER BY 1",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    let mut out = BTreeMap::new();
    for table in tables {
        let mut rows: Vec<Value> = client
            .query(
                format!("SELECT to_jsonb(t) FROM public.\"{table}\" t").as_str(),
                &[],
            )
            .await
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        if table == "users" {
            for row in &mut rows {
                row.as_object_mut().unwrap().remove("token_version");
            }
        }
        rows.sort_by_key(ToString::to_string);
        out.insert(table, rows);
    }
    out
}

struct Seed {
    owner: Uuid,
    totp_user: Uuid,
    totp_username: String,
    viewer: Uuid,
}

/// Rows in every table that exists today, including 2FA enrolment.
async fn seed(pool: &deadpool_postgres::Pool) -> Seed {
    let db = DatabaseService::new(pool.clone());
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for label in ["owner", "totp", "viewer"] {
        let name = common::test_username(label);
        common::create_test_user(pool, &name).await;
        ids.push(db.get_user_by_username(&name).await.unwrap().unwrap().id);
        names.push(name);
    }
    let (owner, totp_user, viewer) = (ids[0], ids[1], ids[2]);

    let inv_a = common::create_test_inventory(pool, owner, "Garage")
        .await
        .unwrap();
    let inv_b = common::create_test_inventory(pool, totp_user, "Office")
        .await
        .unwrap();
    let item_a = common::create_test_item(pool, inv_a, "Drill")
        .await
        .unwrap();
    let item_b = common::create_test_item(pool, inv_a, "Saw").await.unwrap();
    common::create_test_item(pool, inv_b, "Monitor")
        .await
        .unwrap();

    let org = db
        .create_organizer_type(
            inv_a,
            CreateOrganizerTypeRequest {
                name: "Room".into(),
                input_type: Some("select".into()),
                is_required: None,
                display_order: None,
            },
        )
        .await
        .unwrap();
    let org_id = org.id.unwrap();
    let option = db
        .create_organizer_option(
            org_id,
            CreateOrganizerOptionRequest {
                name: "Workshop".into(),
                display_order: Some(1),
            },
        )
        .await
        .unwrap();
    let client = pool.get().await.unwrap();
    client
        .execute(
            "INSERT INTO item_organizer_values (item_id, organizer_type_id, organizer_option_id)
             VALUES ($1, $2, $3)",
            &[&item_a, &org_id, &option.id.unwrap()],
        )
        .await
        .unwrap();
    let _ = item_b;

    db.create_inventory_share(inv_a, viewer, owner, PermissionLevel::View)
        .await
        .unwrap();
    db.create_user_access_grant(owner, totp_user).await.unwrap();
    db.store_recovery_codes(owner, vec!["h1".into(), "h2".into()])
        .await
        .unwrap();
    db.get_or_create_user_settings(owner).await.unwrap();
    db.create_totp_settings(totp_user, "encrypted-secret-bytes")
        .await
        .unwrap();
    db.enable_totp(totp_user, "2fa_only").await.unwrap();

    // Tables that H-09 will drop still exist and must be carried along
    client
        .batch_execute(
            "INSERT INTO categories (name) VALUES ('cat');
             INSERT INTO tags (name) VALUES ('tag');",
        )
        .await
        .unwrap();

    Seed {
        owner,
        totp_user,
        totp_username: names[1].clone(),
        viewer,
    }
}

async fn read_backup(dir: &Path, name: &str) -> BackupData {
    serde_json::from_slice(&tokio::fs::read(dir.join(name)).await.unwrap()).unwrap()
}

/// Write `backup` into `dir` under a freshly generated valid backup name
fn write_backup(dir: &Path, backup: &BackupData) -> String {
    let name = home_registry::api::backup::generate_backup_filename("home_registry");
    std::fs::write(dir.join(&name), serde_json::to_vec_pretty(backup).unwrap()).unwrap();
    name
}

/// The text form of every `type_zoo` row, including identity and generated columns
async fn render_zoo(client: &deadpool_postgres::Object) -> Vec<String> {
    client
        .query("SELECT z::text FROM type_zoo z ORDER BY id", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get::<_, String>(0))
        .collect()
}

async fn row_count(pool: &deadpool_postgres::Pool, table: &str) -> i64 {
    pool.get()
        .await
        .unwrap()
        .query_one(
            format!("SELECT COUNT(*) FROM public.\"{table}\"").as_str(),
            &[],
        )
        .await
        .unwrap()
        .get(0)
}

// ---------------------------------------------------------------- export

#[actix_web::test]
async fn export_covers_exactly_the_catalog_tables() {
    let scratch = ScratchDb::create().await;
    seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());

    let export = db.export_all_data().await.unwrap();
    let keys: Vec<&str> = export.tables.0.keys().map(String::as_str).collect();

    // 2FA enrolment is included (the table the old fixed list forgot)
    assert!(keys.contains(&"user_totp_settings"));
    assert_eq!(export.tables.row_count("user_totp_settings"), 1);
    // Derived objects and migration bookkeeping are not
    assert!(
        !keys.contains(&"inventory_access"),
        "views must not be exported"
    );
    assert!(!keys.contains(&"refinery_schema_history"));
    // Everything in the catalog is there
    let catalog: Vec<String> = scratch
        .pool
        .get()
        .await
        .unwrap()
        .query(
            "SELECT table_name::text FROM information_schema.tables
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
               AND table_name <> 'refinery_schema_history' ORDER BY 1",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(keys, catalog);
    // The schema version comes from the same export
    assert_eq!(export.schema_version, Some(scratch.schema_version));

    // For the tables in use the rows are exactly what the old per-table query produced
    // (`to_jsonb(row)`), so backups written before and after this change have the same shape
    let client = scratch.pool.get().await.unwrap();
    for table in &catalog {
        let old_style: Value = client
            .query_one(
                format!(
                    "SELECT COALESCE(jsonb_agg(to_jsonb(x)), '[]'::jsonb) FROM public.\"{table}\" x"
                )
                .as_str(),
                &[],
            )
            .await
            .unwrap()
            .get(0);
        let sorted = |v: &Value| {
            let mut rows: Vec<String> = v
                .as_array()
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect();
            rows.sort();
            rows
        };
        assert_eq!(
            sorted(&export.tables.0[table]),
            sorted(&old_style),
            "table {table}"
        );
    }
}

#[actix_web::test]
async fn export_reads_only_through_the_snapshot_it_is_given() {
    let scratch = ScratchDb::create().await;
    let db = DatabaseService::new(scratch.pool.clone());
    common::create_test_user(&scratch.pool, &common::test_username("before")).await;

    let mut reader = scratch.pool.get().await.unwrap();
    let tx = reader
        .build_transaction()
        .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .await
        .unwrap();
    // The snapshot is taken by the first query
    tx.query_one("SELECT COUNT(*) FROM users", &[])
        .await
        .unwrap();

    // A write committed from another connection after the snapshot ...
    common::create_test_user(&scratch.pool, &common::test_username("after")).await;

    // ... is invisible to an export through that transaction, but not to a new one
    let snapshot_export = export_tables(&tx).await.unwrap();
    assert_eq!(snapshot_export.tables.row_count("users"), 1);
    tx.commit().await.unwrap();
    assert_eq!(
        db.export_all_data()
            .await
            .unwrap()
            .tables
            .row_count("users"),
        2
    );
}

// ---------------------------------------------------------------- round trip

#[actix_web::test]
async fn a_restore_brings_back_every_table_and_resets_the_sequences() {
    let scratch = ScratchDb::create().await;
    let seeded = seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());
    let before = snapshot(&scratch.pool).await;
    assert_ne!(before["user_totp_settings"].len(), 0);
    assert_ne!(before["item_organizer_values"].len(), 0);

    let export = db.export_all_data().await.unwrap();

    // Wreck the database: delete everything, add unrelated data
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .batch_execute("TRUNCATE users CASCADE")
        .await
        .unwrap();
    common::create_test_user(&scratch.pool, &common::test_username("intruder")).await;

    let report = db.import_all_data(&export.tables, &[]).await.unwrap();
    assert!(report.sessions_ended);
    assert!(report.rows_restored > 0);
    assert_eq!(snapshot(&scratch.pool).await, before);

    // Sequences were reset past the restored rows: new rows get fresh ids
    let client = scratch.pool.get().await.unwrap();
    let max_inventory: i32 = client
        .query_one("SELECT MAX(id) FROM inventories", &[])
        .await
        .unwrap()
        .get(0);
    let new_inventory = common::create_test_inventory(&scratch.pool, seeded.viewer, "New")
        .await
        .unwrap();
    assert!(new_inventory > max_inventory);
    let new_item = common::create_test_item(&scratch.pool, new_inventory, "New item").await;
    assert!(new_item.is_ok());
    // 2FA enrolment survived the restore
    assert_eq!(row_count(&scratch.pool, "user_totp_settings").await, 1);
    let _ = seeded.owner;
}

#[actix_web::test]
async fn every_column_type_survives_a_round_trip() {
    let scratch = ScratchDb::create().await;
    let db = DatabaseService::new(scratch.pool.clone());
    let client = scratch.pool.get().await.unwrap();
    client
        .batch_execute(
            "CREATE TYPE mood AS ENUM ('sad', 'ok', 'happy');
             CREATE TABLE type_zoo (
                 id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                 b boolean, s2 smallint, i4 integer, i8 bigint,
                 n numeric(30,10), f4 real, f8 double precision,
                 t text, -- a column named like the export's row alias must not confuse it
                 vc varchar(20), d date, tm time, ts timestamp, tstz timestamptz,
                 iv interval, u uuid, j jsonb, jj json, by bytea,
                 ta text[], ia integer[], e mood, ip inet,
                 doubled bigint GENERATED ALWAYS AS (i4::bigint * 2) STORED
             );
             INSERT INTO type_zoo (b, s2, i4, i8, n, f4, f8, t, vc, d, tm, ts, tstz, iv, u, j, jj,
                                   by, ta, ia, e, ip)
             VALUES
               (true, -32768, 2147483647, 9223372036854775807, 12345678901234567890.1234567890,
                0.1, 0.1, E'quote '' backslash \\\\ newline \\n unicode \u{e9}\u{4e2d}\u{1f600}',
                'varchar', '2024-02-29'::date - 1, '23:59:59.999999', '2026-10-17 12:34:56.123456',
                '2026-10-17 12:34:56.123456+05:30', '1 year 2 mons 3 days 04:05:06.789',
                'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', '{\"k\": [1, 2, {\"x\": null}]}',
                '{\"a\": 1}', '\\x00ff10', ARRAY['a', 'b c', 'd\"e', NULL, ''],
                ARRAY[1, NULL, 3], 'happy', '10.0.0.1/24'),
               (false, NULL, -1, NULL, -0.0000000001, 'NaN', 'Infinity',
                NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, '{}', NULL, NULL, NULL, NULL,
                'sad', NULL),
               (NULL, 0, 0, 0, 0, 1e-30, -1e300, '', '', '4713-01-01 BC'::date, '00:00:00',
                '1970-01-01', 'infinity', '-1 days', NULL, '[]', '[]', '', ARRAY[]::text[],
                ARRAY[]::integer[], 'ok', '::1');",
        )
        .await
        .unwrap();

    let before = render_zoo(&client).await;
    assert_eq!(before.len(), 3);

    let export = db.export_all_data().await.unwrap();
    // Identity ids are part of the data and must come back unchanged
    client
        .batch_execute("TRUNCATE type_zoo RESTART IDENTITY")
        .await
        .unwrap();
    db.import_all_data(&export.tables, &[]).await.unwrap();
    assert_eq!(render_zoo(&client).await, before);

    // The identity sequence continues after the restored ids
    client
        .batch_execute("INSERT INTO type_zoo (i4) VALUES (7)")
        .await
        .unwrap();
    let max_id: i32 = client
        .query_one("SELECT MAX(id) FROM type_zoo", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(max_id, 4);
}

#[actix_web::test]
async fn numeric_values_keep_every_digit_and_json_null_is_the_one_known_exception() {
    let scratch = ScratchDb::create().await;
    let db = DatabaseService::new(scratch.pool.clone());
    let client = scratch.pool.get().await.unwrap();
    client
        .batch_execute(
            "CREATE TABLE precise (id integer PRIMARY KEY, n numeric, na numeric[], j jsonb);
             INSERT INTO precise VALUES
               (1, 123456789012345678901234567890.123456789012345678901234567890,
                ARRAY[0.1, 99999999999999999999.99999999999999999999, NULL],
                '{\"a\": 1}'),
               (2, NULL, NULL, NULL),
               (3, 'NaN', ARRAY[]::numeric[], 'null'::jsonb);",
        )
        .await
        .unwrap();
    let render = || async {
        client
            .query("SELECT p::text FROM precise p ORDER BY id", &[])
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<_, String>(0))
            .collect::<Vec<_>>()
    };
    let before = render().await;

    let export = db.export_all_data().await.unwrap();
    db.import_all_data(&export.tables, &[]).await.unwrap();
    let after = render().await;

    // Numbers far beyond what a 64-bit float holds come back digit for digit
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[1]);
    assert!(after[0].contains("123456789012345678901234567890.123456789012345678901234567890"));
    // Known limit: JSON cannot tell the JSON literal `null` in a jsonb column from SQL NULL
    assert!(before[2].ends_with(",null)"));
    assert!(
        after[2].ends_with(",)"),
        "restored as SQL NULL: {}",
        after[2]
    );
}

#[actix_web::test]
async fn a_child_row_before_its_parent_restores_in_a_self_referencing_table() {
    let scratch = ScratchDb::create().await;
    let db = DatabaseService::new(scratch.pool.clone());
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .batch_execute(
            "CREATE TABLE selfref_test (
                 id integer PRIMARY KEY,
                 parent_id integer REFERENCES selfref_test (id),
                 name text
             )",
        )
        .await
        .unwrap();

    // Child (2, parent 1) is listed before its parent (1)
    let mut tables = BTreeMap::new();
    tables.insert(
        "selfref_test".to_string(),
        json!([
            {"id": 3, "parent_id": 2, "name": "grandchild"},
            {"id": 2, "parent_id": 1, "name": "child"},
            {"id": 1, "parent_id": null, "name": "root"}
        ]),
    );
    db.import_all_data(&BackupDatabaseContent(tables), &[])
        .await
        .expect("self-referencing rows restore regardless of order");
    assert_eq!(row_count(&scratch.pool, "selfref_test").await, 3);
}

#[actix_web::test]
async fn tables_that_reference_each_other_in_a_cycle_are_refused_before_anything_changes() {
    let scratch = ScratchDb::create().await;
    seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .batch_execute(
            "CREATE TABLE cyc_a (id integer PRIMARY KEY, b_id integer);
             CREATE TABLE cyc_b (id integer PRIMARY KEY, a_id integer REFERENCES cyc_a (id));
             ALTER TABLE cyc_a ADD FOREIGN KEY (b_id) REFERENCES cyc_b (id);",
        )
        .await
        .unwrap();
    let before = snapshot(&scratch.pool).await;
    let export = db.export_all_data().await.unwrap();

    let err = db.import_all_data(&export.tables, &[]).await.unwrap_err();
    assert!(err.to_string().contains("cycle"), "{err}");
    assert!(err.to_string().contains("cyc_a"), "{err}");
    assert_eq!(snapshot(&scratch.pool).await, before, "nothing may change");

    // The pure ordering agrees
    assert!(topological_order(
        &["cyc_a".into(), "cyc_b".into()],
        &[
            ("cyc_a".into(), "cyc_b".into()),
            ("cyc_b".into(), "cyc_a".into())
        ]
    )
    .is_err());
}

#[actix_web::test]
async fn a_failing_restore_rolls_back_and_leaves_the_data_untouched() {
    let scratch = ScratchDb::create().await;
    seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());
    let before = snapshot(&scratch.pool).await;
    let mut export = db.export_all_data().await.unwrap();

    // An item pointing at an inventory that is not in the backup breaks a foreign key
    export.tables.0.insert(
        "items".into(),
        json!([{ "id": 999, "inventory_id": 424_242, "name": "orphan", "quantity": 1 }]),
    );
    let err = db.import_all_data(&export.tables, &[]).await.unwrap_err();
    assert!(err.to_string().contains("items"), "{err}");
    assert_eq!(snapshot(&scratch.pool).await, before);
}

// ---------------------------------------------------------------- old and new formats

const LEGACY_BACKUP: &str = r#"{
  "metadata": {
    "version": "1.0",
    "app_version": "0.1.0-beta.1",
    "created_at": "2025-01-01T00:00:00+00:00",
    "database_type": "postgresql",
    "description": null
  },
  "data": {
    "users": [
      {"id": "11111111-1111-1111-1111-111111111111", "username": "legacy_admin",
       "full_name": "Legacy Admin", "password_hash": "x", "is_admin": true, "is_active": true,
       "created_at": "2025-01-01T00:00:00+00:00", "updated_at": "2025-01-01T00:00:00+00:00",
       "recovery_codes_generated_at": null, "recovery_codes_confirmed": false,
       "nickname": "a column that no longer exists"}
    ],
    "inventories": [
      {"id": 1, "name": "Old", "description": null, "location": null, "image_url": null,
       "user_id": "11111111-1111-1111-1111-111111111111",
       "created_at": "2025-01-01T00:00:00+00:00", "updated_at": "2025-01-01T00:00:00+00:00"}
    ],
    "items": [],
    "legacy_widgets": [{"id": 1}]
  }
}"#;

#[actix_web::test]
async fn a_legacy_1_0_backup_still_restores_with_warnings() {
    let scratch = ScratchDb::create().await;
    seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();

    let backup: BackupData = serde_json::from_str(LEGACY_BACKUP).expect("1.0 format still parses");
    assert!(backup.metadata.schema_version.is_none());
    let name = write_backup(dir.path(), &backup);

    let report = restore_backup_file(&db, dir.path(), &name, &RestoreRequest::default())
        .await
        .expect("legacy backup restores");
    assert_eq!(report.tables_restored, 3); // users, inventories and items are in the backup
    let warnings = report.warnings.join("\n");
    assert!(warnings.contains("'legacy_widgets'"), "{warnings}");
    assert!(warnings.contains("'nickname'"), "{warnings}");
    assert!(warnings.contains("'user_totp_settings'"), "{warnings}");

    // Columns added after the backup took their defaults; sessions were ended (0 -> 1)
    let client = scratch.pool.get().await.unwrap();
    let row = client
        .query_one(
            "SELECT username::text, token_version FROM users WHERE username = 'legacy_admin'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "legacy_admin");
    assert_eq!(row.get::<_, i32>(1), 1);
    assert_eq!(row_count(&scratch.pool, "users").await, 1);
    assert_eq!(row_count(&scratch.pool, "user_totp_settings").await, 0);
}

#[actix_web::test]
async fn a_backup_from_a_newer_schema_is_refused_without_changing_anything() {
    let scratch = ScratchDb::create().await;
    seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();
    let before = snapshot(&scratch.pool).await;

    let mut backup: BackupData = serde_json::from_str(LEGACY_BACKUP).unwrap();
    backup.metadata.version = "2.0".into();
    backup.metadata.schema_version = Some(scratch.schema_version + 1);
    let name = write_backup(dir.path(), &backup);

    match restore_backup_file(&db, dir.path(), &name, &RestoreRequest::default()).await {
        Err(RestoreError::NewerThanDatabase { backup, database }) => {
            assert_eq!(
                (backup, database),
                (scratch.schema_version + 1, scratch.schema_version)
            );
        },
        other => panic!("expected NewerThanDatabase, got {other:?}"),
    }
    assert_eq!(snapshot(&scratch.pool).await, before);
    // Not even a safety backup was written
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

    // The same backup at the current schema version restores
    backup.metadata.schema_version = Some(scratch.schema_version);
    let ok_name = write_backup(dir.path(), &backup);
    assert!(
        restore_backup_file(&db, dir.path(), &ok_name, &RestoreRequest::default())
            .await
            .is_ok()
    );
}

// ---------------------------------------------------------------- sessions

#[actix_web::test]
async fn a_restore_ends_every_session_even_for_tokens_that_were_already_revoked() {
    let scratch = ScratchDb::create().await;
    home_registry::auth::get_or_init_jwt_secret();
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();

    let known = common::test_username("known");
    common::create_test_user(&scratch.pool, &known).await;
    let known_id = db.get_user_by_username(&known).await.unwrap().unwrap().id;

    // The backup has `known` at generation 2 and a user that only exists in the backup at 3
    let only_in_backup = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let user_row = |id: Uuid, name: &str, tv: i32| {
        json!({
            "id": id, "username": name, "full_name": "X", "password_hash": "x",
            "is_admin": false, "is_active": true, "created_at": now, "updated_at": now,
            "recovery_codes_generated_at": null, "recovery_codes_confirmed": false,
            "token_version": tv
        })
    };
    let mut backup: BackupData = serde_json::from_str(LEGACY_BACKUP).unwrap();
    backup.metadata.version = "2.0".into();
    backup.data.0.remove("legacy_widgets");
    backup.data.0.remove("inventories");
    backup.data.0.insert(
        "users".into(),
        json!([
            user_row(known_id, &known, 2),
            user_row(only_in_backup, "only_in_backup", 3)
        ]),
    );
    let name = write_backup(dir.path(), &backup);

    // Live state: `known` has meanwhile advanced to generation 5 (a token was revoked)
    let client = scratch.pool.get().await.unwrap();
    client
        .execute(
            "UPDATE users SET token_version = 5 WHERE id = $1",
            &[&known_id],
        )
        .await
        .unwrap();
    let token_at_5 = home_registry::auth::issue_token(known_id, &known, false, 5).unwrap();
    let token_at_2 = home_registry::auth::issue_token(known_id, &known, false, 2).unwrap();

    restore_backup_file(&db, dir.path(), &name, &RestoreRequest::default())
        .await
        .unwrap();

    let version = |id: Uuid| {
        let client = &client;
        async move {
            client
                .query_one("SELECT token_version FROM users WHERE id = $1", &[&id])
                .await
                .unwrap()
                .get::<_, i32>(0)
        }
    };
    // max(restored 2, current 5) + 1, and restored + 1 for the backup-only user
    assert_eq!(version(known_id).await, 6);
    assert_eq!(version(only_in_backup).await, 4);

    // Neither the live token nor the revoked token from before the backup works any more
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(scratch.pool.clone()))
            .service(web::scope("/api").service(api::auth::get_current_user)),
    )
    .await;
    for token in [token_at_5, token_at_2] {
        let req = test::TestRequest::get()
            .uri("/api/auth/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["error"], "session_revoked");
    }
}

// ---------------------------------------------------------------- two-factor key

/// A backup of the seeded database with the TOTP fingerprint replaced
async fn backup_with_fingerprint(
    db: &DatabaseService,
    dir: &Path,
    fingerprint: Option<&str>,
) -> String {
    let info = create_backup_file(db, "home_registry", dir).await.unwrap();
    let mut backup = read_backup(dir, &info.name).await;
    backup.metadata.totp_key_fingerprint = fingerprint.map(ToString::to_string);
    std::fs::remove_file(dir.join(&info.name)).unwrap();
    write_backup(dir, &backup)
}

#[actix_web::test]
async fn a_backup_records_the_key_fingerprint_only_when_it_holds_2fa_enrolment() {
    let scratch = ScratchDb::create().await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();

    let empty = create_backup_file(&db, "home_registry", dir.path())
        .await
        .unwrap();
    assert!(read_backup(dir.path(), &empty.name)
        .await
        .metadata
        .totp_key_fingerprint
        .is_none());

    seed(&scratch.pool).await;
    let full = create_backup_file(&db, "home_registry", dir.path())
        .await
        .unwrap();
    let meta = read_backup(dir.path(), &full.name).await.metadata;
    assert_eq!(meta.totp_key_fingerprint, Some(totp_key_fingerprint()));
    assert_eq!(meta.version, "2.0");
    assert_eq!(meta.schema_version, Some(scratch.schema_version));
}

#[actix_web::test]
async fn a_different_totp_key_refuses_by_default_and_the_override_restores_without_2fa() {
    let scratch = ScratchDb::create().await;
    home_registry::auth::get_or_init_jwt_secret();
    let seeded = seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();
    let name = backup_with_fingerprint(&db, dir.path(), Some("deadbeefdeadbeef")).await;
    let before = snapshot(&scratch.pool).await;

    // Default: refused, nothing changed, no safety backup written
    let files_before = std::fs::read_dir(dir.path()).unwrap().count();
    match restore_backup_file(&db, dir.path(), &name, &RestoreRequest::default()).await {
        Err(RestoreError::TotpKeyMismatch { users }) => assert_eq!(users, 1),
        other => panic!("expected TotpKeyMismatch, got {other:?}"),
    }
    assert_eq!(snapshot(&scratch.pool).await, before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), files_before);

    // Override: everything except 2FA is restored and the count is reported
    let report = restore_backup_file(
        &db,
        dir.path(),
        &name,
        &RestoreRequest {
            restore_without_2fa: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(report.two_factor_cleared_users, Some(1));
    assert_eq!(row_count(&scratch.pool, "user_totp_settings").await, 0);
    assert_eq!(row_count(&scratch.pool, "users").await, 3);
    assert!(row_count(&scratch.pool, "items").await > 0);

    // A user who had mode 2fa_only is not locked out: with no enrolment row, login needs only
    // the password (there is no server-wide "2FA required" rule that could send them elsewhere)
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(scratch.pool.clone()))
            .service(web::scope("/api").service(api::auth::login)),
    )
    .await;
    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .peer_addr("10.55.0.1:4000".parse().unwrap())
        .set_json(json!({ "username": seeded.totp_username, "password": common::test_password() }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["token"].is_string());
    assert!(body["data"]["requires_totp"].is_null());
    let _ = seeded.totp_user;
}

#[actix_web::test]
async fn a_matching_key_restores_2fa_and_a_missing_fingerprint_with_rows_is_refused() {
    let scratch = ScratchDb::create().await;
    seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();

    let matching = backup_with_fingerprint(&db, dir.path(), Some(&totp_key_fingerprint())).await;
    let report = restore_backup_file(&db, dir.path(), &matching, &RestoreRequest::default())
        .await
        .unwrap();
    assert_eq!(report.two_factor_cleared_users, None);
    assert_eq!(row_count(&scratch.pool, "user_totp_settings").await, 1);

    // Rows without a fingerprint cannot be verified, so they are treated as a mismatch
    let unverifiable = backup_with_fingerprint(&db, dir.path(), None).await;
    assert!(matches!(
        restore_backup_file(&db, dir.path(), &unverifiable, &RestoreRequest::default()).await,
        Err(RestoreError::TotpKeyMismatch { .. })
    ));
}

#[actix_web::test]
async fn an_empty_totp_table_never_triggers_the_key_check() {
    let scratch = ScratchDb::create().await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();
    common::create_test_user(&scratch.pool, &common::test_username("plain")).await;

    // Different fingerprint recorded, but no 2FA rows in the backup
    let name = backup_with_fingerprint(&db, dir.path(), Some("deadbeefdeadbeef")).await;
    let backup = read_backup(dir.path(), &name).await;
    assert_eq!(backup.data.row_count("user_totp_settings"), 0);
    restore_backup_file(&db, dir.path(), &name, &RestoreRequest::default())
        .await
        .expect("no TOTP rows, so no key check");
}

// ---------------------------------------------------------------- safety backup

#[cfg(unix)]
#[actix_web::test]
async fn if_the_safety_backup_cannot_be_written_the_restore_does_not_start() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = ScratchDb::create().await;
    seed(&scratch.pool).await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();
    let name = backup_with_fingerprint(&db, dir.path(), Some(&totp_key_fingerprint())).await;
    let before = snapshot(&scratch.pool).await;

    // Make the directory read-only: the backup can still be read, but nothing can be written
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(dir.path().join("probe"), b"x").is_ok() {
        eprintln!("skipping: directory permissions are not enforced (running as root?)");
        return;
    }

    let result = restore_backup_file(&db, dir.path(), &name, &RestoreRequest::default()).await;
    assert!(
        matches!(result, Err(RestoreError::SafetyBackupFailed(_))),
        "{result:?}"
    );
    assert_eq!(
        snapshot(&scratch.pool).await,
        before,
        "the restore must not have started"
    );
}

// ---------------------------------------------------------------- files

#[actix_web::test]
async fn only_generated_backup_names_are_valid() {
    use home_registry::api::backup::{generate_backup_filename, is_valid_backup_filename};

    for valid in [
        "home_registry_2026.10.17.12.00.00.json",
        "home_registry_auto_pre_restore_2026.10.17.12.00.00.json",
        "home_registry_2026.10.17.12.00.00.123_0a1b2c3d.json",
        "home_registry_auto_pre_restore_2026.10.17.12.00.00.123_0a1b2c3d.json",
        "home_registry_2026.10.17.12.00.00_2.json",
    ] {
        assert!(is_valid_backup_filename(valid), "{valid}");
    }
    for invalid in [
        "",
        "backup.json",
        "home_registry_2026.10.17.12.00.00.txt",
        "home_registry_2026.10.17.12.00.00",
        "../home_registry_2026.10.17.12.00.00.json",
        "home_registry_../2026.10.17.12.00.00.json",
        "home_registry_2026.10.17.12.00.00/../../x.json",
        "home_registry_2026.10.17.12.00.00.json/",
        "home_registry_2026.10.17.12.00.00\\x.json",
        "home_registry_mybackup.json",
        "home_registry_2026.10.17.12.00.json",
        "home_registry_2026.10.17.12.00.00.123_0A1B2C3D.json",
        "home_registry_2026.10.17.12.00.00.123_0a1b2c.json",
        "home_registry_2026.10.17.12.00.00_1234.json",
        "home_registry_2026.10.17.12.00.00_.json",
        "home_registry_auto_2026.10.17.12.00.00.json",
        "home_registry_2026.10.17.12.00.00.json\0",
        "HOME_REGISTRY_2026.10.17.12.00.00.json",
    ] {
        assert!(!is_valid_backup_filename(invalid), "{invalid:?}");
    }

    // Every generated name is valid, and two generated at once differ
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..2000 {
        let name = generate_backup_filename("home_registry");
        assert!(is_valid_backup_filename(&name), "{name}");
        assert!(seen.insert(name), "duplicate generated name");
    }
    assert!(is_valid_backup_filename(&generate_backup_filename(
        "home_registry_auto_pre_restore"
    )));
}

#[actix_web::test]
async fn two_backups_in_the_same_instant_never_overwrite_each_other() {
    let scratch = ScratchDb::create().await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = TempDir::new();

    let mut names = std::collections::BTreeSet::new();
    for _ in 0..8 {
        let info = create_backup_file(&db, "home_registry", dir.path())
            .await
            .unwrap();
        assert!(names.insert(info.name));
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 8);
}

#[actix_web::test]
async fn uploads_are_validated_first_and_never_overwrite() {
    let dir = TempDir::new();
    let good = serde_json::to_vec(&serde_json::from_str::<Value>(LEGACY_BACKUP).unwrap()).unwrap();

    // Not a backup: rejected and nothing written
    assert!(matches!(
        store_uploaded_backup(
            dir.path(),
            "home_registry_2026.10.17.12.00.00.json",
            b"{\"hello\": 1}"
        )
        .await,
        Err(UploadError::NotABackup)
    ));
    assert!(matches!(
        store_uploaded_backup(dir.path(), "x.json", b"not json at all").await,
        Err(UploadError::NotABackup)
    ));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

    // Unknown format version
    let mut future: Value = serde_json::from_slice(&good).unwrap();
    future["metadata"]["version"] = json!("9.9");
    assert!(matches!(
        store_uploaded_backup(dir.path(), "x.json", &serde_json::to_vec(&future).unwrap()).await,
        Err(UploadError::UnsupportedFormat(v)) if v == "9.9"
    ));

    // A valid name is kept; storing the same name again must not overwrite it
    let wanted = "home_registry_2026.10.17.12.00.00.json";
    let (first, _) = store_uploaded_backup(dir.path(), wanted, &good)
        .await
        .unwrap();
    assert_eq!(first.name, wanted);
    let original = std::fs::read(dir.path().join(wanted)).unwrap();
    let mut other: Value = serde_json::from_slice(&good).unwrap();
    other["metadata"]["description"] = json!("second upload");
    let (second, _) =
        store_uploaded_backup(dir.path(), wanted, &serde_json::to_vec(&other).unwrap())
            .await
            .unwrap();
    assert_ne!(second.name, wanted);
    assert_eq!(std::fs::read(dir.path().join(wanted)).unwrap(), original);

    // An invalid name is replaced by a generated one (never used as a path)
    let (third, _) = store_uploaded_backup(dir.path(), "../../etc/passwd.json", &good)
        .await
        .unwrap();
    assert!(home_registry::api::backup::is_valid_backup_filename(
        &third.name
    ));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
}
