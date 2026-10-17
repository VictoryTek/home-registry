//! Checks that run at startup, before database migrations.

use std::path::Path;

use deadpool_postgres::Pool;
use log::{error, warn};

use crate::api::backup::create_backup_file;
use crate::db::DatabaseService;

/// Tables removed by migration V026. They were never written by any release, so they should be
/// empty; this is checked anyway, because dropping them cannot be undone.
const V026_TABLES: [&str; 6] = [
    "categories",
    "tags",
    "item_tags",
    "custom_fields",
    "item_custom_values",
    "password_reset_tokens",
];

/// Prefix of the backup written before migration V026 (kept valid by
/// `api::backup::is_valid_backup_filename`).
const V026_BACKUP_PREFIX: &str = "home_registry_pre_migration_v026";

/// Data that migration V026 is about to delete.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LegacyData {
    /// Rows in each of the dropped tables that currently holds any
    pub tables: Vec<(String, i64)>,
    /// Items with a non-null `category_id` (the dropped column)
    pub items_with_category_id: i64,
}

impl LegacyData {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty() && self.items_with_category_id == 0
    }

    /// One line for the log: `tags=12, items.category_id=3`
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self
            .tables
            .iter()
            .map(|(t, n)| format!("{t}={n}"))
            .collect();
        if self.items_with_category_id > 0 {
            parts.push(format!("items.category_id={}", self.items_with_category_id));
        }
        parts.join(", ")
    }
}

/// Find data in what migration V026 will drop. Only looks at objects that still exist, so it is
/// safe on a fresh database, on one already migrated, and on one that never had these tables.
///
/// # Errors
/// Fails on database errors.
pub async fn find_legacy_data(pool: &Pool) -> Result<LegacyData, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let mut found = LegacyData::default();

    for table in V026_TABLES {
        let exists: bool = client
            .query_one(
                "SELECT to_regclass($1::text) IS NOT NULL",
                &[&format!("public.{table}")],
            )
            .await?
            .get(0);
        if !exists {
            continue;
        }
        let count: i64 = client
            .query_one(
                format!("SELECT COUNT(*) FROM public.\"{table}\"").as_str(),
                &[],
            )
            .await?
            .get(0);
        if count > 0 {
            found.tables.push((table.to_string(), count));
        }
    }

    let has_column: bool = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                            WHERE table_schema = 'public' AND table_name = 'items'
                              AND column_name = 'category_id')",
            &[],
        )
        .await?
        .get(0);
    if has_column {
        found.items_with_category_id = client
            .query_one(
                "SELECT COUNT(*) FROM public.items WHERE category_id IS NOT NULL",
                &[],
            )
            .await?
            .get(0);
    }
    Ok(found)
}

/// Whether migration 26 has not been applied yet (also true when there is no migration
/// bookkeeping table at all).
async fn v026_pending(pool: &Pool) -> Result<bool, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let has_history: bool = client
        .query_one(
            "SELECT to_regclass('public.refinery_schema_history') IS NOT NULL",
            &[],
        )
        .await?
        .get(0);
    if !has_history {
        return Ok(true);
    }
    let applied: bool = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM public.refinery_schema_history WHERE version = 26)",
            &[],
        )
        .await?
        .get(0);
    Ok(!applied)
}

/// Before migration V026 runs: if the data it will delete is not empty, write a backup first.
///
/// Returns the backup's file name when one was written, `None` when there was nothing to protect.
/// The counts and the file name are logged at WARN before the migration starts.
///
/// # Errors
/// Returns an error if the data check or the backup fails. The caller must then refuse to
/// start rather than migrate: the data would be gone with no copy.
pub async fn protect_legacy_data_before_migration(
    pool: &Pool,
    backups_dir: &Path,
) -> Result<Option<String>, String> {
    let pending = v026_pending(pool)
        .await
        .map_err(|e| format!("could not check the migration state: {e}"))?;
    if !pending {
        return Ok(None);
    }
    let found = find_legacy_data(pool)
        .await
        .map_err(|e| format!("could not check the tables that migration V026 removes: {e}"))?;
    if found.is_empty() {
        return Ok(None);
    }

    warn!(
        "Migration V026 will delete data from tables no release ever wrote to: {}",
        found.summary()
    );
    let db = DatabaseService::new(pool.clone());
    match create_backup_file(&db, V026_BACKUP_PREFIX, backups_dir).await {
        Ok(info) => {
            warn!(
                "Wrote a backup of the current data before migration V026: {}/{}",
                backups_dir.display(),
                info.name
            );
            Ok(Some(info.name))
        },
        Err(e) => {
            error!("Could not write the backup before migration V026: {e}");
            Err(format!(
                "migration V026 would delete data ({}) and the backup before it could not be \
                 written to {}: {e}",
                found.summary(),
                backups_dir.display()
            ))
        },
    }
}
