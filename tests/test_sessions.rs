// Session revocation: token_version, auth context from the stored user, fresh tokens.
// Requires the Postgres test database (see tests/common).

mod common;

use actix_web::{http::StatusCode, test, web, App};
use home_registry::api;
use home_registry::db::DatabaseService;
use serde_json::{json, Value};

const PASSWORD: &str = "TestPassword123!";

/// The API surface these tests exercise.
macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new().app_data(web::Data::new($pool.clone())).service(
                web::scope("/api")
                    .service(api::auth::get_current_user)
                    .service(api::auth::change_password)
                    .service(api::auth::logout_all_devices)
                    .service(api::auth::admin_get_users)
                    .service(api::auth::admin_update_user)
                    .service(api::auth::login)
                    .service(api::totp::totp_setup)
                    .service(api::totp::totp_verify_setup)
                    .service(api::totp::totp_verify)
                    .service(api::totp::totp_disable),
            ),
        )
        .await
    };
}

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

async fn token_version(db: &DatabaseService, username: &str) -> i32 {
    db.get_user_by_username(username)
        .await
        .unwrap()
        .unwrap()
        .token_version
}

/// GET /api/auth/me with `$token`; yields (status, parsed JSON body or Null).
macro_rules! me_status {
    ($app:expr, $token:expr) => {{
        let req = test::TestRequest::get()
            .uri("/api/auth/me")
            .insert_header(bearer($token))
            .to_request();
        let resp = test::call_service($app, req).await;
        let status = resp.status();
        let body = test::read_body(resp).await;
        (
            status,
            serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null),
        )
    }};
}

fn fresh_token(body: &Value) -> String {
    body["data"]["token"]
        .as_str()
        .expect("response carries a fresh token")
        .to_string()
}

#[actix_web::test]
async fn password_change_revokes_other_devices_but_keeps_this_one() {
    let pool = common::create_test_pool();
    let app = app!(pool);
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("sess_pw");
    common::create_test_user(&pool, &username).await;
    let device_a = common::get_test_token(&pool, &username).await;
    let device_b = common::get_test_token(&pool, &username).await;

    assert_eq!(me_status!(&app, &device_a).0, StatusCode::OK);
    assert_eq!(me_status!(&app, &device_b).0, StatusCode::OK);

    let req = test::TestRequest::put()
        .uri("/api/auth/password")
        .insert_header(bearer(&device_a))
        .set_json(json!({ "current_password": PASSWORD, "new_password": "BrandNewPass123!" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    let device_a_fresh = fresh_token(&body);
    assert_eq!(token_version(&db, &username).await, 1);

    // The second device is signed out, with the code the login page keys its notice off
    let (status, body) = me_status!(&app, &device_b);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "session_revoked");
    // So is the pre-change token of the device that made the change; its fresh one works
    assert_eq!(me_status!(&app, &device_a).0, StatusCode::UNAUTHORIZED);
    assert_eq!(me_status!(&app, &device_a_fresh).0, StatusCode::OK);

    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn logout_all_signs_out_other_devices_and_returns_a_working_token() {
    let pool = common::create_test_pool();
    let app = app!(pool);

    let username = common::test_username("sess_all");
    common::create_test_user(&pool, &username).await;
    let this_device = common::get_test_token(&pool, &username).await;
    let other_device = common::get_test_token(&pool, &username).await;

    let req = test::TestRequest::post()
        .uri("/api/auth/logout-all")
        .insert_header(bearer(&this_device))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let fresh = fresh_token(&test::read_body_json(resp).await);

    assert_eq!(me_status!(&app, &other_device).0, StatusCode::UNAUTHORIZED);
    assert_eq!(me_status!(&app, &this_device).0, StatusCode::UNAUTHORIZED);
    assert_eq!(me_status!(&app, &fresh).0, StatusCode::OK);

    // Unauthenticated callers get nothing
    let req = test::TestRequest::post()
        .uri("/api/auth/logout-all")
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );

    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn admin_flag_comes_from_the_database_not_the_token() {
    let pool = common::create_test_pool();
    let app = app!(pool);
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("sess_adm");
    common::create_admin_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;

    let list = |token: &str| {
        test::TestRequest::get()
            .uri("/api/admin/users")
            .insert_header(bearer(token))
            .to_request()
    };
    assert_eq!(
        test::call_service(&app, list(&token)).await.status(),
        StatusCode::OK
    );

    // Demote straight in the database (no version bump): the token still says is_admin = true
    // and still matches the session generation, but admin access is gone at once.
    let client = pool.get().await.unwrap();
    client
        .execute(
            "UPDATE users SET is_admin = false WHERE username = $1",
            &[&username],
        )
        .await
        .unwrap();
    assert_eq!(token_version(&db, &username).await, 0);
    assert_eq!(
        test::call_service(&app, list(&token)).await.status(),
        StatusCode::FORBIDDEN
    );

    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn admin_updates_bump_only_on_real_changes_and_revoke_the_target() {
    let pool = common::create_test_pool();
    let app = app!(pool);
    let db = DatabaseService::new(pool.clone());

    let admin_name = common::test_username("sess_boss");
    let target_name = common::test_username("sess_tgt");
    common::create_admin_user(&pool, &admin_name).await;
    common::create_test_user(&pool, &target_name).await;
    let admin_token = common::get_test_token(&pool, &admin_name).await;
    let target_token = common::get_test_token(&pool, &target_name).await;
    let target = db
        .get_user_by_username(&target_name)
        .await
        .unwrap()
        .unwrap();

    let update = |body: Value| {
        test::TestRequest::put()
            .uri(&format!("/api/admin/users/{}", target.id))
            .insert_header(bearer(&admin_token))
            .set_json(body)
            .to_request()
    };

    // A save that changes nothing (same flags, name only) signs nobody out
    for body in [
        json!({ "is_admin": false, "is_active": true }),
        json!({ "full_name": "Renamed Target" }),
        json!({}),
    ] {
        let resp = test::call_service(&app, update(body)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(token_version(&db, &target_name).await, 0);
        assert_eq!(me_status!(&app, &target_token).0, StatusCode::OK);
    }

    // Deactivating takes effect immediately and revokes the sessions
    let resp = test::call_service(&app, update(json!({ "is_active": false }))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(token_version(&db, &target_name).await, 1);
    assert_eq!(me_status!(&app, &target_token).0, StatusCode::FORBIDDEN);

    // Reactivating does not resurrect the old token
    let resp = test::call_service(&app, update(json!({ "is_active": true }))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(token_version(&db, &target_name).await, 2);
    let (status, body) = me_status!(&app, &target_token);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "session_revoked");

    // Promotion is a role change too; an admin reset of the password revokes as well
    let before = token_version(&db, &target_name).await;
    let resp = test::call_service(&app, update(json!({ "is_admin": true }))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(token_version(&db, &target_name).await, before + 1);
    let resp = test::call_service(&app, update(json!({ "password": "ResetByAdmin123!" }))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(token_version(&db, &target_name).await, before + 2);

    // The admin's own session is untouched by editing someone else
    assert_eq!(me_status!(&app, &admin_token).0, StatusCode::OK);

    common::delete_test_user(&pool, &admin_name).await.ok();
    common::delete_test_user(&pool, &target_name).await.ok();
}

#[actix_web::test]
async fn admins_cannot_demote_or_deactivate_themselves() {
    let pool = common::create_test_pool();
    let app = app!(pool);
    let db = DatabaseService::new(pool.clone());

    // (The last-active-admin guard for *other* admins cannot be exercised here: the shared
    // test database may hold other active admins.)
    let admin_name = common::test_username("sess_last");
    common::create_admin_user(&pool, &admin_name).await;
    let token = common::get_test_token(&pool, &admin_name).await;
    let admin = db.get_user_by_username(&admin_name).await.unwrap().unwrap();

    // Self-demotion and self-deactivation are refused outright, whatever the admin count
    for body in [json!({ "is_admin": false }), json!({ "is_active": false })] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/admin/users/{}", admin.id))
            .insert_header(bearer(&token))
            .set_json(body)
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(token_version(&db, &admin_name).await, 0);

    common::delete_test_user(&pool, &admin_name).await.ok();
}

#[actix_web::test]
async fn tokens_without_a_tv_claim_count_as_generation_zero() {
    use jsonwebtoken::{encode, EncodingKey, Header};

    #[derive(serde::Serialize)]
    struct LegacyClaims {
        sub: String,
        username: String,
        is_admin: bool,
        exp: u64,
        iat: u64,
        totp_pending: bool,
    }

    let pool = common::create_test_pool();
    let app = app!(pool);
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("sess_legacy");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let secret = home_registry::auth::get_or_init_jwt_secret();
    #[allow(clippy::cast_sign_loss)]
    let now = chrono::Utc::now().timestamp() as u64;
    let legacy = encode(
        &Header::default(),
        &LegacyClaims {
            sub: user.id.to_string(),
            username: username.clone(),
            is_admin: false,
            exp: now + 3600,
            iat: now,
            totp_pending: false,
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap();

    // Accepted while the user is still at generation 0 ...
    assert_eq!(me_status!(&app, &legacy).0, StatusCode::OK);
    // ... rejected as soon as the sessions are revoked
    db.bump_token_version(user.id).await.unwrap();
    assert_eq!(me_status!(&app, &legacy).0, StatusCode::UNAUTHORIZED);

    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn a_pending_totp_login_dies_when_sessions_are_revoked() {
    use home_registry::auth::generate_partial_token;

    let pool = common::create_test_pool();
    let app = app!(pool);
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("sess_part");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();
    let setup = home_registry::auth::totp::generate_totp_setup(&username).unwrap();
    db.create_totp_settings(user.id, &setup.encrypted_secret)
        .await
        .unwrap();
    db.enable_totp(user.id, "both").await.unwrap();

    let partial = generate_partial_token(&user).unwrap();
    let verify = |token: &str| {
        test::TestRequest::post()
            .uri("/api/auth/totp/verify")
            .insert_header(bearer(token))
            .set_json(json!({ "code": "000000" }))
            .to_request()
    };

    // Before the bump the request gets as far as checking the (wrong) code
    let resp = test::call_service(&app, verify(&partial)).await;
    let body: Value = test::read_body_json(resp).await;
    assert_ne!(body["error"], "session_revoked");

    db.bump_token_version(user.id).await.unwrap();
    let resp = test::call_service(&app, verify(&partial)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "session_revoked");

    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn enabling_and_disabling_totp_revoke_other_sessions_and_return_a_fresh_token() {
    use totp_rs::{Algorithm, Secret, TOTP};

    let pool = common::create_test_pool();
    let app = app!(pool);
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("sess_totp");
    common::create_test_user(&pool, &username).await;
    let this_device = common::get_test_token(&pool, &username).await;
    let other_device = common::get_test_token(&pool, &username).await;

    // Start the setup and compute the current code from the returned secret
    let req = test::TestRequest::post()
        .uri("/api/auth/totp/setup")
        .insert_header(bearer(&this_device))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    let secret = body["data"]["secret"].as_str().unwrap().to_string();
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        Secret::Encoded(secret).to_bytes().unwrap(),
        None,
        username.clone(),
    )
    .unwrap();
    let code = totp.generate_current().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/auth/totp/verify-setup")
        .insert_header(bearer(&this_device))
        .set_json(json!({ "code": code, "mode": "both" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let enabled_token = fresh_token(&test::read_body_json(resp).await);
    assert_eq!(token_version(&db, &username).await, 1);
    assert_eq!(me_status!(&app, &other_device).0, StatusCode::UNAUTHORIZED);
    assert_eq!(me_status!(&app, &this_device).0, StatusCode::UNAUTHORIZED);
    assert_eq!(me_status!(&app, &enabled_token).0, StatusCode::OK);

    // Disabling: other sessions go again, and a fresh token comes back
    let second_device = common::get_test_token(&pool, &username).await;
    assert_eq!(me_status!(&app, &second_device).0, StatusCode::OK);
    let req = test::TestRequest::delete()
        .uri("/api/auth/totp")
        .insert_header(bearer(&enabled_token))
        .set_json(json!({ "password": PASSWORD }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let disabled_token = fresh_token(&test::read_body_json(resp).await);
    assert_eq!(token_version(&db, &username).await, 2);
    assert_eq!(me_status!(&app, &enabled_token).0, StatusCode::UNAUTHORIZED);
    assert_eq!(me_status!(&app, &second_device).0, StatusCode::UNAUTHORIZED);
    assert_eq!(me_status!(&app, &disabled_token).0, StatusCode::OK);

    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn concurrent_bumps_both_apply() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("sess_race");
    common::create_test_user(&pool, &username).await;
    let user = db.get_user_by_username(&username).await.unwrap().unwrap();

    let (a, b) = tokio::join!(
        db.bump_token_version(user.id),
        db.bump_token_version(user.id)
    );
    let mut versions = vec![a.unwrap().unwrap(), b.unwrap().unwrap()];
    versions.sort_unstable();
    // Each caller saw its own result of the increment; none was lost
    assert_eq!(versions, vec![1, 2]);
    assert_eq!(token_version(&db, &username).await, 2);

    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn migration_is_idempotent_and_existing_users_start_at_zero() {
    let pool = common::create_test_pool();
    let db = DatabaseService::new(pool.clone());

    let username = common::test_username("sess_mig");
    common::create_test_user(&pool, &username).await;

    let sql = include_str!("../migrations/V024__add_user_token_version.sql");
    let client = pool.get().await.unwrap();
    client.batch_execute(sql).await.unwrap();
    client.batch_execute(sql).await.unwrap();

    assert_eq!(token_version(&db, &username).await, 0);
    let rows = client
        .query(
            "SELECT data_type, is_nullable, column_default FROM information_schema.columns
             WHERE table_name = 'users' AND column_name = 'token_version'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, String>(0), "integer");
    assert_eq!(rows[0].get::<_, String>(1), "NO");
    assert_eq!(rows[0].get::<_, String>(2), "0");

    common::delete_test_user(&pool, &username).await.ok();
}

#[actix_web::test]
async fn password_change_stays_behind_the_failed_attempt_throttle() {
    let pool = common::create_test_pool();
    let app = app!(pool);

    let username = common::test_username("sess_thr");
    common::create_test_user(&pool, &username).await;
    let token = common::get_test_token(&pool, &username).await;
    let peer = "10.77.0.1:4000".parse().unwrap();

    let change = |current: &str| {
        test::TestRequest::put()
            .uri("/api/auth/password")
            .peer_addr(peer)
            .insert_header(bearer(&token))
            .set_json(json!({ "current_password": current, "new_password": "BrandNewPass123!" }))
            .to_request()
    };

    for _ in 0..5 {
        let resp = test::call_service(&app, change("WrongPassword1!")).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
    // Locked: even the right current password is refused, and no fresh token is issued
    let resp = test::call_service(&app, change(PASSWORD)).await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["token"].is_null());

    common::delete_test_user(&pool, &username).await.ok();
}
