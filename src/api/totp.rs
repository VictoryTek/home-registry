//! TOTP authenticator API endpoints
//!
//! Provides endpoints for TOTP setup, verification, recovery, mode changes,
//! and status checking.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse, Responder, Result};
use deadpool_postgres::Pool;
use log::{error, info, warn};
use uuid::Uuid;

use crate::auth::throttle::{self, Subject};
use crate::auth::totp::{generate_totp_setup, verify_totp_code};
use crate::auth::{
    extract_token, generate_token, hash_password, validate_password, verify_password, verify_token,
    PasswordWorkError,
};
use crate::db::DatabaseService;
use crate::models::{
    ApiResponse, ErrorResponse, LoginResponse, TotpDisableRequest, TotpModeRequest,
    TotpRecoveryRequest, TotpSetupResponse, TotpStatusResponse, TotpVerifyRequest,
    TotpVerifySetupRequest, TotpVerifySetupResponse,
};
use crate::rate_limit::client_ip_key;

use super::auth::get_auth_context_from_request;

/// Maximum failed TOTP attempts before rate-limiting
const MAX_FAILED_ATTEMPTS: i32 = 5;

/// Rate limit window in minutes
const RATE_LIMIT_MINUTES: i64 = 15;

/// Check if TOTP is rate-limited based on failed attempts
fn is_totp_rate_limited(
    failed_attempts: i32,
    last_failed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    if failed_attempts < MAX_FAILED_ATTEMPTS {
        return false;
    }

    if let Some(last_failed) = last_failed_at {
        let window = chrono::Utc::now() - chrono::Duration::minutes(RATE_LIMIT_MINUTES);
        last_failed > window
    } else {
        false
    }
}

// ==================== TOTP Setup ====================

/// Generate TOTP secret and QR code for setup
#[post("/auth/totp/setup")]
pub async fn totp_setup(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Check if TOTP is already enabled
    if let Ok(Some(settings)) = db_service.get_totp_settings(auth.user_id).await {
        if settings.is_enabled && settings.is_verified {
            return Ok(HttpResponse::Conflict().json(ErrorResponse {
                success: false,
                error: "TOTP already enabled".to_string(),
                message: Some(
                    "Disable your current authenticator before setting up a new one".to_string(),
                ),
            }));
        }
    }

    // Generate new TOTP setup
    let setup_data = match generate_totp_setup(&auth.username) {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to generate TOTP setup: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to generate authenticator setup".to_string(),
                message: None,
            }));
        },
    };

    // Store encrypted secret (unverified, disabled)
    if let Err(e) = db_service
        .create_totp_settings(auth.user_id, &setup_data.encrypted_secret)
        .await
    {
        error!("Failed to store TOTP settings: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "Failed to save authenticator setup".to_string(),
            message: None,
        }));
    }

    info!("TOTP setup initiated for user {}", auth.username);

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(TotpSetupResponse {
            secret: setup_data.secret_base32,
            otpauth_uri: setup_data.otpauth_uri,
            qr_code_data_uri: setup_data.qr_code_data_uri,
            issuer: "HomeRegistry".to_string(),
            algorithm: "SHA1".to_string(),
            digits: 6,
            period: 30,
        }),
        message: Some(
            "Scan the QR code with your authenticator app, then verify with a code".to_string(),
        ),
        error: None,
    }))
}

// ==================== TOTP Verify Setup ====================

/// Verify first TOTP code to confirm setup and enable TOTP
#[post("/auth/totp/verify-setup")]
pub async fn totp_verify_setup(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<TotpVerifySetupRequest>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Get pending TOTP settings
    let settings = match db_service.get_totp_settings(auth.user_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "No pending TOTP setup found".to_string(),
                message: Some("Please initiate TOTP setup first".to_string()),
            }));
        },
        Err(e) => {
            error!("Error getting TOTP settings: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Database error".to_string(),
                message: None,
            }));
        },
    };

    // Check rate limiting
    if is_totp_rate_limited(settings.failed_attempts, settings.last_failed_at) {
        warn!(
            "TOTP setup verification rate limited for user {}",
            auth.username
        );
        return Ok(HttpResponse::TooManyRequests().json(ErrorResponse {
            success: false,
            error: "Too many failed attempts".to_string(),
            message: Some(format!(
                "Please wait {RATE_LIMIT_MINUTES} minutes before trying again"
            )),
        }));
    }

    // If already enabled, reject
    if settings.is_enabled && settings.is_verified {
        return Ok(HttpResponse::Conflict().json(ErrorResponse {
            success: false,
            error: "TOTP already enabled".to_string(),
            message: None,
        }));
    }

    // Verify the code
    let is_valid = match verify_totp_code(&settings.totp_secret_encrypted, &body.code) {
        Ok(valid) => valid,
        Err(e) => {
            error!("Error verifying TOTP code: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Verification error".to_string(),
                message: None,
            }));
        },
    };

    if !is_valid {
        let _ = db_service
            .increment_totp_failed_attempts(auth.user_id)
            .await;
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid verification code".to_string(),
            message: Some("The code you entered is incorrect. Please try again.".to_string()),
        }));
    }

    // Enable TOTP with the selected mode
    let mode_str = body.mode.as_str();
    if let Err(e) = db_service.enable_totp(auth.user_id, mode_str).await {
        error!("Error enabling TOTP: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "Failed to enable authenticator".to_string(),
            message: None,
        }));
    }

    info!(
        "TOTP enabled for user {} with mode {}",
        auth.username, mode_str
    );

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(TotpVerifySetupResponse {
            enabled: true,
            mode: body.mode,
        }),
        message: Some("Authenticator enabled successfully".to_string()),
        error: None,
    }))
}

// ==================== TOTP Verify (Login 2FA) ====================

/// Verify TOTP code during login (second factor)
/// Requires a `partial_token` (JWT with `totp_pending=true`)
#[post("/auth/totp/verify")]
pub async fn totp_verify(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<TotpVerifyRequest>,
) -> Result<impl Responder> {
    // Extract and verify partial token
    let Some(token) = extract_token(&req) else {
        return Ok(HttpResponse::Unauthorized().json(ErrorResponse {
            success: false,
            error: "No authentication token provided".to_string(),
            message: Some("Please log in first".to_string()),
        }));
    };

    let claims = match verify_token(&token) {
        Ok(c) => c,
        Err(e) => {
            return Ok(HttpResponse::Unauthorized().json(ErrorResponse {
                success: false,
                error: format!("Invalid or expired token: {e}"),
                message: Some("Please log in again".to_string()),
            }));
        },
    };

    // Must be a partial (TOTP pending) token
    if !claims.totp_pending {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid token type".to_string(),
            message: Some("This endpoint requires a TOTP verification token".to_string()),
        }));
    }

    let Ok(user_id) = Uuid::parse_str(&claims.sub) else {
        return Ok(HttpResponse::Unauthorized().json(ErrorResponse {
            success: false,
            error: "Invalid token".to_string(),
            message: Some("Please log in again".to_string()),
        }));
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Verify user still exists and is active
    let user = match db_service.get_user_by_id(user_id).await {
        Ok(Some(u)) if u.is_active => u,
        Ok(Some(_)) => {
            return Ok(HttpResponse::Forbidden().json(ErrorResponse {
                success: false,
                error: "Account deactivated".to_string(),
                message: None,
            }));
        },
        _ => {
            return Ok(HttpResponse::Unauthorized().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: Some("Please log in again".to_string()),
            }));
        },
    };

    // Get TOTP settings
    let settings = match db_service.get_totp_settings(user_id).await {
        Ok(Some(s)) if s.is_enabled => s,
        _ => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "TOTP not enabled".to_string(),
                message: None,
            }));
        },
    };

    // Check rate limiting
    if is_totp_rate_limited(settings.failed_attempts, settings.last_failed_at) {
        warn!(
            "TOTP login verification rate limited for user {}",
            user.username
        );
        return Ok(HttpResponse::TooManyRequests().json(ErrorResponse {
            success: false,
            error: "Too many failed attempts".to_string(),
            message: Some(format!(
                "Please wait {RATE_LIMIT_MINUTES} minutes before trying again"
            )),
        }));
    }

    // Verify the code
    let is_valid = match verify_totp_code(&settings.totp_secret_encrypted, &body.code) {
        Ok(valid) => valid,
        Err(e) => {
            error!("Error verifying TOTP code: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Verification error".to_string(),
                message: None,
            }));
        },
    };

    if !is_valid {
        let _ = db_service.increment_totp_failed_attempts(user_id).await;
        return Ok(HttpResponse::Unauthorized().json(ErrorResponse {
            success: false,
            error: "Invalid TOTP code".to_string(),
            message: Some("The code you entered is incorrect".to_string()),
        }));
    }

    // Reset failed attempts and update last used
    let _ = db_service.reset_totp_failed_attempts(user_id).await;
    let _ = db_service.update_totp_last_used(user_id).await;

    // Generate full JWT token
    let full_token = match generate_token(&user) {
        Ok(t) => t,
        Err(e) => {
            error!("Error generating token: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to generate token".to_string(),
                message: None,
            }));
        },
    };

    info!("User {} completed TOTP verification", user.username);

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(LoginResponse {
            token: full_token,
            user: user.into(),
        }),
        message: Some("Login successful".to_string()),
        error: None,
    }))
}

// ==================== TOTP Recovery ====================

/// Reset password using TOTP code (no auth required)
#[post("/auth/totp/recover")]
pub async fn totp_recover(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    body: web::Json<TotpRecoveryRequest>,
) -> Result<impl Responder> {
    let db_service = DatabaseService::new(pool.get_ref().clone());
    let client_ip = client_ip_key(&http_req);

    if let Err(wait) = throttle::global().check(Subject::Username(&body.username), &client_ip) {
        return Ok(crate::api::auth::throttled_response(wait));
    }

    // Validate new password first
    if let Err(msg) = validate_password(&body.new_password) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid password".to_string()),
        }));
    }

    // Count the attempt before any branch can return, so every generic failure (unknown or
    // inactive user, TOTP not enabled, wrong code) is throttled identically and the
    // lockout cannot be used to tell which usernames exist. Success clears it below.
    throttle::global().record_failure(Subject::Username(&body.username), &client_ip);

    // Look up user (don't reveal existence on failure)
    let Ok(Some(user)) = db_service.get_user_by_username(&body.username).await else {
        // Same error for user not found to prevent enumeration
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid credentials".to_string(),
            message: Some("Invalid username, code, or password".to_string()),
        }));
    };

    if !user.is_active {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid credentials".to_string(),
            message: Some("Invalid username, code, or password".to_string()),
        }));
    }

    // Check TOTP settings
    let settings = match db_service.get_totp_settings(user.id).await {
        Ok(Some(s)) if s.is_enabled && s.is_verified => s,
        _ => {
            // Same generic error
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Invalid credentials".to_string(),
                message: Some("Invalid username, code, or password".to_string()),
            }));
        },
    };

    // Check if mode allows recovery
    let mode: Result<crate::models::TotpMode, _> = settings.totp_mode.parse();
    match mode {
        Ok(m) if m.allows_recovery() => {},
        _ => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Invalid credentials".to_string(),
                message: Some("Invalid username, code, or password".to_string()),
            }));
        },
    }

    // Check rate limiting
    if is_totp_rate_limited(settings.failed_attempts, settings.last_failed_at) {
        warn!("TOTP recovery rate limited for user {}", user.username);
        return Ok(HttpResponse::TooManyRequests().json(ErrorResponse {
            success: false,
            error: "Too many failed attempts".to_string(),
            message: Some(format!(
                "Please wait {RATE_LIMIT_MINUTES} minutes before trying again"
            )),
        }));
    }

    // Verify TOTP code
    let is_valid = match verify_totp_code(&settings.totp_secret_encrypted, &body.totp_code) {
        Ok(valid) => valid,
        Err(e) => {
            error!("Error verifying TOTP code for recovery: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Verification error".to_string(),
                message: None,
            }));
        },
    };

    if !is_valid {
        let _ = db_service.increment_totp_failed_attempts(user.id).await;
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid credentials".to_string(),
            message: Some("Invalid username, code, or password".to_string()),
        }));
    }
    throttle::global().record_success(Subject::Username(&body.username), &client_ip);

    // Hash new password
    let password_hash = match hash_password(body.new_password.clone()).await {
        Ok(h) => h,
        Err(e) => {
            if matches!(e, PasswordWorkError::Busy) {
                return Ok(crate::api::auth::busy_response());
            }
            error!("Error hashing password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to reset password".to_string(),
                message: None,
            }));
        },
    };

    // Update password
    if let Err(e) = db_service
        .update_user_password(user.id, &password_hash)
        .await
    {
        error!("Error updating password: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "Failed to reset password".to_string(),
            message: None,
        }));
    }

    // Reset failed attempts and update last used
    let _ = db_service.reset_totp_failed_attempts(user.id).await;
    let _ = db_service.update_totp_last_used(user.id).await;

    info!(
        "User {} reset password via TOTP authenticator",
        user.username
    );

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(serde_json::json!({
            "message": "Password reset successfully via authenticator"
        })),
        message: Some(
            "Password reset successfully. You can now log in with your new password.".to_string(),
        ),
        error: None,
    }))
}

// ==================== TOTP Mode ====================

/// Change TOTP mode
#[put("/auth/totp/mode")]
pub async fn totp_update_mode(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<TotpModeRequest>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Check TOTP is enabled
    match db_service.get_totp_settings(auth.user_id).await {
        Ok(Some(s)) if s.is_enabled => {},
        Ok(_) => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "TOTP not enabled".to_string(),
                message: Some("Enable TOTP before changing the mode".to_string()),
            }));
        },
        Err(e) => {
            error!("Error getting TOTP settings: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Database error".to_string(),
                message: None,
            }));
        },
    }

    let mode_str = body.mode.as_str();
    if let Err(e) = db_service.update_totp_mode(auth.user_id, mode_str).await {
        error!("Error updating TOTP mode: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "Failed to update mode".to_string(),
            message: None,
        }));
    }

    info!(
        "TOTP mode updated to {} for user {}",
        mode_str, auth.username
    );

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(serde_json::json!({ "mode": mode_str })),
        message: Some(format!("TOTP mode updated to {mode_str}")),
        error: None,
    }))
}

// ==================== TOTP Disable ====================

/// Disable TOTP (requires password confirmation)
#[delete("/auth/totp")]
pub async fn totp_disable(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<TotpDisableRequest>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Get user to verify password
    let Ok(Some(user)) = db_service.get_user_by_id(auth.user_id).await else {
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "User not found".to_string(),
            message: None,
        }));
    };

    // Verify password
    let password_valid =
        match verify_password(body.password.clone(), user.password_hash.clone()).await {
            Ok(valid) => valid,
            Err(e) => {
                if matches!(e, PasswordWorkError::Busy) {
                    return Ok(crate::api::auth::busy_response());
                }
                error!("Error verifying password: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "Password verification failed".to_string(),
                    message: None,
                }));
            },
        };

    if !password_valid {
        return Ok(HttpResponse::Unauthorized().json(ErrorResponse {
            success: false,
            error: "Invalid password".to_string(),
            message: Some("The password you entered is incorrect".to_string()),
        }));
    }

    // Delete TOTP settings
    match db_service.delete_totp_settings(auth.user_id).await {
        Ok(true) => {
            info!("TOTP disabled for user {}", auth.username);
            Ok(HttpResponse::Ok().json(ApiResponse::<()> {
                success: true,
                data: None,
                message: Some("Authenticator disabled successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "TOTP not enabled".to_string(),
            message: Some("No authenticator to disable".to_string()),
        })),
        Err(e) => {
            error!("Error disabling TOTP: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to disable authenticator".to_string(),
                message: None,
            }))
        },
    }
}

// ==================== TOTP Status ====================

/// Get TOTP status for current user
#[get("/auth/totp/status")]
pub async fn totp_status(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    let response = match db_service.get_totp_settings(auth.user_id).await {
        Ok(Some(settings)) if settings.is_enabled => {
            let mode = settings.totp_mode.parse().ok();
            TotpStatusResponse {
                is_enabled: true,
                mode,
                last_used_at: settings.last_used_at,
                created_at: Some(settings.created_at),
            }
        },
        Ok(_) => TotpStatusResponse {
            is_enabled: false,
            mode: None,
            last_used_at: None,
            created_at: None,
        },
        Err(e) => {
            error!("Error getting TOTP status: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Database error".to_string(),
                message: None,
            }));
        },
    };

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(response),
        message: None,
        error: None,
    }))
}
