//! Database export and restore for backups.
//!
//! Which tables exist is read from the catalog, never from a hand-kept list, so a new
//! migration's table is backed up and restored automatically. Export runs in a single
//! `REPEATABLE READ` read-only transaction (one snapshot for every table). Restore runs in one
//! transaction: truncate everything, insert in foreign-key order, reset sequences, end all
//! sessions; any error rolls the whole restore back.

use std::collections::{BTreeMap, BTreeSet};

use log::{info, warn};
use tokio_postgres::{IsolationLevel, Transaction};
use uuid::Uuid;

use super::DatabaseService;
use crate::models::{BackupDatabaseContent, RestoreReport};

type BoxError = Box<dyn std::error::Error>;

/// Tables that are never exported or restored.
///
/// `refinery_schema_history` is migration bookkeeping that must stay in step with the schema,
/// not with a backup. Add anything else that must never be restored here, with a comment saying
/// why (for example a future session denylist).
const BACKUP_EXCLUDED_TABLES: &[&str] = &["refinery_schema_history"];

/// A consistent export of the database
#[derive(Debug)]
pub struct BackupExport {
    pub tables: BackupDatabaseContent,
    /// Latest applied migration at the time of the export (same snapshot as the data)
    pub schema_version: Option<i64>,
}

/// Quote an identifier for use in SQL.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Order `tables` so every table comes after the tables its foreign keys point to.
///
/// `edges` are `(child, parent)` pairs. Self-references are ignored: all of a table's rows are
/// inserted by one statement, and non-deferrable foreign keys are checked at the end of the
/// statement, so row order inside a table does not matter. Edges to tables outside `tables`
/// are ignored. A genuine cycle between tables cannot be restored and is an error.
pub fn topological_order(
    tables: &[String],
    edges: &[(String, String)],
) -> Result<Vec<String>, String> {
    let known: BTreeSet<&str> = tables.iter().map(String::as_str).collect();
    let mut parents: BTreeMap<&str, BTreeSet<&str>> = tables
        .iter()
        .map(|t| (t.as_str(), BTreeSet::new()))
        .collect();
    for (child, parent) in edges {
        if child != parent && known.contains(child.as_str()) && known.contains(parent.as_str()) {
            parents
                .get_mut(child.as_str())
                .expect("known table")
                .insert(parent.as_str());
        }
    }

    let mut ordered: Vec<String> = Vec::with_capacity(tables.len());
    let mut placed: BTreeSet<&str> = BTreeSet::new();
    while ordered.len() < tables.len() {
        // Deterministic: among the tables whose parents are placed, take them in name order
        let ready: Vec<&str> = parents
            .iter()
            .filter(|(table, deps)| {
                !placed.contains(*table) && deps.iter().all(|d| placed.contains(d))
            })
            .map(|(table, _)| *table)
            .collect();
        if ready.is_empty() {
            let stuck: Vec<&str> = parents
                .keys()
                .copied()
                .filter(|t| !placed.contains(t))
                .collect();
            return Err(format!(
                "foreign keys form a cycle between tables: {}",
                stuck.join(", ")
            ));
        }
        for table in ready {
            placed.insert(table);
            ordered.push(table.to_string());
        }
    }
    Ok(ordered)
}

/// Base tables in `public` that take part in backups, sorted by name.
async fn catalog_tables(tx: &Transaction<'_>) -> Result<Vec<String>, BoxError> {
    // Base tables only: views such as inventory_access are derived and must not be exported
    let rows = tx
        .query(
            "SELECT table_name::text FROM information_schema.tables
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
             ORDER BY table_name",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| r.get::<_, String>(0))
        .filter(|name| !BACKUP_EXCLUDED_TABLES.contains(&name.as_str()))
        .collect())
}

/// Foreign-key edges `(child, parent)` between tables in `public`.
async fn foreign_key_edges(tx: &Transaction<'_>) -> Result<Vec<(String, String)>, BoxError> {
    let rows = tx
        .query(
            "SELECT child.relname::text, parent.relname::text
             FROM pg_constraint con
             JOIN pg_class child ON child.oid = con.conrelid
             JOIN pg_class parent ON parent.oid = con.confrelid
             JOIN pg_namespace n ON n.oid = child.relnamespace
             WHERE con.contype = 'f' AND n.nspname = 'public'",
            &[],
        )
        .await?;
    Ok(rows.iter().map(|r| (r.get(0), r.get(1))).collect())
}

/// Latest applied migration, if the bookkeeping table exists.
async fn schema_version_in(tx: &Transaction<'_>) -> Result<Option<i64>, BoxError> {
    let exists: bool = tx
        .query_one(
            "SELECT to_regclass('public.refinery_schema_history') IS NOT NULL",
            &[],
        )
        .await?
        .get(0);
    if !exists {
        return Ok(None);
    }
    let version: Option<i64> = tx
        .query_one(
            "SELECT MAX(version)::bigint FROM public.refinery_schema_history",
            &[],
        )
        .await?
        .get(0);
    Ok(version)
}

/// Export every backup table through `tx`.
///
/// Reads only through the transaction it is given, so when that transaction holds a
/// repeatable-read snapshot every table is exported from the same moment.
pub async fn export_tables(tx: &Transaction<'_>) -> Result<BackupExport, BoxError> {
    let tables = catalog_tables(tx).await?;
    let mut content = BTreeMap::new();
    for table in &tables {
        let query = export_query(tx, table).await?;
        let rows: serde_json::Value = tx.query_one(query.as_str(), &[]).await?.get(0);
        content.insert(table.clone(), rows);
    }
    Ok(BackupExport {
        tables: BackupDatabaseContent(content),
        schema_version: schema_version_in(tx).await?,
    })
}

/// The query that exports one table as a JSON array of row objects.
///
/// Values go through `to_jsonb`, which matches what restore feeds to
/// `jsonb_populate_recordset`. `numeric` columns are exported as text instead: a JSON number is
/// read as a 64-bit float on the way through Rust, which would silently round large or precise
/// values. Postgres parses the text back exactly.
///
/// Known limit: a `jsonb` column holding the JSON literal `null` comes back as SQL `NULL`,
/// because JSON cannot tell the two apart. The only jsonb column
/// (`user_settings.settings_json`) treats both as "no settings".
async fn export_query(tx: &Transaction<'_>, table: &str) -> Result<String, BoxError> {
    let regclass = format!("public.{}", quote_ident(table));
    let numeric_columns: Vec<String> = tx
        .query(
            "SELECT attname::text FROM pg_attribute
             WHERE attrelid = $1::text::regclass AND attnum > 0 AND NOT attisdropped
               AND atttypid IN ('numeric'::regtype, 'numeric[]'::regtype)
             ORDER BY attnum",
            &[&regclass],
        )
        .await?
        .iter()
        .map(|r| r.get(0))
        .collect();

    // `t.*` is the whole row; a bare `t` would pick a column named "t" if there is one
    let mut row = "to_jsonb(t.*)".to_string();
    if !numeric_columns.is_empty() {
        let pairs = numeric_columns
            .iter()
            .map(|c| format!("'{}', t.{}::text", c.replace('\'', "''"), quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        row = format!("{row} || jsonb_build_object({pairs})");
    }
    Ok(format!(
        "SELECT COALESCE(jsonb_agg({row}), '[]'::jsonb) FROM {regclass} t"
    ))
}

/// A column of a table as seen by restore.
struct Column {
    name: String,
    /// `GENERATED ALWAYS AS (...) STORED`: computed by the database, never inserted
    generated: bool,
    /// `GENERATED ALWAYS AS IDENTITY`: inserting needs `OVERRIDING SYSTEM VALUE`
    identity_always: bool,
}

async fn table_columns(tx: &Transaction<'_>, table: &str) -> Result<Vec<Column>, BoxError> {
    let regclass = format!("public.{}", quote_ident(table));
    let rows = tx
        .query(
            "SELECT attname::text, attgenerated::text, attidentity::text
             FROM pg_attribute
             WHERE attrelid = $1::text::regclass AND attnum > 0 AND NOT attisdropped
             ORDER BY attnum",
            &[&regclass],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let generated: String = r.get(1);
            let identity: String = r.get(2);
            Column {
                name: r.get(0),
                generated: !generated.is_empty(),
                identity_always: identity == "a",
            }
        })
        .collect())
}

/// `INSERT` statement for one table's rows (bound as `$1`, a JSON array).
///
/// Only columns present in the backup are listed, so columns added by later migrations take
/// their defaults, and Postgres itself casts each JSON value to the column's type.
fn insert_statement(table: &str, columns: &[&Column]) -> String {
    let names = columns
        .iter()
        .map(|c| quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let overriding = if columns.iter().any(|c| c.identity_always) {
        " OVERRIDING SYSTEM VALUE"
    } else {
        ""
    };
    let table = quote_ident(table);
    format!(
        "INSERT INTO public.{table} ({names}){overriding}
         SELECT {names} FROM jsonb_populate_recordset(NULL::public.{table}, $1)"
    )
}

impl DatabaseService {
    /// Latest applied migration (`None` when the bookkeeping table does not exist).
    pub async fn current_schema_version(&self) -> Result<Option<i64>, BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let version = schema_version_in(&tx).await?;
        tx.commit().await?;
        Ok(version)
    }

    /// Export all database tables for a backup, from one consistent snapshot.
    pub async fn export_all_data(&self) -> Result<BackupExport, BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .await?;
        let export = export_tables(&tx).await?;
        tx.commit().await?;
        info!(
            "Exported {} tables for backup (schema version {:?})",
            export.tables.0.len(),
            export.schema_version
        );
        Ok(export)
    }

    /// Replace the whole database with a backup, atomically.
    ///
    /// `skip_tables` are emptied but not restored from the backup (used to leave out 2FA
    /// enrolment when its encryption key does not match). Every user's `token_version` is
    /// raised past both the restored and the pre-restore value, which ends all sessions, so a
    /// restore cannot revive a token that had been revoked.
    pub async fn import_all_data(
        &self,
        data: &BackupDatabaseContent,
        skip_tables: &[String],
    ) -> Result<RestoreReport, BoxError> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let mut report = RestoreReport::default();

        let db_tables = catalog_tables(&tx).await?;
        let order = topological_order(&db_tables, &foreign_key_edges(&tx).await?)?;

        // Tables in the backup that this database no longer has (dropped by a later migration)
        for (name, rows) in &data.0 {
            if BACKUP_EXCLUDED_TABLES.contains(&name.as_str()) || db_tables.contains(name) {
                continue;
            }
            // Only worth reporting when the backup actually held data for it
            let count = rows.as_array().map_or(0, Vec::len);
            if count > 0 {
                report.warnings.push(format!(
                    "Skipped {count} {} from '{name}', a table that no longer exists in this version",
                    if count == 1 { "row" } else { "rows" }
                ));
            }
        }
        // Tables this database has that the backup does not: they end up empty
        for table in &db_tables {
            if !data.0.contains_key(table) && !skip_tables.contains(table) {
                report.warnings.push(format!(
                    "The backup has no data for table '{table}'; it was left empty"
                ));
            }
        }

        // Take every table's lock first, so nothing can change a session generation (or write to
        // the database) between reading the previous generations below and the TRUNCATE. The
        // restore is rare and TRUNCATE needs these locks anyway.
        if !db_tables.is_empty() {
            let list = db_tables
                .iter()
                .map(|t| format!("public.{}", quote_ident(t)))
                .collect::<Vec<_>>()
                .join(", ");
            tx.batch_execute(&format!("LOCK TABLE {list} IN ACCESS EXCLUSIVE MODE"))
                .await?;
        }

        // Remember each user's current session generation before the tables are emptied
        let previous: Vec<(Uuid, i32)> = tx
            .query("SELECT id, token_version FROM users", &[])
            .await?
            .iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect();

        // One statement empties every table; no CASCADE is needed because all are listed
        if !db_tables.is_empty() {
            let list = db_tables
                .iter()
                .map(|t| format!("public.{}", quote_ident(t)))
                .collect::<Vec<_>>()
                .join(", ");
            tx.execute(
                format!("TRUNCATE TABLE {list} RESTART IDENTITY").as_str(),
                &[],
            )
            .await?;
        }

        for table in &order {
            if skip_tables.contains(table) {
                continue;
            }
            let Some(rows) = data.0.get(table) else {
                continue;
            };
            let Some(rows_array) = rows.as_array() else {
                return Err(
                    format!("backup data for table '{table}' is not a list of rows").into(),
                );
            };
            if !rows_array.iter().all(serde_json::Value::is_object) {
                return Err(format!(
                    "backup data for table '{table}' has rows that are not objects"
                )
                .into());
            }
            report.tables_restored += 1;
            let Some(first) = rows_array.first().and_then(serde_json::Value::as_object) else {
                continue; // empty table: nothing to insert
            };

            let columns = table_columns(&tx, table).await?;
            let mut insert_columns: Vec<&Column> = Vec::new();
            for column in &columns {
                if !column.generated && first.contains_key(&column.name) {
                    insert_columns.push(column);
                }
            }
            // Backup columns the table no longer has (dropped by a later migration). Only
            // reported when some row had a value for it; an all-NULL column lost nothing.
            for key in first.keys() {
                if columns.iter().any(|c| &c.name == key) {
                    continue;
                }
                let with_value = rows_array
                    .iter()
                    .filter(|row| row.get(key).is_some_and(|v| !v.is_null()))
                    .count();
                if with_value > 0 {
                    report.warnings.push(format!(
                        "Skipped {with_value} {} of column '{key}' in '{table}', a column that no longer exists in this version",
                        if with_value == 1 { "value" } else { "values" }
                    ));
                }
            }
            if insert_columns.is_empty() {
                return Err(format!(
                    "none of the backup columns for table '{table}' exist in the database"
                )
                .into());
            }

            let statement = insert_statement(table, &insert_columns);
            tx.execute(statement.as_str(), &[rows])
                .await
                .map_err(|e| format!("restoring table '{table}' failed: {e}"))?;
            report.rows_restored += rows_array.len() as u64;
        }

        // Point every serial/identity sequence past the restored rows. The sequences come from
        // the catalog, so none of these statements can fail for a missing sequence.
        let sequences = tx
            .query(
                "SELECT c.relname::text, a.attname::text,
                        pg_get_serial_sequence(format('public.%I', c.relname), a.attname)
                 FROM pg_class c
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 JOIN pg_attribute a ON a.attrelid = c.oid
                 WHERE n.nspname = 'public' AND c.relkind = 'r'
                   AND a.attnum > 0 AND NOT a.attisdropped
                   AND pg_get_serial_sequence(format('public.%I', c.relname), a.attname) IS NOT NULL",
                &[],
            )
            .await?;
        for row in &sequences {
            let table: String = row.get(0);
            let column: String = row.get(1);
            let sequence: String = row.get(2);
            if BACKUP_EXCLUDED_TABLES.contains(&table.as_str()) {
                continue;
            }
            let statement = format!(
                "SELECT setval($1::text::regclass,
                        COALESCE((SELECT MAX({col})::bigint FROM public.{tbl}), 0) + 1, false)",
                col = quote_ident(&column),
                tbl = quote_ident(&table)
            );
            tx.query_one(statement.as_str(), &[&sequence]).await?;
        }

        // End every session: past both the restored and the previous generation
        let (ids, versions): (Vec<Uuid>, Vec<i32>) = previous.into_iter().unzip();
        tx.execute(
            "UPDATE users u
             SET token_version = GREATEST(
                     u.token_version,
                     COALESCE((SELECT p.v FROM unnest($1::uuid[], $2::int4[]) AS p(id, v)
                               WHERE p.id = u.id), u.token_version)
                 ) + 1",
            &[&ids, &versions],
        )
        .await?;
        report.sessions_ended = true;

        tx.commit().await?;
        if !report.warnings.is_empty() {
            warn!("Restore finished with warnings: {:?}", report.warnings);
        }
        info!(
            "Restored {} tables ({} rows) from backup",
            report.tables_restored, report.rows_restored
        );
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    fn edges(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
            .collect()
    }

    #[test]
    fn parents_come_before_children() {
        let order = topological_order(
            &names(&["items", "users", "inventories"]),
            &edges(&[("items", "inventories"), ("inventories", "users")]),
        )
        .unwrap();
        assert_eq!(order, ["users", "inventories", "items"]);
    }

    #[test]
    fn order_is_deterministic_for_independent_tables() {
        let order = topological_order(&names(&["c", "a", "b"]), &[]).unwrap();
        assert_eq!(order, ["a", "b", "c"]);
    }

    #[test]
    fn self_references_are_not_cycles() {
        let order = topological_order(
            &names(&["tree", "users"]),
            &edges(&[("tree", "tree"), ("tree", "users")]),
        )
        .unwrap();
        assert_eq!(order, ["users", "tree"]);
    }

    #[test]
    fn a_real_cycle_between_tables_is_an_error() {
        let err = topological_order(&names(&["a", "b", "c"]), &edges(&[("a", "b"), ("b", "a")]))
            .unwrap_err();
        // Only the tables on the cycle are named, not the unrelated "c"
        assert!(err.ends_with(": a, b"), "{err}");
    }

    #[test]
    fn edges_to_unknown_tables_are_ignored() {
        let order =
            topological_order(&names(&["a"]), &edges(&[("a", "refinery_schema_history")])).unwrap();
        assert_eq!(order, ["a"]);
    }

    #[test]
    fn identifiers_are_quoted() {
        assert_eq!(quote_ident("users"), "\"users\"");
        assert_eq!(quote_ident("we\"ird"), "\"we\"\"ird\"");
    }

    #[test]
    fn insert_lists_only_backup_columns_and_overrides_identity() {
        let id = Column {
            name: "id".into(),
            generated: false,
            identity_always: true,
        };
        let name = Column {
            name: "name".into(),
            generated: false,
            identity_always: false,
        };
        let sql = insert_statement("things", &[&id, &name]);
        assert!(
            sql.contains("(\"id\", \"name\") OVERRIDING SYSTEM VALUE"),
            "{sql}"
        );
        assert!(
            sql.contains("jsonb_populate_recordset(NULL::public.\"things\", $1)"),
            "{sql}"
        );

        let sql = insert_statement("things", &[&name]);
        assert!(!sql.contains("OVERRIDING"), "{sql}");
    }
}
