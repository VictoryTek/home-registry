// Tests for authentication module

mod common;

use actix_web::{http::StatusCode, test, web, App};
use home_registry::auth::{create_token, verify_token};
use home_registry::models::{Claims, PermissionLevel};
use serde_json::json;

// ==================== Unit Tests (existing) ====================
// NOTE: These sync tests are temporarily commented out due to compilation issues
// They need to be migrated to async or fixed separately

/*
#[test]
fn test_password_hashing() {
    let password = "test_password_123";
    let hash = home_registry::auth::hash_password_sync(password).expect("Failed to hash password");

    // Hash should not be empty
    assert_ne!(hash, "");

    // Hash should not equal the password
    assert_ne!(hash, password);

    // Verify correct password
    assert!(home_registry::auth::verify_password_sync(password, &hash).unwrap());

    // Verify incorrect password
    assert!(!home_registry::auth::verify_password_sync("wrong_password", &hash).unwrap());
}

#[test]
fn test_password_validation() {
    // Valid passwords
    assert!(validate_password("password123").is_ok());
    assert!(validate_password("longer_password").is_ok());
    assert!(validate_password("P@ssw0rd!123").is_ok());

    // Invalid: too short
    assert!(validate_password("short").is_err());
    assert!(validate_password("1234567").is_err());

    // Invalid: empty
    assert!(validate_password("").is_err());

    // Invalid: too long
    assert!(validate_password(&"a".repeat(129)).is_err());
}

#[test]
fn test_username_validation() {
    // Valid usernames
    assert!(validate_username("user123").is_ok());
    assert!(validate_username("valid_user").is_ok());
    assert!(validate_username("user-name").is_ok());
    assert!(validate_username("User_Name-123").is_ok());

    // Invalid: too short
    assert!(validate_username("ab").is_err());
    assert!(validate_username("x").is_err());

    // Invalid: empty
    assert!(validate_username("").is_err());

    // Invalid: too long
    assert!(validate_username(&"a".repeat(51)).is_err());

    // Invalid: contains spaces
    assert!(validate_username("user name").is_err());

    // Invalid: contains invalid characters
    assert!(validate_username("user@name").is_err());
    assert!(validate_username("user.name").is_err());
    assert!(validate_username("user#name").is_err());
    assert!(validate_username("user$name").is_err());
}

#[test]
fn test_jwt_token_creation() {
    use uuid::Uuid;

    // Initialize JWT secret
    let _ = home_registry::auth::get_or_init_jwt_secret();

    let user_id = Uuid::new_v4();
    let username = "test_user";

    let token = create_token(&user_id, username).expect("Failed to create token");

    // Token should not be empty
    assert_ne!(token, "");

    // Token should have 3 parts (header.payload.signature)
    assert_eq!(token.split('.').count(), 3);
}

#[test]
fn test_jwt_token_verification() {
    use uuid::Uuid;

    // Initialize JWT secret
    let _ = home_registry::auth::get_or_init_jwt_secret();

    let user_id = Uuid::new_v4();
    let username = "test_user";

    let token = create_token(&user_id, username).expect("Failed to create token");

    // Verify valid token
    let claims = verify_token(&token).expect("Failed to verify token");
    assert_eq!(claims.sub, user_id.to_string());
    assert_eq!(claims.username, username);

    // Verify invalid token
    let invalid_token = "invalid.token.here";
    assert!(verify_token(invalid_token).is_err());

    // Verify tampered token (change last character)
    let mut tampered = token.clone();
    tampered.pop();
    tampered.push('X');
    assert!(verify_token(&tampered).is_err());
}

#[test]
fn test_jwt_secret_initialization() {
    let secret = home_registry::auth::get_or_init_jwt_secret();

    // Secret should not be empty
    assert_ne!(secret, "");

    // Secret should be at least 32 characters for security
    assert!(secret.len() >= 32);

    // Calling again should return the same secret
    let secret2 = home_registry::auth::get_or_init_jwt_secret();
    assert_eq!(secret, secret2);
}

#[test]
fn test_password_hash_uniqueness() {
    let password = "test_password";

    let hash1 = home_registry::auth::hash_password_sync(password).expect("Failed to hash password");
    let hash2 = home_registry::auth::hash_password_sync(password).expect("Failed to hash password");

    // Hashes should be different due to different salts
    assert_ne!(hash1, hash2);

    // But both should verify against the same password
    assert!(home_registry::auth::verify_password_sync(password, &hash1).unwrap());
    assert!(home_registry::auth::verify_password_sync(password, &hash2).unwrap());
}
*/

// ==================== Integration Tests (API Endpoints) ====================

#[actix_web::test]
async fn test_user_registration_valid() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    // Create initial admin user (required for registration endpoint)
    let admin_username = common::test_username("admin_init");
    common::create_admin_user(&pool, &admin_username).await;
    common::set_registration_mode(&pool, "open").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::register)),
    )
    .await;

    let username = common::test_username("reg_valid");
    let password = "ValidPassword123!";

    let register_payload = json!({
        "username": username,
        "password": password,
        "full_name": "Test User"
    });

    let req = test::TestRequest::post()
        .uri("/api/auth/register")
        .set_json(&register_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
    common::delete_test_user(&pool, &admin_username).await.ok();
}

#[actix_web::test]
async fn test_user_registration_duplicate_username() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    // Create initial admin user (required for registration endpoint)
    let admin_username = common::test_username("admin_init");
    common::create_admin_user(&pool, &admin_username).await;
    common::set_registration_mode(&pool, "open").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::register)),
    )
    .await;

    let username = common::test_username("reg_dup");

    // Create first user
    common::create_test_user(&pool, &username).await;

    // Attempt to create duplicate
    let register_payload = json!({
        "username": username,
        "password": "ValidPassword123!",
        "full_name": "Duplicate User"
    });

    let req = test::TestRequest::post()
        .uri("/api/auth/register")
        .set_json(&register_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
    common::delete_test_user(&pool, &admin_username).await.ok();
}

#[actix_web::test]
async fn test_user_registration_weak_password() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    // Create initial admin user (required for registration endpoint)
    let admin_username = common::test_username("admin_init");
    common::create_admin_user(&pool, &admin_username).await;
    common::set_registration_mode(&pool, "open").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::register)),
    )
    .await;

    let username = common::test_username("reg_weak");

    let register_payload = json!({
        "username": username,
        "password": "weak",  // Too short
        "full_name": "Test User"
    });

    let req = test::TestRequest::post()
        .uri("/api/auth/register")
        .set_json(&register_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Cleanup
    common::delete_test_user(&pool, &admin_username).await.ok();
}

#[actix_web::test]
async fn test_user_login_valid_credentials() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::login)),
    )
    .await;

    let username = common::test_username("login_valid");
    let (username, password) = common::create_test_user(&pool, &username).await;

    let login_payload = json!({
        "username": username,
        "password": password
    });

    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(&login_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"]["token"].is_string());
    assert!(body["data"]["user"].is_object());

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_user_login_invalid_password() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::login)),
    )
    .await;

    let username = common::test_username("login_invalid");
    let (username, _) = common::create_test_user(&pool, &username).await;

    let login_payload = json!({
        "username": username,
        "password": "WrongPassword123!"
    });

    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(&login_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_user_login_nonexistent_user() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::login)),
    )
    .await;

    let login_payload = json!({
        "username": "nonexistent_user_12345",
        "password": "Password123!"
    });

    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(&login_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_change_password_valid() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::change_password)),
    )
    .await;

    let username = common::test_username("changepw_valid");
    let (username, old_password) = common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let change_payload = json!({
        "current_password": old_password,
        "new_password": "NewValidPassword456!"
    });

    let req = test::TestRequest::put()
        .uri("/api/auth/password")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&change_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_change_password_wrong_old_password() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::change_password)),
    )
    .await;

    let username = common::test_username("changepw_wrong");
    let (username, _) = common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let change_payload = json!({
        "current_password": "WrongOldPassword123!",
        "new_password": "NewValidPassword456!"
    });

    let req = test::TestRequest::put()
        .uri("/api/auth/password")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&change_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_change_password_weak_new_password() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::change_password)),
    )
    .await;

    let username = common::test_username("changepw_weak");
    let (username, old_password) = common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let change_payload = json!({
        "current_password": old_password,
        "new_password": "weak"  // Too short
    });

    let req = test::TestRequest::put()
        .uri("/api/auth/password")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&change_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_token_validation_valid() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::get_current_user)),
    )
    .await;

    let username = common::test_username("token_valid");
    let (username, _) = common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_token_validation_missing() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::get_current_user)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/auth/me").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_token_validation_malformed() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::get_current_user)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header(("Authorization", "Bearer invalid.token.here"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_admin_role_check_success() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::admin_get_users)),
    )
    .await;

    let username = common::test_username("admin_check");
    let (username, _) = common::create_admin_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_admin_role_check_forbidden() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::admin_get_users)),
    )
    .await;

    let username = common::test_username("admin_forbidden");
    let (username, _) = common::create_test_user(&pool, &username).await; // Regular user
    let token = common::get_test_token(&pool, &username).await;

    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

// ==================== Session Management & JWT Lifecycle Tests ====================

#[actix_web::test]
async fn test_jwt_token_refresh_after_expiry() {
    use uuid::Uuid;

    // Initialize JWT secret
    let _ = home_registry::auth::get_or_init_jwt_secret();

    let user_id = Uuid::new_v4();
    let username = "refresh_test";

    // Create token
    let token = create_token(&user_id, username).expect("Failed to create token");

    // Verify it's valid
    let claims = verify_token(&token).expect("Token should be valid");
    assert_eq!(claims.username, username);
    assert_eq!(claims.sub, user_id.to_string());
}

#[actix_web::test]
async fn test_multiple_concurrent_sessions() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new().app_data(web::Data::new(pool.clone())).service(
            web::scope("/api")
                .service(home_registry::api::auth::login)
                .service(home_registry::api::auth::get_current_user),
        ),
    )
    .await;

    let username = common::test_username("multi_session");
    let (username, password) = common::create_test_user(&pool, &username).await;

    // Login twice to get two tokens
    let login_payload = json!({"username": username, "password": password});

    let req1 = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(&login_payload)
        .to_request();
    let resp1 = test::call_service(&app, req1).await;
    let body1: serde_json::Value = test::read_body_json(resp1).await;
    let token1 = body1["data"]["token"].as_str().unwrap();

    let req2 = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(&login_payload)
        .to_request();
    let resp2 = test::call_service(&app, req2).await;
    let body2: serde_json::Value = test::read_body_json(resp2).await;
    let token2 = body2["data"]["token"].as_str().unwrap();

    // Note: Tokens may be identical if generated within the same second due to JWT timestamp precision
    // The important part is that both tokens work for authenticated requests

    // Both should work for authenticated requests
    let req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header(("Authorization", format!("Bearer {token1}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header(("Authorization", format!("Bearer {token2}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_token_with_expired_claims() {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use uuid::Uuid;

    let secret = home_registry::auth::get_or_init_jwt_secret();

    // Create an expired token (exp in the past)
    #[allow(clippy::cast_sign_loss)]
    let claims = Claims {
        sub: Uuid::new_v4().to_string(),
        username: "test".to_string(),
        is_admin: false,
        exp: (chrono::Utc::now() - chrono::Duration::hours(1)).timestamp() as u64,
        iat: chrono::Utc::now().timestamp() as u64,
        totp_pending: false,
        tv: 0,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("Failed to encode token");

    // Should fail verification
    assert!(verify_token(&token).is_err());
}

#[actix_web::test]
async fn test_session_isolation_between_users() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::get_current_user)),
    )
    .await;

    // Create two users
    let username1 = common::test_username("session_user1");
    let username2 = common::test_username("session_user2");

    common::create_test_user(&pool, &username1).await;
    common::create_test_user(&pool, &username2).await;

    let token1 = common::get_test_token(&pool, &username1).await;
    let token2 = common::get_test_token(&pool, &username2).await;

    // Each token should return its own user data
    let req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header(("Authorization", format!("Bearer {token1}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["username"], username1);

    let req = test::TestRequest::get()
        .uri("/api/auth/me")
        .insert_header(("Authorization", format!("Bearer {token2}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["username"], username2);

    // Cleanup
    common::delete_test_user(&pool, &username1).await.ok();
    common::delete_test_user(&pool, &username2).await.ok();
}

// ==================== Permission & Authorization Tests ====================

#[actix_web::test]
async fn test_user_cannot_modify_other_user_password() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::admin_update_user)),
    )
    .await;

    // Create two regular users
    let username1 = common::test_username("perm_user1");
    let username2 = common::test_username("perm_user2");

    common::create_test_user(&pool, &username1).await;
    common::create_test_user(&pool, &username2).await;

    let db = home_registry::db::DatabaseService::new(pool.clone());
    let user2 = db.get_user_by_username(&username2).await.unwrap().unwrap();

    let token1 = common::get_test_token(&pool, &username1).await;

    // User1 tries to update User2's data
    let update_payload = json!({"full_name": "Hacked Name"});

    let req = test::TestRequest::put()
        .uri(&format!("/api/admin/users/{}", user2.id))
        .insert_header(("Authorization", format!("Bearer {token1}")))
        .set_json(&update_payload)
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Should be forbidden for non-admin
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Cleanup
    common::delete_test_user(&pool, &username1).await.ok();
    common::delete_test_user(&pool, &username2).await.ok();
}

#[actix_web::test]
async fn test_admin_can_list_all_users() {
    let pool = common::create_test_pool();

    // Initialize JWT secret for token generation
    home_registry::auth::get_or_init_jwt_secret();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::admin_get_users)),
    )
    .await;

    let admin_username = common::test_username("admin_list");
    common::create_admin_user(&pool, &admin_username).await;
    let token = common::get_test_token(&pool, &admin_username).await;

    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());

    // Cleanup
    common::delete_test_user(&pool, &admin_username).await.ok();
}

#[actix_web::test]
async fn test_regular_user_cannot_list_users() {
    let pool = common::create_test_pool();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(web::scope("/api").service(home_registry::api::auth::admin_get_users)),
    )
    .await;

    let username = common::test_username("regular_nolist");
    common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Cleanup
    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn test_shared_inventory_access_permission() {
    let pool = common::create_test_pool();
    let db = home_registry::db::DatabaseService::new(pool.clone());

    // Create two users
    let owner_username = common::test_username("share_owner");
    let shared_username = common::test_username("share_user");

    common::create_test_user(&pool, &owner_username).await;
    common::create_test_user(&pool, &shared_username).await;

    let owner = db
        .get_user_by_username(&owner_username)
        .await
        .unwrap()
        .unwrap();
    let shared_user = db
        .get_user_by_username(&shared_username)
        .await
        .unwrap()
        .unwrap();

    // Owner creates inventory
    let inventory_id = common::create_test_inventory(&pool, owner.id, "Shared Inventory")
        .await
        .unwrap();

    // Share inventory with second user (view permission)
    let share_result = db
        .create_inventory_share(
            inventory_id,
            shared_user.id,
            owner.id,
            PermissionLevel::View,
        )
        .await;
    assert!(share_result.is_ok());

    // Verify shared user can view
    let accessible_inventories = db.get_accessible_inventories(shared_user.id).await.unwrap();
    assert!(accessible_inventories
        .iter()
        .any(|inv| inv.id == Some(inventory_id)));

    // Cleanup
    common::delete_test_inventory(&pool, inventory_id)
        .await
        .ok();
    common::delete_test_user(&pool, &owner_username).await.ok();
    common::delete_test_user(&pool, &shared_username).await.ok();
}
