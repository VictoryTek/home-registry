//! User authentication and management API endpoints
//!
//! Provides endpoints for login, registration, profile management,
//! admin user management, and initial setup.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse, Responder, Result};
use deadpool_postgres::Pool;
use log::{error, info, warn};
use rand::distributions::Alphanumeric;
use rand::Rng;
use uuid::Uuid;

use crate::auth::throttle::{self, Subject};
use crate::auth::tokens;
use crate::auth::{
    extract_token, generate_partial_token, generate_token, hash_password, issue_token,
    validate_password, validate_username, verify_against_dummy, verify_password, verify_token,
    AuthContext, PasswordWorkError, ARGON2_RETRY_AFTER_SECS,
};
use crate::db::{DatabaseService, InviteRegistration, PasswordReset};
use crate::models::{
    AdminCreateUserRequest, AdminUpdateUserRequest, ApiResponse, ChangePasswordRequest,
    ConfirmRecoveryCodesRequest, CreateInventoryShareRequest, CreateUserAccessGrantRequest,
    ErrorResponse, GenerateRecoveryCodesRequest, InitialSetupRequest, InviteCheckResponse,
    IssueTokenRequest, IssuedTokenResponse, LoginRequest, LoginResponse, LoginTotpRequiredResponse,
    RecoveryCodeUsedResponse, RecoveryCodesResponse, RecoveryCodesStatus, RegistrationMode,
    RegistrationSettingsResponse, ResetPasswordRequest, SessionTokenResponse, SetupStatusResponse,
    TokenRequest, TransferOwnershipRequest, TransferOwnershipResponse, UpdateInventoryShareRequest,
    UpdateProfileRequest, UpdateRegistrationSettingsRequest, UpdateUserSettingsRequest,
    UseRecoveryCodeRequest, UserResponse,
};
use crate::rate_limit::client_ip_key;

/// Recovery codes issued per user (and verifications done per recovery attempt).
const RECOVERY_CODE_COUNT: usize = 10;

// ==================== Helper Functions ====================

/// 429 for a locked-out username/IP, with `Retry-After` rounded up to whole seconds.
pub(crate) fn throttled_response(wait: std::time::Duration) -> HttpResponse {
    let secs = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
    HttpResponse::TooManyRequests()
        .insert_header(("Retry-After", secs.max(1).to_string()))
        .json(ErrorResponse {
            success: false,
            // The login page shows `error`, so the wait goes there
            error: format!("Too many attempts. Try again in {} seconds.", secs.max(1)),
            message: None,
        })
}

/// 503 when the Argon2 worker pool stayed saturated for the whole wait.
pub(crate) fn busy_response() -> HttpResponse {
    HttpResponse::ServiceUnavailable()
        .insert_header(("Retry-After", ARGON2_RETRY_AFTER_SECS.to_string()))
        .json(ErrorResponse {
            success: false,
            error: "Server busy. Please try again in a moment.".to_string(),
            message: None,
        })
}

/// 401 for a token whose session generation no longer matches the account (password changed,
/// role changed, signed out of other devices, ...). The login page keys its notice off the
/// `session_revoked` error code.
pub(crate) fn session_revoked_response() -> HttpResponse {
    HttpResponse::Unauthorized().json(ErrorResponse {
        success: false,
        error: "session_revoked".to_string(),
        message: Some(
            "Your session ended because your account was changed or you signed out of all devices"
                .to_string(),
        ),
    })
}

/// Audit trail for session revocations. Records who and why, never token material.
pub(crate) fn audit_sessions_revoked(subject: Uuid, actor: &str, reason: &str) {
    info!(target: "audit", "sessions_revoked subject={subject} actor={actor} reason={reason}");
}

/// Whether `target` is the only active admin (removing, demoting or deactivating them would
/// leave nobody able to administer the system).
async fn is_last_active_admin(db: &DatabaseService, target: &crate::models::User) -> bool {
    target.is_admin && target.is_active && db.count_admin_users().await.unwrap_or(0) <= 1
}

/// Extract and verify auth context from request
pub async fn get_auth_context_from_request(
    req: &HttpRequest,
    pool: &Pool,
) -> Result<AuthContext, HttpResponse> {
    let Some(token) = extract_token(req) else {
        return Err(HttpResponse::Unauthorized().json(ErrorResponse {
            success: false,
            error: "No authentication token provided".to_string(),
            message: Some("Please log in to access this resource".to_string()),
        }));
    };

    let claims = match verify_token(&token) {
        Ok(c) => c,
        Err(e) => {
            return Err(HttpResponse::Unauthorized().json(ErrorResponse {
                success: false,
                error: format!("Invalid token: {e}"),
                message: Some("Please log in again".to_string()),
            }));
        },
    };

    // Reject partial TOTP tokens — they can only be used for TOTP verification
    if claims.totp_pending {
        return Err(HttpResponse::Unauthorized().json(ErrorResponse {
            success: false,
            error: "TOTP verification required".to_string(),
            message: Some("Please complete two-factor authentication".to_string()),
        }));
    }

    let Ok(user_id) = Uuid::parse_str(&claims.sub) else {
        return Err(HttpResponse::Unauthorized().json(ErrorResponse {
            success: false,
            error: "Invalid user ID in token".to_string(),
            message: Some("Please log in again".to_string()),
        }));
    };

    // One read of the user row gives existence, active flag, role, name and session generation
    let db_service = DatabaseService::new(pool.clone());
    let auth_ctx = match db_service.get_user_by_id(user_id).await {
        Ok(Some(user)) => {
            if !user.is_active {
                return Err(HttpResponse::Forbidden().json(ErrorResponse {
                    success: false,
                    error: "Account is deactivated".to_string(),
                    message: Some(
                        "Your account has been deactivated. Contact an administrator.".to_string(),
                    ),
                }));
            }
            if claims.tv != user.token_version {
                return Err(session_revoked_response());
            }
            AuthContext::from_user(&user)
        },
        Ok(None) => {
            return Err(HttpResponse::Unauthorized().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: Some("Please log in again".to_string()),
            }));
        },
        Err(e) => {
            error!("Database error verifying user: {}", e);
            return Err(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Database error".to_string(),
                message: Some("Unable to verify user".to_string()),
            }));
        },
    };

    Ok(auth_ctx)
}

/// Require admin privileges
pub async fn require_admin(req: &HttpRequest, pool: &Pool) -> Result<AuthContext, HttpResponse> {
    let auth_ctx = get_auth_context_from_request(req, pool).await?;

    if !auth_ctx.is_admin {
        return Err(HttpResponse::Forbidden().json(ErrorResponse {
            success: false,
            error: "Admin privileges required".to_string(),
            message: Some("You don't have permission to access this resource".to_string()),
        }));
    }

    Ok(auth_ctx)
}

// ==================== Public Endpoints ====================

/// Check if initial setup is needed (no users exist)
#[get("/auth/setup/status")]
pub async fn get_setup_status(pool: web::Data<Pool>) -> Result<impl Responder> {
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_user_count().await {
        Ok(count) => {
            // Only used to decide what the sign-in page offers; if it cannot be read, say invite
            let registration_mode = db_service
                .get_registration_mode()
                .await
                .unwrap_or(RegistrationMode::Invite);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(SetupStatusResponse {
                    needs_setup: count == 0,
                    user_count: count,
                    registration_mode,
                }),
                message: None,
                error: None,
            }))
        },
        Err(e) => {
            error!("Error checking setup status: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to check setup status".to_string()),
            }))
        },
    }
}

/// Initial setup - create first admin user (only works when no users exist)
#[post("/auth/setup")]
pub async fn initial_setup(
    pool: web::Data<Pool>,
    req: web::Json<InitialSetupRequest>,
) -> Result<impl Responder> {
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Verify no users exist
    match db_service.get_user_count().await {
        Ok(count) if count > 0 => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Setup already completed".to_string(),
                message: Some("An admin user already exists".to_string()),
            }));
        },
        Err(e) => {
            error!("Error checking user count: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: Some("Failed to check setup status".to_string()),
            }));
        },
        _ => {},
    }

    // Validate input
    if let Err(msg) = validate_username(&req.username) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid username".to_string()),
        }));
    }
    if let Err(msg) = validate_password(&req.password) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid password".to_string()),
        }));
    }

    // Hash password
    let password_hash = match hash_password(req.password.clone()).await {
        Ok(hash) => hash,
        Err(e) => {
            if matches!(e, PasswordWorkError::Busy) {
                return Ok(crate::api::auth::busy_response());
            }
            error!("Error hashing password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to hash password".to_string(),
                message: None,
            }));
        },
    };

    // Create admin user
    let user = match db_service
        .create_user(
            &req.username,
            &req.full_name,
            &password_hash,
            true, // is_admin
            true, // is_active
        )
        .await
    {
        Ok(u) => u,
        Err(e) => {
            error!("Error creating admin user: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: format!("Failed to create user: {e}"),
                message: None,
            }));
        },
    };

    // Create default settings for user
    if let Err(e) = db_service.create_user_settings(user.id).await {
        warn!("Failed to create user settings: {}", e);
    }

    // Generate token
    let token = match generate_token(&user) {
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

    info!(
        "Initial setup completed - created admin user: {}",
        user.username
    );

    Ok(HttpResponse::Created().json(ApiResponse {
        success: true,
        data: Some(LoginResponse {
            token,
            user: user.into(),
        }),
        message: Some("Setup completed successfully".to_string()),
        error: None,
    }))
}

/// User login
#[post("/auth/login")]
pub async fn login(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    req: web::Json<LoginRequest>,
) -> Result<impl Responder> {
    let db_service = DatabaseService::new(pool.get_ref().clone());
    let client_ip = client_ip_key(&http_req);

    // Locked-out username/IP pairs are turned away before any password work
    if let Err(wait) = throttle::global().check(Subject::Username(&req.username), &client_ip) {
        return Ok(throttled_response(wait));
    }

    // Find user by username
    let user = match db_service.get_user_by_username(&req.username).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            // Spend the same Argon2 time as a real check so timing does not reveal
            // whether the username exists
            if let Err(e) = verify_against_dummy(req.password.clone()).await {
                error!("Error during dummy password verification: {}", e);
                return Ok(match e {
                    PasswordWorkError::Busy => busy_response(),
                    PasswordWorkError::Hash(_) => {
                        HttpResponse::InternalServerError().json(ErrorResponse {
                            success: false,
                            error: "Password verification failed".to_string(),
                            message: None,
                        })
                    },
                });
            }
            throttle::global().record_failure(Subject::Username(&req.username), &client_ip);
            // Don't reveal whether username exists
            return Ok(HttpResponse::Unauthorized().json(ErrorResponse {
                success: false,
                error: "Invalid credentials".to_string(),
                message: Some("Username or password is incorrect".to_string()),
            }));
        },
        Err(e) => {
            error!("Database error during login: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Database error".to_string(),
                message: None,
            }));
        },
    };

    // Check if user is active
    if !user.is_active {
        return Ok(HttpResponse::Forbidden().json(ErrorResponse {
            success: false,
            error: "Account deactivated".to_string(),
            message: Some(
                "Your account has been deactivated. Contact an administrator.".to_string(),
            ),
        }));
    }

    // Verify password
    let password_valid =
        match verify_password(req.password.clone(), user.password_hash.clone()).await {
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
        throttle::global().record_failure(Subject::Username(&req.username), &client_ip);
        return Ok(HttpResponse::Unauthorized().json(ErrorResponse {
            success: false,
            error: "Invalid credentials".to_string(),
            message: Some("Username or password is incorrect".to_string()),
        }));
    }
    throttle::global().record_success(Subject::Username(&req.username), &client_ip);

    // Check if user has TOTP enabled with a mode that requires login verification.
    // A failed lookup must not fall through to a full token.
    let totp_settings = match db_service.get_totp_settings(user.id).await {
        Ok(settings) => settings,
        Err(e) => {
            error!("Error loading TOTP settings during login: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };

    if totp_settings.is_some_and(|settings| settings.login_requires_totp()) {
        // Generate a partial token (5-min, totp_pending=true)
        let partial_token = match generate_partial_token(&user) {
            Ok(t) => t,
            Err(e) => {
                error!("Error generating partial token: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "Failed to generate token".to_string(),
                    message: None,
                }));
            },
        };

        info!("User {} requires TOTP verification", user.username);

        return Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(LoginTotpRequiredResponse {
                requires_totp: true,
                partial_token,
                user: user.into(),
            }),
            message: Some("TOTP verification required".to_string()),
            error: None,
        }));
    }

    // Generate token (normal login - no TOTP or recovery_only mode)
    let token = match generate_token(&user) {
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

    info!("User logged in: {}", user.username);

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(LoginResponse {
            token,
            user: user.into(),
        }),
        message: Some("Login successful".to_string()),
        error: None,
    }))
}

/// 403 for a registration without a usable invitation. The same response for every way an
/// invitation can be unusable (unknown, used, revoked, expired, not an invitation), so it says
/// nothing about which one it was.
fn invitation_required() -> HttpResponse {
    HttpResponse::Forbidden().json(ErrorResponse {
        success: false,
        error: "Registration is by invitation only".to_string(),
        message: Some("A valid invitation link is required to register".to_string()),
    })
}

fn registration_closed() -> HttpResponse {
    HttpResponse::Forbidden().json(ErrorResponse {
        success: false,
        error: "Registration is closed".to_string(),
        message: Some("New accounts can only be created by an administrator".to_string()),
    })
}

fn username_taken() -> HttpResponse {
    HttpResponse::Conflict().json(ErrorResponse {
        success: false,
        error: "Username already taken".to_string(),
        message: Some("Please choose a different username".to_string()),
    })
}

fn database_error(what: &str, e: &dyn std::fmt::Display) -> HttpResponse {
    error!("{what}: {e}");
    HttpResponse::InternalServerError().json(ErrorResponse {
        success: false,
        error: "Database error".to_string(),
        message: None,
    })
}

/// Register new user (after initial setup). Who may register depends on the registration mode:
/// anyone when open, only the holder of a valid invitation when invite-only, nobody when closed.
/// An invitation always makes an ordinary, active user.
#[post("/auth/register")]
pub async fn register(
    pool: web::Data<Pool>,
    req: web::Json<crate::models::CreateUserRequest>,
) -> Result<impl Responder> {
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Check that at least one user exists (initial setup has been done)
    match db_service.get_user_count().await {
        Ok(0) => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Initial setup required".to_string(),
                message: Some(
                    "Please complete the initial setup before registering users".to_string(),
                ),
            }));
        },
        Err(e) => return Ok(database_error("Database error checking user count", &e)),
        _ => {},
    }

    // Who may register. The invitation is only looked at (not used up) until the account exists.
    let mode = match db_service.get_registration_mode().await {
        Ok(mode) => mode,
        Err(e) => return Ok(database_error("Error reading registration mode", &e)),
    };
    let invite_hash = match mode {
        RegistrationMode::Closed => return Ok(registration_closed()),
        RegistrationMode::Open => None,
        RegistrationMode::Invite => {
            let hash = req
                .invite_token
                .as_deref()
                .filter(|token| tokens::is_well_formed(token))
                .map(tokens::hash_token);
            let usable = match &hash {
                Some(hash) => match db_service.invite_is_redeemable(hash).await {
                    Ok(usable) => usable,
                    Err(e) => return Ok(database_error("Error checking invitation", &e)),
                },
                None => false,
            };
            if !usable {
                return Ok(invitation_required());
            }
            hash
        },
    };

    // Validate username
    if let Err(msg) = validate_username(&req.username) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid username".to_string()),
        }));
    }

    // Validate password
    if let Err(msg) = validate_password(&req.password) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid password".to_string()),
        }));
    }

    // Check if username already exists
    if let Ok(Some(_)) = db_service.get_user_by_username(&req.username).await {
        return Ok(username_taken());
    }

    // Hash password
    let password_hash = match hash_password(req.password.clone()).await {
        Ok(hash) => hash,
        Err(e) => {
            if matches!(e, PasswordWorkError::Busy) {
                return Ok(crate::api::auth::busy_response());
            }
            error!("Error hashing password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to process password".to_string(),
                message: None,
            }));
        },
    };

    // Create user (non-admin, active)
    let user = if let Some(token_hash) = invite_hash {
        match db_service
            .register_with_invite(&token_hash, &req.username, &req.full_name, &password_hash)
            .await
        {
            Ok(InviteRegistration::Registered { user, token_id }) => {
                info!(
                    target: "audit",
                    "invite_redeemed token={token_id} subject={0} actor={0}",
                    user.id
                );
                user
            },
            // Used by someone else between the check and now
            Ok(InviteRegistration::InviteInvalid) => return Ok(invitation_required()),
            Ok(InviteRegistration::UsernameTaken) => return Ok(username_taken()),
            Err(e) => {
                error!("Error creating user: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "An internal error occurred".to_string(),
                    message: Some("Failed to create account".to_string()),
                }));
            },
        }
    } else {
        let user = match db_service
            .create_user(
                &req.username,
                &req.full_name,
                &password_hash,
                false, // not admin
                true,  // active
            )
            .await
        {
            Ok(u) => u,
            Err(e) => {
                error!("Error creating user: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "An internal error occurred".to_string(),
                    message: Some("Failed to create account".to_string()),
                }));
            },
        };

        // Create default settings for the user
        if let Err(e) = db_service.create_user_settings(user.id).await {
            warn!(
                "Failed to create user settings for {}: {}",
                user.username, e
            );
        }
        user
    };

    // Generate token for immediate login
    let token = match generate_token(&user) {
        Ok(t) => t,
        Err(e) => {
            error!("Error generating token: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Account created but login failed".to_string(),
                message: Some("Please log in manually".to_string()),
            }));
        },
    };

    info!("New user registered: {}", user.username);

    Ok(HttpResponse::Created().json(ApiResponse {
        success: true,
        data: Some(LoginResponse {
            token,
            user: user.into(),
        }),
        message: Some("Registration successful".to_string()),
        error: None,
    }))
}

/// Whether an invitation can be used right now. Answers only yes or no, and the same "no" for
/// every kind of unusable token (and for any token while registration is closed).
#[post("/auth/invite/check")]
pub async fn check_invite(
    pool: web::Data<Pool>,
    body: web::Json<TokenRequest>,
) -> Result<impl Responder> {
    let db_service = DatabaseService::new(pool.get_ref().clone());

    let valid = if tokens::is_well_formed(&body.token) {
        match db_service.get_registration_mode().await {
            Ok(RegistrationMode::Invite) => {
                match db_service
                    .invite_is_redeemable(&tokens::hash_token(&body.token))
                    .await
                {
                    Ok(valid) => valid,
                    Err(e) => return Ok(database_error("Error checking invitation", &e)),
                }
            },
            Ok(_) => false,
            Err(e) => return Ok(database_error("Error reading registration mode", &e)),
        }
    } else {
        false
    };

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(InviteCheckResponse { valid }),
        message: None,
        error: None,
    }))
}

/// 400 for a reset link that cannot be used; the same for every reason.
fn invalid_reset_link() -> HttpResponse {
    HttpResponse::BadRequest().json(ErrorResponse {
        success: false,
        error: "Invalid or expired link".to_string(),
        message: Some("Ask an administrator for a new password reset link".to_string()),
    })
}

/// Set a new password with a reset link from an administrator. Does not sign anyone in: the user
/// signs in afterwards, with two-factor authentication where it is on.
#[post("/auth/reset-password")]
pub async fn reset_password_with_token(
    pool: web::Data<Pool>,
    body: web::Json<ResetPasswordRequest>,
) -> Result<impl Responder> {
    // The same password policy as registration
    if let Err(msg) = validate_password(&body.new_password) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid password".to_string()),
        }));
    }

    if !tokens::is_well_formed(&body.token) {
        return Ok(invalid_reset_link());
    }
    let token_hash = tokens::hash_token(&body.token);
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Password hashing is expensive and this endpoint is public: do it only for a link that is
    // usable. The hash then goes through the same Argon2 limit as every other password hash.
    match db_service.reset_token_is_redeemable(&token_hash).await {
        Ok(true) => {},
        Ok(false) => return Ok(invalid_reset_link()),
        Err(e) => return Ok(database_error("Error checking reset link", &e)),
    }

    let password_hash = match hash_password(body.new_password.clone()).await {
        Ok(hash) => hash,
        Err(e) => {
            if matches!(e, PasswordWorkError::Busy) {
                return Ok(crate::api::auth::busy_response());
            }
            error!("Error hashing password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to process password".to_string(),
                message: None,
            }));
        },
    };

    match db_service
        .reset_password_with_token(&token_hash, &password_hash)
        .await
    {
        Ok(PasswordReset::Done {
            user_id, token_id, ..
        }) => {
            info!(
                target: "audit",
                "password_reset_by_link token={token_id} subject={user_id} actor={user_id}"
            );
            audit_sessions_revoked(user_id, &user_id.to_string(), "password_reset_link");
            Ok(HttpResponse::Ok().json(ApiResponse::<()> {
                success: true,
                data: None,
                message: Some("Password updated. You can now sign in.".to_string()),
                error: None,
            }))
        },
        // Used or revoked between the check and now
        Ok(PasswordReset::LinkInvalid) => Ok(invalid_reset_link()),
        Err(e) => Ok(database_error("Error resetting password", &e)),
    }
}

// ==================== Authenticated User Endpoints ====================

/// Get current user's profile
#[get("/auth/me")]
pub async fn get_current_user(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth_ctx = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_user_by_id(auth_ctx.user_id).await {
        Ok(Some(user)) => Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(UserResponse::from(user)),
            message: None,
            error: None,
        })),
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "User not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error getting user profile: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Update current user's profile
#[put("/auth/me")]
pub async fn update_current_user(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<UpdateProfileRequest>,
) -> Result<impl Responder> {
    let auth_ctx = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .update_user_profile(auth_ctx.user_id, body.full_name.as_deref())
        .await
    {
        Ok(Some(user)) => {
            info!("User {} updated their profile", auth_ctx.username);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(UserResponse::from(user)),
                message: Some("Profile updated successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "User not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error updating user profile: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Change current user's password
#[put("/auth/password")]
pub async fn change_password(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<ChangePasswordRequest>,
) -> Result<impl Responder> {
    let auth_ctx = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    // A stolen token must not be able to guess the current password without limit
    let throttle_subject = Subject::UserId(auth_ctx.user_id);
    let client_ip = client_ip_key(&req);
    if let Err(wait) = throttle::global().check(throttle_subject, &client_ip) {
        return Ok(throttled_response(wait));
    }

    // Validate new password
    if let Err(msg) = validate_password(&body.new_password) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid new password".to_string()),
        }));
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Get user to verify current password
    let user = match db_service.get_user_by_id(auth_ctx.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: None,
            }));
        },
        Err(e) => {
            error!("Error getting user: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Database error".to_string(),
                message: None,
            }));
        },
    };

    // Verify current password
    let password_valid =
        match verify_password(body.current_password.clone(), user.password_hash).await {
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
        throttle::global().record_failure(throttle_subject, &client_ip);
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Current password is incorrect".to_string(),
            message: None,
        }));
    }
    throttle::global().record_success(throttle_subject, &client_ip);

    // Hash new password
    let password_hash = match hash_password(body.new_password.clone()).await {
        Ok(hash) => hash,
        Err(e) => {
            if matches!(e, PasswordWorkError::Busy) {
                return Ok(crate::api::auth::busy_response());
            }
            error!("Error hashing password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to hash password".to_string(),
                message: None,
            }));
        },
    };

    // Update password; this also revokes every other session of the account
    let new_version = match db_service
        .update_user_password(auth_ctx.user_id, &password_hash)
        .await
    {
        Ok(Some(version)) => version,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: None,
            }));
        },
        Err(e) => {
            error!("Error updating password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to update password".to_string(),
                message: None,
            }));
        },
    };

    info!("User {} changed their password", auth_ctx.username);
    audit_sessions_revoked(
        auth_ctx.user_id,
        &auth_ctx.user_id.to_string(),
        "password_change",
    );

    // The current device stays signed in with a token for the new session generation
    let token = match issue_token(
        auth_ctx.user_id,
        &auth_ctx.username,
        auth_ctx.is_admin,
        new_version,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("Error generating token after password change: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Password changed, but a new session token could not be issued. \
                        Please sign in again."
                    .to_string(),
                message: None,
            }));
        },
    };

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(SessionTokenResponse { token }),
        message: Some("Password changed successfully".to_string()),
        error: None,
    }))
}

/// Sign out every other device: bump the session generation and hand the caller a fresh token
#[post("/auth/logout-all")]
pub async fn logout_all_devices(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth_ctx = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());
    let new_version = match db_service.bump_token_version(auth_ctx.user_id).await {
        Ok(Some(version)) => version,
        Ok(None) => return Ok(session_revoked_response()),
        Err(e) => {
            error!("Error revoking sessions: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to sign out other devices".to_string(),
                message: None,
            }));
        },
    };

    audit_sessions_revoked(
        auth_ctx.user_id,
        &auth_ctx.user_id.to_string(),
        "logout_all_devices",
    );

    let token = match issue_token(
        auth_ctx.user_id,
        &auth_ctx.username,
        auth_ctx.is_admin,
        new_version,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("Error generating token after revoking sessions: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Other devices were signed out, but a new session token could not be \
                        issued. Please sign in again."
                    .to_string(),
                message: None,
            }));
        },
    };

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(SessionTokenResponse { token }),
        message: Some("Signed out all other devices".to_string()),
        error: None,
    }))
}

// ==================== User Settings Endpoints ====================

/// Get current user's settings
#[get("/auth/settings")]
pub async fn get_user_settings(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth_ctx = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .get_or_create_user_settings(auth_ctx.user_id)
        .await
    {
        Ok(settings) => Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(settings),
            message: None,
            error: None,
        })),
        Err(e) => {
            error!("Error getting user settings: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Update current user's settings
#[put("/auth/settings")]
pub async fn update_user_settings(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<UpdateUserSettingsRequest>,
) -> Result<impl Responder> {
    let auth_ctx = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Ensure settings exist
    let _ = db_service
        .get_or_create_user_settings(auth_ctx.user_id)
        .await;

    match db_service
        .update_user_settings(auth_ctx.user_id, body.into_inner())
        .await
    {
        Ok(Some(settings)) => Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(settings),
            message: Some("Settings updated successfully".to_string()),
            error: None,
        })),
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "Settings not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error updating user settings: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

// ==================== Admin User Management Endpoints ====================

/// Get all users (admin only)
#[get("/admin/users")]
pub async fn admin_get_users(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let _auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_all_users().await {
        Ok(users) => Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(users.clone()),
            message: Some(format!("Retrieved {count} users", count = users.len())),
            error: None,
        })),
        Err(e) => {
            error!("Error getting users: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Get a specific user (admin only)
#[get("/admin/users/{id}")]
pub async fn admin_get_user(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
) -> Result<impl Responder> {
    let _auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let user_id = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_user_by_id(user_id).await {
        Ok(Some(user)) => Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(UserResponse::from(user)),
            message: None,
            error: None,
        })),
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "User not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error getting user: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Create a new user (admin only)
#[post("/admin/users")]
pub async fn admin_create_user(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<AdminCreateUserRequest>,
) -> Result<impl Responder> {
    let _auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    // Validate input
    if let Err(msg) = validate_username(&body.username) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid username".to_string()),
        }));
    }
    if let Err(msg) = validate_password(&body.password) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid password".to_string()),
        }));
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Check if username already exists
    if let Ok(Some(_)) = db_service.get_user_by_username(&body.username).await {
        return Ok(HttpResponse::Conflict().json(ErrorResponse {
            success: false,
            error: "Username already exists".to_string(),
            message: None,
        }));
    }

    // Hash password
    let password_hash = match hash_password(body.password.clone()).await {
        Ok(hash) => hash,
        Err(e) => {
            if matches!(e, PasswordWorkError::Busy) {
                return Ok(crate::api::auth::busy_response());
            }
            error!("Error hashing password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to hash password".to_string(),
                message: None,
            }));
        },
    };

    // Create user
    match db_service
        .create_user(
            &body.username,
            &body.full_name,
            &password_hash,
            body.is_admin,
            body.is_active,
        )
        .await
    {
        Ok(user) => {
            // Create default settings
            let _ = db_service.create_user_settings(user.id).await;

            info!("Admin created new user: {}", user.username);
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(UserResponse::from(user)),
                message: Some("User created successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error creating user: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: format!("Failed to create user: {e}"),
                message: None,
            }))
        },
    }
}

/// Update a user (admin only)
#[put("/admin/users/{id}")]
pub async fn admin_update_user(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
    body: web::Json<AdminUpdateUserRequest>,
) -> Result<impl Responder> {
    let auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let user_id = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Validate username if provided
    if let Some(ref username) = body.username {
        if let Err(msg) = validate_username(username) {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: msg.to_string(),
                message: Some("Invalid username".to_string()),
            }));
        }
    }

    // Prevent admin from demoting themselves
    if user_id == auth_ctx.user_id {
        if let Some(false) = body.is_admin {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Cannot remove your own admin privileges".to_string(),
                message: None,
            }));
        }
        if let Some(false) = body.is_active {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Cannot deactivate your own account".to_string(),
                message: None,
            }));
        }
    }

    // The target as stored before this request (used for the last-admin guard and audit)
    let target_before = db_service.get_user_by_id(user_id).await.ok().flatten();
    // Session generation to compare the final state against, so only role/active changes are
    // logged as such (a password reset bumps it separately, above)
    let mut baseline_version = target_before.as_ref().map(|u| u.token_version);

    // Protect the last active admin from demotion or deactivation
    if body.is_admin == Some(false) || body.is_active == Some(false) {
        if let Some(ref target) = target_before {
            if is_last_active_admin(&db_service, target).await {
                return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                    success: false,
                    error: "Cannot remove admin privileges from, or deactivate, the last admin"
                        .to_string(),
                    message: None,
                }));
            }
        }
    }

    // Admin password reset — handled separately from metadata update
    if let Some(ref password) = body.password {
        if let Err(msg) = validate_password(password) {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: msg.to_string(),
                message: Some("Invalid password".to_string()),
            }));
        }

        let password_hash = match hash_password(password.clone()).await {
            Ok(h) => h,
            Err(e) => {
                if matches!(e, PasswordWorkError::Busy) {
                    return Ok(crate::api::auth::busy_response());
                }
                error!("Error hashing password: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "Failed to process password".to_string(),
                    message: None,
                }));
            },
        };

        match db_service
            .update_user_password(user_id, &password_hash)
            .await
        {
            Ok(Some(version)) => {
                // The reset already bumped the version; later changes are compared to it
                baseline_version = Some(version);
                audit_sessions_revoked(
                    user_id,
                    &auth_ctx.user_id.to_string(),
                    "admin_password_reset",
                );
            },
            Ok(None) => {},
            Err(e) => {
                error!("Error updating user password: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: format!("Failed to update password: {e}"),
                    message: None,
                }));
            },
        }

        info!(
            "Admin {} reset password for user {}",
            auth_ctx.username, user_id
        );
    }

    match db_service
        .admin_update_user(user_id, body.into_inner())
        .await
    {
        Ok(Some(user)) => {
            info!("Admin updated user: {}", user.username);
            if baseline_version.is_some_and(|before| before != user.token_version) {
                audit_sessions_revoked(
                    user.id,
                    &auth_ctx.user_id.to_string(),
                    "admin_role_or_active_change",
                );
            }
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(UserResponse::from(user)),
                message: Some("User updated successfully".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "User not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error updating user: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: format!("Failed to update user: {e}"),
                message: None,
            }))
        },
    }
}

/// Delete a user (admin only)
#[delete("/admin/users/{id}")]
pub async fn admin_delete_user(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
) -> Result<impl Responder> {
    let auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };

    let user_id = path.into_inner();

    // Prevent admin from deleting themselves
    if user_id == auth_ctx.user_id {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Cannot delete your own account".to_string(),
            message: None,
        }));
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Check if this is the last admin
    let target_user = match db_service.get_user_by_id(user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: None,
            }));
        },
        Err(e) => {
            error!("Error getting user: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Database error".to_string(),
                message: None,
            }));
        },
    };

    if is_last_active_admin(&db_service, &target_user).await {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Cannot delete the last admin".to_string(),
            message: None,
        }));
    }

    match db_service.delete_user(user_id).await {
        Ok(true) => {
            info!(
                "Admin deleted user: {} (ID: {})",
                target_user.username, user_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: None::<()>,
                message: Some("User deleted successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "User not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error deleting user: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: format!("Failed to delete user: {e}"),
                message: None,
            }))
        },
    }
}

// ==================== Inventory Sharing Endpoints ====================

// ==================== Registration mode, invitations and reset links (admin) ====================

/// Invitation lifetime, in hours: 7 days unless the admin asks otherwise, at most 30 days.
const INVITE_DEFAULT_HOURS: i64 = 7 * 24;
const INVITE_MAX_HOURS: i64 = 30 * 24;
/// Reset link lifetime, in hours: 24 hours unless the admin asks otherwise, at most 7 days.
const RESET_DEFAULT_HOURS: i64 = 24;
const RESET_MAX_HOURS: i64 = 7 * 24;

/// The lifetime to use, or the 400 to send for one out of range.
fn lifetime_hours(requested: Option<i64>, default: i64, max: i64) -> Result<i32, HttpResponse> {
    let hours = requested.unwrap_or(default);
    if (1..=max).contains(&hours) {
        return Ok(i32::try_from(hours).unwrap_or(i32::MAX));
    }
    Err(HttpResponse::BadRequest().json(ErrorResponse {
        success: false,
        error: "Invalid expiry".to_string(),
        message: Some(format!("expires_in_hours must be between 1 and {max}")),
    }))
}

async fn registration_settings_response(db_service: &DatabaseService) -> HttpResponse {
    let registration_mode = match db_service.get_registration_mode().await {
        Ok(mode) => mode,
        Err(e) => return database_error("Error reading registration mode", &e),
    };
    let upgrade_notice = match db_service.registration_upgrade_notice_pending().await {
        Ok(pending) => pending,
        Err(e) => return database_error("Error reading registration notice", &e),
    };
    HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(RegistrationSettingsResponse {
            registration_mode,
            upgrade_notice,
        }),
        message: None,
        error: None,
    })
}

/// Who may register, and whether to show the upgrade notice (admin only)
#[get("/admin/settings/registration")]
pub async fn admin_get_registration_settings(
    pool: web::Data<Pool>,
    req: HttpRequest,
) -> Result<impl Responder> {
    if let Err(response) = require_admin(&req, pool.get_ref()).await {
        return Ok(response);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());
    Ok(registration_settings_response(&db_service).await)
}

/// Choose who may register and/or dismiss the upgrade notice (admin only)
#[put("/admin/settings/registration")]
pub async fn admin_update_registration_settings(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<UpdateRegistrationSettingsRequest>,
) -> Result<impl Responder> {
    let auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };
    if body.registration_mode.is_none() && body.dismiss_upgrade_notice.is_none() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Nothing to change".to_string(),
            message: Some("Send registration_mode and/or dismiss_upgrade_notice".to_string()),
        }));
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    if let Some(mode) = body.registration_mode {
        if let Err(e) = db_service.set_registration_mode(mode).await {
            return Ok(database_error("Error setting registration mode", &e));
        }
        info!(
            target: "audit",
            "registration_mode_changed mode={} actor={}",
            mode.as_str(),
            auth_ctx.user_id
        );
    }
    if body.dismiss_upgrade_notice == Some(true) {
        if let Err(e) = db_service.dismiss_registration_upgrade_notice().await {
            return Ok(database_error("Error dismissing registration notice", &e));
        }
        info!(
            target: "audit",
            "registration_notice_dismissed actor={}",
            auth_ctx.user_id
        );
    }
    Ok(registration_settings_response(&db_service).await)
}

/// Create an invitation link (admin only). The token is returned once and never shown again.
#[post("/admin/invites")]
pub async fn admin_create_invite(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<IssueTokenRequest>,
) -> Result<impl Responder> {
    let auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };
    let hours = match lifetime_hours(
        body.expires_in_hours,
        INVITE_DEFAULT_HOURS,
        INVITE_MAX_HOURS,
    ) {
        Ok(hours) => hours,
        Err(response) => return Ok(response),
    };
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_registration_mode().await {
        Ok(RegistrationMode::Closed) => {
            return Ok(HttpResponse::Conflict().json(ErrorResponse {
                success: false,
                error: "Registration is closed".to_string(),
                message: Some(
                    "Invitations cannot be created while registration is closed. \
                     Change the registration mode first."
                        .to_string(),
                ),
            }));
        },
        Ok(_) => {},
        Err(e) => return Ok(database_error("Error reading registration mode", &e)),
    }

    let token = tokens::generate_token();
    match db_service
        .create_invite(auth_ctx.user_id, &tokens::hash_token(&token), hours)
        .await
    {
        Ok((id, expires_at)) => {
            info!(
                target: "audit",
                "invite_created id={id} actor={} expires_at={expires_at}",
                auth_ctx.user_id
            );
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(IssuedTokenResponse {
                    id,
                    token,
                    expires_at,
                }),
                message: Some("Invitation created. The link is shown only once.".to_string()),
                error: None,
            }))
        },
        Err(e) => Ok(database_error("Error creating invitation", &e)),
    }
}

/// Invitations that can still be used (admin only). Never includes the secret.
#[get("/admin/invites")]
pub async fn admin_list_invites(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    if let Err(response) = require_admin(&req, pool.get_ref()).await {
        return Ok(response);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());
    match db_service.list_open_invites().await {
        Ok(invites) => Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(invites),
            message: None,
            error: None,
        })),
        Err(e) => Ok(database_error("Error listing invitations", &e)),
    }
}

/// Revoke an unused invitation (admin only)
#[delete("/admin/invites/{id}")]
pub async fn admin_revoke_invite(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
) -> Result<impl Responder> {
    let auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };
    let id = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());
    match db_service.revoke_invite(id).await {
        Ok(true) => {
            info!(target: "audit", "invite_revoked id={id} actor={}", auth_ctx.user_id);
            Ok(HttpResponse::Ok().json(ApiResponse::<()> {
                success: true,
                data: None,
                message: Some("Invitation revoked".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "Invitation not found".to_string(),
            message: None,
        })),
        Err(e) => Ok(database_error("Error revoking invitation", &e)),
    }
}

/// Create a password reset link for a user (admin only). Replaces any earlier unused link for
/// them. The token is returned once and never shown again.
#[post("/admin/users/{id}/reset-link")]
pub async fn admin_issue_reset_link(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
    body: web::Json<IssueTokenRequest>,
) -> Result<impl Responder> {
    let auth_ctx = match require_admin(&req, pool.get_ref()).await {
        Ok(ctx) => ctx,
        Err(response) => return Ok(response),
    };
    let user_id = path.into_inner();
    let hours = match lifetime_hours(body.expires_in_hours, RESET_DEFAULT_HOURS, RESET_MAX_HOURS) {
        Ok(hours) => hours,
        Err(response) => return Ok(response),
    };
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_user_by_id(user_id).await {
        Ok(Some(_)) => {},
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: None,
            }));
        },
        Err(e) => return Ok(database_error("Error looking up user", &e)),
    }

    let token = tokens::generate_token();
    match db_service
        .issue_reset_token(
            user_id,
            auth_ctx.user_id,
            &tokens::hash_token(&token),
            hours,
        )
        .await
    {
        Ok(Some((id, expires_at))) => {
            info!(
                target: "audit",
                "reset_link_issued token={id} subject={user_id} actor={} expires_at={expires_at}",
                auth_ctx.user_id
            );
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(IssuedTokenResponse {
                    id,
                    token,
                    expires_at,
                }),
                message: Some("Reset link created. It is shown only once.".to_string()),
                error: None,
            }))
        },
        // Deleted just now
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "User not found".to_string(),
            message: None,
        })),
        Err(e) => Ok(database_error("Error creating reset link", &e)),
    }
}

/// Get shares for an inventory
#[get("/inventories/{id}/shares")]
pub async fn get_inventory_shares(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let inventory_id = path.into_inner();
    if let Err(resp) = super::authorize_inventory(
        pool.get_ref(),
        &req,
        inventory_id,
        super::Perm::ManageSharing,
    )
    .await
    {
        return Ok(resp);
    }
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_inventory_shares(inventory_id).await {
        Ok(shares) => {
            info!(
                "Retrieved {} shares for inventory {}",
                shares.len(),
                inventory_id
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(shares),
                message: Some("Shares retrieved successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving shares: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Share an inventory with another user
#[post("/inventories/{id}/shares")]
pub async fn create_inventory_share(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<i32>,
    body: web::Json<CreateInventoryShareRequest>,
) -> Result<impl Responder> {
    let inventory_id = path.into_inner();
    let auth = match super::authorize_inventory(
        pool.get_ref(),
        &req,
        inventory_id,
        super::Perm::ManageSharing,
    )
    .await
    {
        Ok(a) => a,
        Err(resp) => return Ok(resp),
    };
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Find the user to share with
    let target_user = match db_service
        .get_user_by_username(&body.shared_with_username)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: Some(format!(
                    "No user found with username or email: {username}",
                    username = body.shared_with_username
                )),
            }));
        },
        Err(e) => {
            error!("Error finding user: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };

    // Don't allow sharing with self
    if target_user.id == auth.user_id {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Cannot share with yourself".to_string(),
            message: None,
        }));
    }

    match db_service
        .create_inventory_share(
            inventory_id,
            target_user.id,
            auth.user_id,
            body.permission_level,
        )
        .await
    {
        Ok(share) => {
            info!(
                "User {} shared inventory {} with {} (permission: {:?})",
                auth.username, inventory_id, target_user.username, body.permission_level
            );
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(share),
                message: Some(format!(
                    "Inventory shared with {username}",
                    username = target_user.username
                )),
                error: None,
            }))
        },
        Err(e) => {
            // Check for duplicate share
            if e.to_string().contains("duplicate") || e.to_string().contains("unique") {
                return Ok(HttpResponse::Conflict().json(ErrorResponse {
                    success: false,
                    error: "Already shared".to_string(),
                    message: Some(format!(
                        "This inventory is already shared with {username}",
                        username = target_user.username
                    )),
                }));
            }
            error!("Error creating share: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Update share permission level
#[put("/shares/{share_id}")]
pub async fn update_inventory_share(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
    body: web::Json<UpdateInventoryShareRequest>,
) -> Result<impl Responder> {
    let share_id = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Only someone who can manage sharing on the share's inventory may change it
    if let Err(resp) = super::authorize_resolved(
        pool.get_ref(),
        &req,
        db_service
            .get_share_inventory_and_user(share_id)
            .await
            .map(|found| found.map(|(inventory_id, _)| inventory_id)),
        "Share",
        super::Perm::ManageSharing,
    )
    .await
    {
        return Ok(resp);
    }

    // Update the share permission
    match db_service
        .update_inventory_share(share_id, body.permission_level)
        .await
    {
        Ok(Some(share)) => {
            info!(
                "Updated share {} permission to {:?}",
                share_id, body.permission_level
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(share),
                message: Some("Share permission updated".to_string()),
                error: None,
            }))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "Share not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error updating share: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Remove a share
#[delete("/shares/{share_id}")]
pub async fn delete_inventory_share(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let share_id = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // The sharee may remove their own share (leave the inventory); anyone else needs
    // permission to manage sharing on the inventory.
    let share_info = match db_service.get_share_inventory_and_user(share_id).await {
        Ok(Some(info)) => info,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "Resource not found".to_string(),
                message: Some("Resource not found".to_string()),
            }))
        },
        Err(e) => {
            error!("Error looking up share: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };
    let (share_inventory_id, shared_with_user_id) = share_info;
    if shared_with_user_id != auth.user_id {
        if let Err(resp) = super::authorize_inventory(
            pool.get_ref(),
            &req,
            share_inventory_id,
            super::Perm::ManageSharing,
        )
        .await
        {
            return Ok(resp);
        }
    }

    match db_service.delete_inventory_share(share_id).await {
        Ok(true) => {
            info!("User {} deleted share {}", auth.username, share_id);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: None::<()>,
                message: Some("Share removed successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "Share not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error deleting share: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Get inventories accessible to the current user (owned + shared + all-access)
#[get("/auth/inventories")]
pub async fn get_my_inventories(pool: web::Data<Pool>, req: HttpRequest) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_accessible_inventories(auth.user_id).await {
        Ok(inventories) => {
            info!(
                "User {} retrieved {} accessible inventories",
                auth.username,
                inventories.len()
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(inventories),
                message: None,
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving inventories: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

// ==================== User Access Grant Endpoints (All Access Tier) ====================

/// Get users who have All Access to my inventories (grants I've made)
#[get("/auth/access-grants")]
pub async fn get_my_access_grants(
    pool: web::Data<Pool>,
    req: HttpRequest,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .get_user_access_grants_by_grantor(auth.user_id)
        .await
    {
        Ok(grants) => {
            info!(
                "User {} retrieved {} access grants they've made",
                auth.username,
                grants.len()
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(grants),
                message: Some("Access grants retrieved successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving access grants: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Get users who have granted me All Access to their inventories
#[get("/auth/access-grants/received")]
pub async fn get_received_access_grants(
    pool: web::Data<Pool>,
    req: HttpRequest,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .get_user_access_grants_by_grantee(auth.user_id)
        .await
    {
        Ok(grants) => {
            info!(
                "User {} retrieved {} received access grants",
                auth.username,
                grants.len()
            );
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(grants),
                message: Some("Received access grants retrieved successfully".to_string()),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error retrieving received access grants: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Grant All Access to another user (gives them access to all my inventories)
#[post("/auth/access-grants")]
pub async fn create_access_grant(
    pool: web::Data<Pool>,
    req: HttpRequest,
    body: web::Json<CreateUserAccessGrantRequest>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Find the user to grant access to
    let target_user = match db_service
        .get_user_by_username(&body.grantee_username)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: Some(format!(
                    "No user found with username or email: {username}",
                    username = body.grantee_username
                )),
            }));
        },
        Err(e) => {
            error!("Error finding user: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };

    // Don't allow granting access to self
    if target_user.id == auth.user_id {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Cannot grant access to yourself".to_string(),
            message: None,
        }));
    }

    match db_service
        .create_user_access_grant(auth.user_id, target_user.id)
        .await
    {
        Ok(grant) => {
            info!(
                "User {} granted All Access to {} for all their inventories",
                auth.username, target_user.username
            );
            Ok(HttpResponse::Created().json(ApiResponse {
                success: true,
                data: Some(grant),
                message: Some(format!(
                    "{username} now has All Access to all your inventories",
                    username = target_user.username
                )),
                error: None,
            }))
        },
        Err(e) => {
            // Check for duplicate grant
            if e.to_string().contains("duplicate") || e.to_string().contains("unique") {
                return Ok(HttpResponse::Conflict().json(ErrorResponse {
                    success: false,
                    error: "Already granted".to_string(),
                    message: Some(format!(
                        "{username} already has All Access to your inventories",
                        username = target_user.username
                    )),
                }));
            }
            error!("Error creating access grant: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Revoke All Access grant (remove someone's access to all my inventories)
#[delete("/auth/access-grants/{grant_id}")]
pub async fn delete_access_grant(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let grant_id = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Verify the grant belongs to the current user (as grantor)
    let grant = match db_service.get_user_access_grant_by_id(grant_id).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "Access grant not found".to_string(),
                message: None,
            }));
        },
        Err(e) => {
            error!("Error finding access grant: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };

    // Only the grantor can revoke their own grants (or admin)
    if grant.grantor_user_id != auth.user_id && !auth.is_admin {
        return Ok(HttpResponse::Forbidden().json(ErrorResponse {
            success: false,
            error: "Access denied".to_string(),
            message: Some("You can only revoke access grants you have made".to_string()),
        }));
    }

    match db_service.delete_user_access_grant(grant_id).await {
        Ok(true) => {
            info!("User {} revoked access grant {}", auth.username, grant_id);
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: None::<()>,
                message: Some("All Access grant revoked successfully".to_string()),
                error: None,
            }))
        },
        Ok(false) => Ok(HttpResponse::NotFound().json(ErrorResponse {
            success: false,
            error: "Access grant not found".to_string(),
            message: None,
        })),
        Err(e) => {
            error!("Error deleting access grant: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

/// Get effective permissions for current user on a specific inventory
#[get("/inventories/{id}/permissions")]
pub async fn get_inventory_permissions(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<i32>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let inventory_id = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service
        .get_effective_permissions(auth.user_id, inventory_id)
        .await
    {
        Ok(permissions) => Ok(HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(permissions),
            message: None,
            error: None,
        })),
        Err(e) => {
            error!("Error retrieving permissions: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }))
        },
    }
}

// ==================== Ownership Transfer ====================

/// Transfer ownership of an inventory to another user
/// This action is irreversible - the original owner loses all access
#[post("/inventories/{id}/transfer-ownership")]
pub async fn transfer_inventory_ownership(
    pool: web::Data<Pool>,
    req: HttpRequest,
    path: web::Path<i32>,
    body: web::Json<TransferOwnershipRequest>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let inventory_id = path.into_inner();
    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Get the inventory to verify ownership and get details
    let inventory = match db_service.get_inventory_by_id(inventory_id).await {
        Ok(Some(inv)) => inv,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "Inventory not found".to_string(),
                message: None,
            }));
        },
        Err(e) => {
            error!("Error retrieving inventory: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };

    // Only the owner can transfer ownership (not even All Access users)
    if inventory.user_id != Some(auth.user_id) {
        return Ok(HttpResponse::Forbidden().json(ErrorResponse {
            success: false,
            error: "Only the owner can transfer ownership of an inventory".to_string(),
            message: Some("You must be the owner to transfer this inventory".to_string()),
        }));
    }

    // Find the target user by username
    let target_user = match db_service
        .get_user_by_username(&body.new_owner_username)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: format!(
                    "User '{username}' not found",
                    username = body.new_owner_username
                ),
                message: None,
            }));
        },
        Err(e) => {
            error!("Error finding target user: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };

    // Cannot transfer to yourself
    if target_user.id == auth.user_id {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Cannot transfer ownership to yourself".to_string(),
            message: None,
        }));
    }

    // Check if target user is active
    if !target_user.is_active {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Cannot transfer ownership to an inactive user".to_string(),
            message: None,
        }));
    }

    // Get current user details for response
    let current_user = match db_service.get_user_by_id(auth.user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Current user not found".to_string(),
                message: None,
            }));
        },
        Err(e) => {
            error!("Error finding current user: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };

    // Perform the ownership transfer
    match db_service
        .transfer_inventory_ownership(inventory_id, auth.user_id, target_user.id)
        .await
    {
        Ok((items_transferred, shares_removed)) => {
            let target_full_name = target_user.full_name.clone();
            let target_username = target_user.username.clone();
            let inventory_name = inventory.name.clone();

            info!(
                "User {} transferred ownership of inventory '{}' (ID: {}) to user {}",
                auth.user_id, inventory_name, inventory_id, target_username
            );

            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(TransferOwnershipResponse {
                    inventory_id,
                    inventory_name: inventory.name,
                    previous_owner: UserResponse {
                        id: current_user.id,
                        username: current_user.username,
                        full_name: current_user.full_name,
                        is_admin: current_user.is_admin,
                        is_active: current_user.is_active,
                        created_at: current_user.created_at,
                        updated_at: current_user.updated_at,
                    },
                    new_owner: UserResponse {
                        id: target_user.id,
                        username: target_user.username,
                        full_name: target_user.full_name,
                        is_admin: target_user.is_admin,
                        is_active: target_user.is_active,
                        created_at: target_user.created_at,
                        updated_at: target_user.updated_at,
                    },
                    items_transferred,
                    shares_removed,
                }),
                message: Some(format!(
                    "Ownership transferred successfully to {target_full_name}. {items_transferred} items transferred, {shares_removed} shares removed."
                )),
                error: None,
            }))
        },
        Err(e) => {
            error!("Error transferring ownership: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: format!("Failed to transfer ownership: {e}"),
                message: None,
            }))
        },
    }
}

// ==================== Recovery Codes Endpoints ====================

/// Generate 10 new recovery codes for the authenticated user
/// Any existing codes are replaced. Requires the current password, because recovery
/// codes can reset the password and a stolen session token must not be enough.
#[post("/auth/recovery-codes/generate")]
pub async fn generate_recovery_codes(
    req: HttpRequest,
    pool: web::Data<Pool>,
    body: web::Json<GenerateRecoveryCodesRequest>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let throttle_subject = Subject::UserId(auth.user_id);
    let client_ip = client_ip_key(&req);
    if let Err(wait) = throttle::global().check(throttle_subject, &client_ip) {
        return Ok(throttled_response(wait));
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Re-authenticate before touching any existing codes
    let user = match db_service.get_user_by_id(auth.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ErrorResponse {
                success: false,
                error: "User not found".to_string(),
                message: None,
            }));
        },
        Err(e) => {
            error!("Error getting user for recovery code generation: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    };

    match verify_password(body.current_password.clone(), user.password_hash).await {
        Ok(true) => throttle::global().record_success(throttle_subject, &client_ip),
        Ok(false) => {
            throttle::global().record_failure(throttle_subject, &client_ip);
            // 400, not 401: the frontend treats any 401 as an expired session
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "Current password is incorrect".to_string(),
                message: None,
            }));
        },
        Err(e) => {
            if matches!(e, PasswordWorkError::Busy) {
                return Ok(crate::api::auth::busy_response());
            }
            error!("Error verifying password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An internal error occurred".to_string(),
                message: None,
            }));
        },
    }

    // Generate 10 random recovery codes
    let mut rng = rand::thread_rng();
    let mut plain_codes: Vec<String> = Vec::with_capacity(RECOVERY_CODE_COUNT);
    let mut code_hashes: Vec<String> = Vec::with_capacity(RECOVERY_CODE_COUNT);

    for _ in 0..RECOVERY_CODE_COUNT {
        // Generate code in format: XXXX-XXXX-XXXX (12 alphanumeric chars with dashes)
        let code: String = (&mut rng)
            .sample_iter(&Alphanumeric)
            .take(12)
            .map(char::from)
            .collect::<String>()
            .to_uppercase();

        // Format with dashes for readability
        let formatted_code = format!("{}-{}-{}", &code[0..4], &code[4..8], &code[8..12]);

        // Hash the code for storage
        let hash = match hash_password(formatted_code.clone()).await {
            Ok(h) => h,
            Err(e) => {
                if matches!(e, PasswordWorkError::Busy) {
                    return Ok(crate::api::auth::busy_response());
                }
                error!("Error hashing recovery code: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "Failed to generate recovery codes".to_string(),
                    message: None,
                }));
            },
        };

        plain_codes.push(formatted_code);
        code_hashes.push(hash);
    }

    // Store the hashed codes
    if let Err(e) = db_service
        .store_recovery_codes(auth.user_id, code_hashes)
        .await
    {
        error!("Error storing recovery codes: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "Failed to store recovery codes".to_string(),
            message: None,
        }));
    }

    info!("Generated 10 recovery codes for user {}", auth.user_id);

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(RecoveryCodesResponse {
            codes: plain_codes,
            generated_at: chrono::Utc::now(),
            message: "Save these codes in a safe place. Each code can only be used once. You won't be able to see these codes again!".to_string(),
        }),
        message: Some("Recovery codes generated successfully".to_string()),
        error: None,
    }))
}

/// Get the status of the user's recovery codes (not the codes themselves)
#[get("/auth/recovery-codes/status")]
pub async fn get_recovery_codes_status(
    req: HttpRequest,
    pool: web::Data<Pool>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    let db_service = DatabaseService::new(pool.get_ref().clone());

    match db_service.get_recovery_codes_status(auth.user_id).await {
        Ok((has_codes, confirmed, unused_count, generated_at)) => {
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(RecoveryCodesStatus {
                    has_codes,
                    codes_confirmed: confirmed,
                    unused_count,
                    generated_at,
                }),
                message: None,
                error: None,
            }))
        },
        Err(e) => {
            error!("Error getting recovery codes status: {}", e);
            Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to get recovery codes status".to_string(),
                message: None,
            }))
        },
    }
}

/// Confirm that the user has saved their recovery codes
#[post("/auth/recovery-codes/confirm")]
pub async fn confirm_recovery_codes(
    req: HttpRequest,
    pool: web::Data<Pool>,
    body: web::Json<ConfirmRecoveryCodesRequest>,
) -> Result<impl Responder> {
    let auth = match get_auth_context_from_request(&req, pool.get_ref()).await {
        Ok(a) => a,
        Err(e) => return Ok(e),
    };

    if !body.confirmed {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "You must confirm that you have saved the codes".to_string(),
            message: Some("Please save your recovery codes before confirming".to_string()),
        }));
    }

    let db_service = DatabaseService::new(pool.get_ref().clone());

    // Check if user has codes to confirm
    match db_service
        .get_unused_recovery_codes_count(auth.user_id)
        .await
    {
        Ok(0) => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                success: false,
                error: "No recovery codes to confirm".to_string(),
                message: Some("Please generate recovery codes first".to_string()),
            }));
        },
        Err(e) => {
            error!("Error checking recovery codes: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to check recovery codes".to_string(),
                message: None,
            }));
        },
        _ => {},
    }

    if let Err(e) = db_service.confirm_recovery_codes(auth.user_id).await {
        error!("Error confirming recovery codes: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            success: false,
            error: "Failed to confirm recovery codes".to_string(),
            message: None,
        }));
    }

    info!(
        "User {} confirmed saving their recovery codes",
        auth.user_id
    );

    Ok(HttpResponse::Ok().json(ApiResponse::<()> {
        success: true,
        data: None,
        message: Some(
            "Recovery codes confirmed. You can now use them to recover your account if needed."
                .to_string(),
        ),
        error: None,
    }))
}

/// Use a recovery code to reset password (no authentication required)
#[post("/auth/recovery-codes/use")]
pub async fn use_recovery_code(
    pool: web::Data<Pool>,
    http_req: HttpRequest,
    body: web::Json<UseRecoveryCodeRequest>,
) -> Result<impl Responder> {
    let db_service = DatabaseService::new(pool.get_ref().clone());
    let client_ip = client_ip_key(&http_req);

    if let Err(wait) = throttle::global().check(Subject::Username(&body.username), &client_ip) {
        return Ok(throttled_response(wait));
    }

    // Validate new password
    if let Err(msg) = validate_password(&body.new_password) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: msg.to_string(),
            message: Some("Invalid new password".to_string()),
        }));
    }

    let invalid = || {
        HttpResponse::BadRequest().json(ErrorResponse {
            success: false,
            error: "Invalid username or recovery code".to_string(),
            message: None,
        })
    };

    // Find user by username
    let user = match db_service.get_user_by_username(&body.username).await {
        Ok(user) => user.filter(|u| u.is_active),
        Err(e) => {
            error!("Error finding user: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "An error occurred".to_string(),
                message: None,
            }));
        },
    };

    // Get unused recovery codes (none for unknown or inactive users)
    let codes = match &user {
        Some(u) => match db_service.get_unused_recovery_codes(u.id).await {
            Ok(c) => c,
            Err(e) => {
                error!("Error getting recovery codes: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: "An error occurred".to_string(),
                    message: None,
                }));
            },
        },
        None => Vec::new(),
    };

    // Always do the work of RECOVERY_CODE_COUNT verifications and never stop at a match, so
    // response time reveals neither whether the account exists nor how many codes are left.
    let mut matched_code_id: Option<Uuid> = None;
    for (code_id, code_hash) in codes.iter().take(RECOVERY_CODE_COUNT) {
        match verify_password(body.recovery_code.clone(), code_hash.clone()).await {
            Ok(true) => matched_code_id = matched_code_id.or(Some(*code_id)),
            Err(PasswordWorkError::Busy) => return Ok(busy_response()),
            Ok(false) | Err(PasswordWorkError::Hash(_)) => {},
        }
    }
    for _ in codes.len().min(RECOVERY_CODE_COUNT)..RECOVERY_CODE_COUNT {
        if let Err(PasswordWorkError::Busy) = verify_against_dummy(body.recovery_code.clone()).await
        {
            return Ok(busy_response());
        }
    }

    let (Some(user), Some(code_id)) = (user, matched_code_id) else {
        throttle::global().record_failure(Subject::Username(&body.username), &client_ip);
        warn!("Invalid recovery code attempt");
        return Ok(invalid());
    };
    throttle::global().record_success(Subject::Username(&body.username), &client_ip);

    // Hash new password
    let new_password_hash = match hash_password(body.new_password.clone()).await {
        Ok(h) => h,
        Err(e) => {
            if matches!(e, PasswordWorkError::Busy) {
                return Ok(crate::api::auth::busy_response());
            }
            error!("Error hashing new password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to reset password".to_string(),
                message: None,
            }));
        },
    };

    // Update password
    match db_service
        .update_user_password(user.id, &new_password_hash)
        .await
    {
        Ok(Some(_)) => {},
        Ok(None) => return Ok(invalid()),
        Err(e) => {
            error!("Error updating password: {}", e);
            return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
                success: false,
                error: "Failed to reset password".to_string(),
                message: None,
            }));
        },
    }

    // Mark recovery code as used
    if let Err(e) = db_service.mark_recovery_code_used(code_id).await {
        error!("Error marking recovery code as used: {}", e);
        // Don't fail the request - password was already changed
    }

    // Get remaining codes count
    let remaining = db_service
        .get_unused_recovery_codes_count(user.id)
        .await
        .unwrap_or(0);

    info!(
        "User {} reset password using recovery code. {} codes remaining.",
        user.username, remaining
    );
    audit_sessions_revoked(user.id, "self_service", "recovery_code_password_reset");

    Ok(HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(RecoveryCodeUsedResponse {
            success: true,
            message: "Password reset successfully. You can now log in with your new password."
                .to_string(),
            remaining_codes: remaining,
        }),
        message: Some(format!(
            "Password reset successfully. You have {remaining} recovery codes remaining."
        )),
        error: None,
    }))
}
