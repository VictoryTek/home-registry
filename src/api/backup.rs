//! Backup & Restore API endpoints
//!
//! Provides endpoints for creating, listing, downloading, uploading,
//! restoring, and deleting database backups. All endpoints require
//! admin authentication.
//!
//! The work itself lives in functions that take the backups directory as a parameter
//! ([`create_backup_file`], [`restore_backup_file`], [`store_uploaded_backup`]) so it can be
//! tested without touching the real `backups/` folder.

use actix_multipart::form::{tempfile::TempFile, MultipartForm};
use actix_web::{delete, get, post, web, HttpRequest, HttpResponse, Responder, Result};
use deadpool_postgres::Pool;
use log::{error, info, warn};
use std::path::Path;
use tokio::io::AsyncWriteExt;

use crate::api::auth;
use crate::auth::totp::totp_key_fingerprint;
use crate::db::DatabaseService;
use crate::models::{
    ApiResponse, BackupData, BackupInfo, BackupMetadata, ErrorResponse, RestoreReport,
    RestoreRequest,
};

/// Directory where backup files are stored
pub const BACKUPS_DIR: &str = "backups";

/// Backup file format written by this version
const FORMAT_VERSION: &str = "2.0";
/// Backup file formats this version can restore ("1.0" had a fixed table list)
const SUPPORTED_FORMATS: [&str; 2] = ["1.0", "2.0"];
/// Table holding 2FA enrolment (secrets encrypted with the TOTP key)
const TOTP_TABLE: &str = "user_totp_settings";

/// Multipart form for backup file upload
#[derive(MultipartForm)]
struct BackupUploadForm {
    #[multipart(limit = "100MB")]
    file: TempFile,
}

// ==================== Helper Functions ====================

fn all_digits(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `name` has the shape of a backup file name this application generates:
/// `home_registry[_auto_pre_restore|_pre_migration_vNNN]_YYYY.MM.DD.HH.MM.SS[.mmm_xxxxxxxx][_N].json`.
///
/// Nothing else is accepted for download, restore or delete, which rules out path traversal
/// (no separators, no `..`, only digits, hex and fixed words).
#[must_use]
pub fn is_valid_backup_filename(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    let rest = stem
        .strip_prefix("home_registry_auto_pre_restore_")
        .or_else(|| strip_pre_migration_prefix(stem))
        .or_else(|| stem.strip_prefix("home_registry_"));
    let Some(rest) = rest else {
        return false;
    };

    // YYYY.MM.DD.HH.MM.SS
    let Some((timestamp, mut tail)) = rest.split_at_checked(19) else {
        return false;
    };
    let parts: Vec<&str> = timestamp.split('.').collect();
    let widths = [4, 2, 2, 2, 2, 2];
    if parts.len() != 6 || !parts.iter().zip(widths).all(|(p, w)| all_digits(p, w)) {
        return false;
    }

    // optional .mmm_xxxxxxxx (milliseconds and a random suffix)
    if let Some(after_dot) = tail.strip_prefix('.') {
        let Some((ms, after_ms)) = after_dot.split_at_checked(3) else {
            return false;
        };
        let Some(after_underscore) = after_ms.strip_prefix('_') else {
            return false;
        };
        let Some((hex, after_hex)) = after_underscore.split_at_checked(8) else {
            return false;
        };
        if !all_digits(ms, 3)
            || !hex
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return false;
        }
        tail = after_hex;
    }

    // optional _N (older uploads that collided were numbered)
    if let Some(n) = tail.strip_prefix('_') {
        return !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit());
    }
    tail.is_empty()
}

/// `home_registry_pre_migration_v<N>_`: backups written automatically before a migration that
/// removes data (see `startup::protect_legacy_data_before_migration`).
fn strip_pre_migration_prefix(stem: &str) -> Option<&str> {
    let rest = stem.strip_prefix("home_registry_pre_migration_v")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=4).contains(&digits) {
        rest[digits..].strip_prefix('_')
    } else {
        None
    }
}

/// Validate a backup filename taken from a request
fn validate_backup_filename(filename: &str) -> std::result::Result<(), String> {
    if is_valid_backup_filename(filename) {
        Ok(())
    } else {
        Err("Invalid backup filename".to_string())
    }
}

/// Format file size in human-readable format
#[allow(clippy::cast_precision_loss)]
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
        format!("{bytes} B")
    }
}

/// Generate a unique backup filename: millisecond timestamp plus a random suffix, so two
/// backups in the same second can never collide.
#[must_use]
pub fn generate_backup_filename(prefix: &str) -> String {
    let timestamp = chrono::Utc::now()
        .format("%Y.%m.%d.%H.%M.%S.%3f")
        .to_string();
    let random = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}_{timestamp}_{}.json", &random[..8])
}

/// Ensure the backups directory exists
async fn ensure_dir(dir: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dir).await
}

/// Write `bytes` to a new file, failing with `AlreadyExists` instead of overwriting, and make
/// sure the data reached the disk before returning.
async fn write_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    let written = async {
        file.write_all(bytes).await?;
        file.sync_all().await
    }
    .await;
    if written.is_err() {
        // Do not leave a truncated file behind that would be listed as a backup
        let _ = tokio::fs::remove_file(path).await;
    }
    written
}

/// Store `bytes` under a freshly generated name that does not exist yet.
async fn store_with_generated_name(
    dir: &Path,
    prefix: &str,
    bytes: &[u8],
) -> std::io::Result<String> {
    // A collision needs the same millisecond and the same 32-bit random suffix; retrying a few
    // times makes running out of attempts a sign of something else being wrong.
    for _ in 0..5 {
        let filename = generate_backup_filename(prefix);
        match write_new_file(&dir.join(&filename), bytes).await {
            Ok(()) => return Ok(filename),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {},
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not find an unused backup filename",
    ))
}

/// Create a backup file in `dir` and return its info.
///
/// # Errors
/// Fails if the export or writing the file fails; nothing partial is left behind.
pub async fn create_backup_file(
    db_service: &DatabaseService,
    filename_prefix: &str,
    dir: &Path,
) -> std::result::Result<BackupInfo, Box<dyn std::error::Error>> {
    ensure_dir(dir).await?;

    // Export all database data from one consistent snapshot
    let export = db_service.export_all_data().await?;

    // Record the TOTP key fingerprint only when the backup holds 2FA enrolment
    let totp_key_fingerprint = (export.tables.row_count(TOTP_TABLE) > 0).then(totp_key_fingerprint);

    let backup_data = BackupData {
        metadata: BackupMetadata {
            version: FORMAT_VERSION.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            database_type: "postgresql".to_string(),
            description: None,
            schema_version: export.schema_version,
            totp_key_fingerprint,
        },
        data: export.tables,
    };

    let json_content = serde_json::to_string_pretty(&backup_data)?;
    let file_size = json_content.len() as u64;

    let filename = store_with_generated_name(dir, filename_prefix, json_content.as_bytes()).await?;

    info!(
        "Backup created: {} ({})",
        filename,
        format_file_size(file_size)
    );

    Ok(BackupInfo {
        name: filename,
        date: backup_data.metadata.created_at,
        size: format_file_size(file_size),
    })
}

/// Why an uploaded file could not be stored as a backup
#[derive(Debug)]
pub enum UploadError {
    /// The file is not a Home Registry backup
    NotABackup,
    /// A backup, but in a format this version cannot restore
    UnsupportedFormat(String),
    /// Writing it failed
    Io(std::io::Error),
}

/// Validate an uploaded file and store it in `dir`.
///
/// The content must parse as a backup before anything is written. The original name is kept
/// when it has the generated shape and is free; otherwise a unique name is generated.
///
/// # Errors
/// See [`UploadError`].
pub async fn store_uploaded_backup(
    dir: &Path,
    original_filename: &str,
    content: &[u8],
) -> std::result::Result<(BackupInfo, BackupData), UploadError> {
    let backup: BackupData = serde_json::from_slice(content).map_err(|e| {
        error!(
            "Invalid backup file format: {}",
            crate::api::extractors::loggable_json_error(&e)
        );
        UploadError::NotABackup
    })?;
    if !SUPPORTED_FORMATS.contains(&backup.metadata.version.as_str()) {
        return Err(UploadError::UnsupportedFormat(backup.metadata.version));
    }

    ensure_dir(dir).await.map_err(UploadError::Io)?;

    let stored_name = if is_valid_backup_filename(original_filename) {
        match write_new_file(&dir.join(original_filename), content).await {
            Ok(()) => original_filename.to_string(),
            // Never overwrite an existing backup: fall back to a generated name
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                store_with_generated_name(dir, "home_registry", content)
                    .await
                    .map_err(UploadError::Io)?
            },
            Err(e) => return Err(UploadError::Io(e)),
        }
    } else {
        store_with_generated_name(dir, "home_registry", content)
            .await
            .map_err(UploadError::Io)?
    };

    let info = BackupInfo {
        name: stored_name,
        date: backup.metadata.created_at.clone(),
        size: format_file_size(content.len() as u64),
    };
    Ok((info, backup))
}

/// Why a restore was refused or failed. Every variant except `Database` is reported before
/// anything in the database was changed.
#[derive(Debug)]
pub enum RestoreError {
    InvalidFilename,
    NotFound,
    /// The file could not be read or is not a valid backup
    InvalidFile(String),
    UnsupportedFormat(String),
    /// The backup was made by a newer version of the application
    NewerThanDatabase {
        backup: i64,
        database: i64,
    },
    /// The backup holds 2FA enrolment encrypted with a different key
    TotpKeyMismatch {
        users: usize,
    },
    /// The automatic pre-restore backup could not be written, so the restore was not started
    SafetyBackupFailed(String),
    /// The restore transaction failed and was rolled back
    Database(String),
}

/// Restore the database from `filename` in `dir`.
///
/// Order matters: every check that can refuse the restore (format, schema version, 2FA key)
/// runs first, then the automatic pre-restore backup is written and synced, and only then does
/// the restore transaction start. If the safety backup fails, nothing is restored.
///
/// # Errors
/// See [`RestoreError`].
pub async fn restore_backup_file(
    db_service: &DatabaseService,
    dir: &Path,
    filename: &str,
    options: &RestoreRequest,
) -> std::result::Result<RestoreReport, RestoreError> {
    if validate_backup_filename(filename).is_err() {
        return Err(RestoreError::InvalidFilename);
    }
    let filepath = dir.join(filename);
    if !tokio::fs::try_exists(&filepath).await.unwrap_or(false) {
        return Err(RestoreError::NotFound);
    }

    let content = tokio::fs::read(&filepath)
        .await
        .map_err(|e| RestoreError::InvalidFile(format!("could not read the backup file: {e}")))?;
    let backup: BackupData = serde_json::from_slice(&content)
        .map_err(|e| RestoreError::InvalidFile(format!("not a valid backup: {e}")))?;
    if !SUPPORTED_FORMATS.contains(&backup.metadata.version.as_str()) {
        return Err(RestoreError::UnsupportedFormat(backup.metadata.version));
    }

    // A backup from a newer schema may hold data this version cannot represent
    let database_version = db_service
        .current_schema_version()
        .await
        .map_err(|e| RestoreError::Database(e.to_string()))?;
    if let (Some(backup_version), Some(database_version)) =
        (backup.metadata.schema_version, database_version)
    {
        if backup_version > database_version {
            return Err(RestoreError::NewerThanDatabase {
                backup: backup_version,
                database: database_version,
            });
        }
    }

    // 2FA secrets are encrypted with the TOTP key. Only check when the backup holds any.
    let mut skip_tables: Vec<String> = Vec::new();
    let mut two_factor_cleared = None;
    let totp_rows = backup.data.row_count(TOTP_TABLE);
    if totp_rows > 0 {
        let key_matches = backup.metadata.totp_key_fingerprint.as_deref()
            == Some(totp_key_fingerprint().as_str());
        if !key_matches {
            if !options.restore_without_2fa {
                return Err(RestoreError::TotpKeyMismatch { users: totp_rows });
            }
            warn!(
                "Restoring '{filename}' without two-factor enrolment: its TOTP key differs; 2FA is cleared for {totp_rows} users"
            );
            skip_tables.push(TOTP_TABLE.to_string());
            two_factor_cleared = Some(totp_rows as u64);
        }
    }

    // Safety net: the pre-restore backup must exist on disk before anything is touched
    let safety = create_backup_file(db_service, "home_registry_auto_pre_restore", dir)
        .await
        .map_err(|e| {
            error!("Failed to create auto-backup before restore: {e}");
            RestoreError::SafetyBackupFailed(e.to_string())
        })?;
    info!("Auto-backup created before restore: {}", safety.name);

    let mut report = db_service
        .import_all_data(&backup.data, &skip_tables)
        .await
        .map_err(|e| RestoreError::Database(e.to_string()))?;
    report.safety_backup = safety.name;
    report.two_factor_cleared_users = two_factor_cleared;
    Ok(report)
}

// ==================== API Handlers ====================

/// Create a new backup of all database data
///
/// POST /api/backup/create
/// Requires: Admin authentication
#[post("/backup/create")]
pub async fn create_backup(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth = match auth::require_admin(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match create_backup_file(&db_service, "home_registry", Path::new(BACKUPS_DIR)).await {
        Ok(backup_info) => {
            info!(
                "Backup created by admin user {}: {}",
                auth.username, backup_info.name
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(backup_info),
                message: Some("Backup created successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Failed to create backup: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to create backup".to_string()),
            }))
        },
    }
}

/// List all available backup files
///
/// GET /api/backup/list
/// Requires: Admin authentication
#[get("/backup/list")]
pub async fn list_backups(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth = match auth::require_admin(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    // Ensure backups directory exists
    if let Err(e) = ensure_dir(Path::new(BACKUPS_DIR)).await {
        error!("Failed to create backups directory: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "An internal error occurred".to_string(),
            message: Some("Failed to access backups directory".to_string()),
        }));
    }

    // Read directory and collect backup file info
    let mut backups: Vec<BackupInfo> = Vec::new();
    let mut entries = match tokio::fs::read_dir(BACKUPS_DIR).await {
        Ok(entries) => entries,
        Err(e) => {
            error!("Failed to read backups directory: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to read backups directory".to_string()),
            }));
        },
    };

    let mut ignored = 0usize;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext == "json" {
                if let Ok(metadata) = entry.metadata().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    // Only names this application generates can be downloaded, restored or
                    // deleted, so only those are listed
                    if !is_valid_backup_filename(&name) {
                        ignored += 1;
                        continue;
                    }
                    let date = metadata
                        .modified()
                        .map(|t| {
                            let datetime: chrono::DateTime<chrono::Utc> = t.into();
                            datetime.to_rfc3339()
                        })
                        .unwrap_or_default();
                    let size = format_file_size(metadata.len());

                    backups.push(BackupInfo { name, date, size });
                }
            }
        }
    }

    if ignored > 0 {
        warn!("Ignoring {ignored} .json file(s) in {BACKUPS_DIR}/ whose names do not match the backup naming pattern");
    }

    // Sort by date, newest first
    backups.sort_by(|a, b| b.date.cmp(&a.date));

    let count = backups.len();
    info!("Listed {} backups for admin user {}", count, auth.username);

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(backups),
        message: Some(format!("Retrieved {count} backups")),
        error: None,
    }))
}

/// Download a backup file
///
/// GET /api/backup/download/{filename}
/// Requires: Admin authentication
#[get("/backup/download/{filename}")]
pub async fn download_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let auth = match auth::require_admin(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let filename = path.into_inner();

    // Validate filename to prevent path traversal
    if let Err(e) = validate_backup_filename(&filename) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: e,
            message: Some("Invalid backup filename".to_string()),
        }));
    }

    let filepath = format!("{BACKUPS_DIR}/{filename}");

    // Verify file exists
    if !tokio::fs::try_exists(&filepath).await.unwrap_or(false) {
        return Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "Backup file not found".to_string(),
            message: Some(format!("Backup '{filename}' does not exist")),
        }));
    }

    // Read file content
    match tokio::fs::read(&filepath).await {
        Ok(content) => {
            info!(
                "Backup '{}' downloaded by admin user {}",
                filename, auth.username
            );
            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .insert_header((
                    "Content-Disposition",
                    format!("attachment; filename=\"{filename}\""),
                ))
                .body(content))
        },
        Err(e) => {
            error!("Failed to read backup file '{}': {}", filename, e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to read backup file".to_string()),
            }))
        },
    }
}

/// Upload a backup file
///
/// POST /api/backup/upload
/// Requires: Admin authentication
/// Accepts: multipart/form-data with a 'file' field containing a .json backup
#[post("/backup/upload")]
pub async fn upload_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    MultipartForm(form): MultipartForm<BackupUploadForm>,
) -> Result<impl Responder> {
    let auth = match auth::require_admin(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    // Get the original filename
    let original_filename = form
        .file
        .file_name
        .as_deref()
        .unwrap_or("unknown.json")
        .to_string();

    // Validate file extension
    if !Path::new(&original_filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Only .json backup files are allowed".to_string(),
            message: Some("Please upload a valid JSON backup file".to_string()),
        }));
    }

    // Read the temp file content
    let content = match tokio::fs::read(form.file.file.path()).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to read uploaded file: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to process uploaded file".to_string()),
            }));
        },
    };

    // Parsed and checked before anything is written to the backups directory
    match store_uploaded_backup(Path::new(BACKUPS_DIR), &original_filename, &content).await {
        Ok((info, _backup)) => {
            info!(
                "Backup '{}' uploaded by admin user {} ({})",
                info.name, auth.username, info.size
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(info),
                message: Some("Backup uploaded successfully".to_string()),
                error: None,
            }))
        },
        Err(UploadError::NotABackup) => Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid backup file format".to_string(),
            message: Some("The uploaded file is not a valid Home Registry backup".to_string()),
        })),
        Err(UploadError::UnsupportedFormat(version)) => {
            Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: format!("Unsupported backup version: {version}"),
                message: Some("This backup version is not supported".to_string()),
            }))
        },
        Err(UploadError::Io(e)) => {
            error!("Failed to save uploaded backup: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to save backup file".to_string()),
            }))
        },
    }
}

/// Restore database from a backup file
///
/// POST /api/backup/restore/{filename}
/// Requires: Admin authentication
/// Optional JSON body: `{"restore_without_2fa": true}` to restore everything except two-factor
/// enrolment when the backup's TOTP key does not match this server's.
/// Creates an automatic backup before restoring (safety net) and ends every session.
#[post("/backup/restore/{filename}")]
pub async fn restore_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<String>,
    body: Option<web::Json<RestoreRequest>>,
) -> Result<impl Responder> {
    let auth = match auth::require_admin(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let filename = path.into_inner();
    let options = body.map(web::Json::into_inner).unwrap_or_default();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match restore_backup_file(&db_service, Path::new(BACKUPS_DIR), &filename, &options).await {
        Ok(report) => {
            info!(
                "Backup '{}' restored by admin user {} ({} tables, {} rows, 2FA cleared for {:?} users); all sessions ended",
                filename,
                auth.username,
                report.tables_restored,
                report.rows_restored,
                report.two_factor_cleared_users
            );
            let message = format!(
                "Backup restored successfully. A pre-restore backup was created: {}. \
                 All sessions ended; everyone must sign in again.",
                report.safety_backup
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(report),
                message: Some(message),
                error: None,
            }))
        },
        Err(e) => Ok(restore_error_response(&filename, e)),
    }
}

/// Turn a restore failure into an HTTP response. Refusals use stable `error` codes the UI keys
/// off (`totp_key_mismatch`, `backup_newer_than_database`).
fn restore_error_response(filename: &str, err: RestoreError) -> HttpResponse {
    let bad_request = |error: &str, message: String| {
        HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: error.to_string(),
            message: Some(message),
        })
    };
    let conflict = |error: &str, message: String| {
        HttpResponse::Conflict().json(ErrorResponse {
            success: false,
            error: error.to_string(),
            message: Some(message),
        })
    };
    match err {
        RestoreError::InvalidFilename => bad_request(
            "Invalid backup filename",
            "Invalid backup filename".to_string(),
        ),
        RestoreError::NotFound => HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "Backup file not found".to_string(),
            message: Some(format!("Backup '{filename}' does not exist")),
        }),
        RestoreError::InvalidFile(detail) => {
            error!("Invalid backup file '{filename}': {detail}");
            bad_request(
                "Invalid backup file format",
                "The backup file could not be parsed".to_string(),
            )
        },
        RestoreError::UnsupportedFormat(version) => bad_request(
            &format!("Unsupported backup version: {version}"),
            "This backup version is not supported".to_string(),
        ),
        RestoreError::NewerThanDatabase { backup, database } => conflict(
            "backup_newer_than_database",
            format!(
                "This backup was made by a newer version of Home Registry (database schema \
                 {backup}, this server is at {database}). Update the application first, then \
                 restore it. Nothing was changed."
            ),
        ),
        RestoreError::TotpKeyMismatch { users } => conflict(
            "totp_key_mismatch",
            format!(
                "The backup contains two-factor authentication for {users} user(s), encrypted \
                 with a key that this server does not have. Start the server with the original \
                 TOTP_ENCRYPTION_KEY (or JWT_SECRET if none was set) and restore again. To \
                 restore everything except two-factor authentication instead, repeat the \
                 request with \"restore_without_2fa\": true; those users will then have to set \
                 up 2FA again. Nothing was changed."
            ),
        ),
        RestoreError::SafetyBackupFailed(detail) => {
            error!("Safety backup before restoring '{filename}' failed: {detail}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some(
                    "Failed to create safety backup before restore. Restore aborted; nothing was changed."
                        .to_string(),
                ),
            })
        },
        RestoreError::Database(detail) => {
            error!("Failed to restore backup '{filename}': {detail}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some(
                    "Failed to restore backup; the database was left unchanged. A pre-restore backup was written beforehand."
                        .to_string(),
                ),
            })
        },
    }
}

/// Delete a backup file
///
/// DELETE /api/backup/{filename}
/// Requires: Admin authentication
#[delete("/backup/{filename}")]
pub async fn delete_backup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let auth = match auth::require_admin(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let filename = path.into_inner();

    // Validate filename to prevent path traversal
    if let Err(e) = validate_backup_filename(&filename) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: e,
            message: Some("Invalid backup filename".to_string()),
        }));
    }

    let filepath = format!("{BACKUPS_DIR}/{filename}");

    // Verify file exists
    if !tokio::fs::try_exists(&filepath).await.unwrap_or(false) {
        return Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "Backup file not found".to_string(),
            message: Some(format!("Backup '{filename}' does not exist")),
        }));
    }

    // Delete the file
    match tokio::fs::remove_file(&filepath).await {
        Ok(()) => {
            info!(
                "Backup '{}' deleted by admin user {}",
                filename, auth.username
            );
            Ok(HttpResponse::Ok().json(ApiResponse::<()> {
                success: true,
                data: None,
                message: Some(format!("Backup {filename} deleted successfully")),
                error: None,
            }))
        },
        Err(e) => {
            error!("Failed to delete backup '{}': {}", filename, e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to delete backup file".to_string()),
            }))
        },
    }
}
