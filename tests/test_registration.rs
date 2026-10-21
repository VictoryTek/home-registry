// Registration mode, invitation links and password-reset links.
//
// Every test works in its own scratch database (tests/common::ScratchDb): the registration mode is
// one setting for the whole server, and restore truncates every table.

mod common;

use actix_web::{http::StatusCode, test, web, App};
use common::ScratchDb;
use home_registry::api;
use home_registry::api::backup::{create_backup_file, restore_backup_file};
use home_registry::db::DatabaseService;
use home_registry::models::RestoreRequest;
use serde_json::{json, Value};
use std::sync::{Mutex, Once};
use uuid::Uuid;

const PASSWORD: &str = "ValidPassword123!";
const NEW_PASSWORD: &str = "BrandNewPassword456!";

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new().app_data(web::Data::new($pool.clone())).service(
                web::scope("/api")
                    .service(api::auth::register)
                    .service(api::auth::check_invite)
                    .service(api::auth::reset_password_with_token)
                    .service(api::auth::login)
                    .service(api::auth::get_current_user)
                    .service(api::auth::get_setup_status)
                    .service(api::auth::admin_get_registration_settings)
                    .service(api::auth::admin_update_registration_settings)
                    .service(api::auth::admin_create_invite)
                    .service(api::auth::admin_list_invites)
                    .service(api::auth::admin_revoke_invite)
                    .service(api::auth::admin_issue_reset_link)
                    .service(api::auth::admin_create_user)
                    .service(api::auth::admin_delete_user)
                    .service(api::auth::admin_get_users),
            ),
        )
        .await
    };
}

/// Send a request; yields (status, parsed JSON body or Null).
macro_rules! send {
    ($app:expr, $req:expr) => {{
        let resp = test::call_service($app, $req.to_request()).await;
        let status = resp.status();
        let body = test::read_body(resp).await;
        (
            status,
            serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null),
        )
    }};
}

fn with_auth(req: test::TestRequest, bearer: Option<&str>) -> test::TestRequest {
    match bearer {
        Some(token) => req.insert_header(("Authorization", format!("Bearer {token}"))),
        None => req,
    }
}

fn get(uri: &str, bearer: Option<&str>) -> test::TestRequest {
    with_auth(test::TestRequest::get().uri(uri), bearer)
}
fn post(uri: &str, bearer: Option<&str>, body: Value) -> test::TestRequest {
    with_auth(test::TestRequest::post().uri(uri).set_json(body), bearer)
}
fn put(uri: &str, bearer: Option<&str>, body: Value) -> test::TestRequest {
    with_auth(test::TestRequest::put().uri(uri).set_json(body), bearer)
}
fn delete(uri: &str, bearer: Option<&str>) -> test::TestRequest {
    with_auth(test::TestRequest::delete().uri(uri), bearer)
}

/// A scratch database with one admin; yields (scratch, admin JWT, admin id).
async fn scratch_with_admin() -> (ScratchDb, String, Uuid) {
    home_registry::auth::get_or_init_jwt_secret();
    let scratch = ScratchDb::create().await;
    let name = common::test_username("admin");
    common::create_admin_user(&scratch.pool, &name).await;
    let jwt = common::get_test_token(&scratch.pool, &name).await;
    let id = user_id(&scratch.pool, &name).await;
    (scratch, jwt, id)
}

async fn user_id(pool: &deadpool_postgres::Pool, username: &str) -> Uuid {
    pool.get()
        .await
        .unwrap()
        .query_one("SELECT id FROM users WHERE username = $1", &[&username])
        .await
        .unwrap()
        .get(0)
}

async fn count(
    pool: &deadpool_postgres::Pool,
    sql: &str,
    param: &(dyn tokio_postgres::types::ToSql + Sync),
) -> i64 {
    pool.get()
        .await
        .unwrap()
        .query_one(sql, &[param])
        .await
        .unwrap()
        .get(0)
}

async fn set_mode(pool: &deadpool_postgres::Pool, mode: &str) {
    common::set_registration_mode(pool, mode).await;
}

macro_rules! new_invite {
    ($app:expr, $admin:expr) => {{
        let (status, body) = send!($app, post("/api/admin/invites", Some($admin), json!({})));
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"]["token"]
            .as_str()
            .expect("token in response")
            .to_string()
    }};
}

/// Like `new_invite!`, also yielding the invitation's id (for revoking it).
macro_rules! new_invite_with_id {
    ($app:expr, $admin:expr) => {{
        let (status, body) = send!($app, post("/api/admin/invites", Some($admin), json!({})));
        assert_eq!(status, StatusCode::CREATED, "{body}");
        (
            body["data"]["token"]
                .as_str()
                .expect("token in response")
                .to_string(),
            body["data"]["id"]
                .as_str()
                .expect("id in response")
                .to_string(),
        )
    }};
}

macro_rules! register {
    ($app:expr, $name:expr, $password:expr, $invite:expr) => {{
        let mut payload = json!({"username": $name, "password": $password, "full_name": "New Person"});
        if let Some(token) = $invite {
            payload["invite_token"] = json!(token);
        }
        send!($app, post("/api/auth/register", None, payload))
    }};
}

macro_rules! check {
    ($app:expr, $token:expr) => {{
        send!($app, post("/api/auth/invite/check", None, json!({ "token": $token })))
    }};
}

macro_rules! reset_link {
    ($app:expr, $admin:expr, $user:expr) => {{
        let (status, body) = send!(
            $app,
            post(
                &format!("/api/admin/users/{}/reset-link", $user),
                Some($admin),
                json!({})
            )
        );
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"]["token"]
            .as_str()
            .expect("token in response")
            .to_string()
    }};
}

macro_rules! reset {
    ($app:expr, $token:expr, $password:expr) => {{
        send!(
            $app,
            post(
                "/api/auth/reset-password",
                None,
                json!({ "token": $token, "new_password": $password })
            )
        )
    }};
}

// ---------------------------------------------------------------- captured logs

static LOGS: Mutex<Vec<(log::Level, String, String)>> = Mutex::new(Vec::new());

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        LOGS.lock().unwrap().push((
            record.level(),
            record.target().to_string(),
            record.args().to_string(),
        ));
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

fn logged() -> Vec<(log::Level, String, String)> {
    LOGS.lock().unwrap().clone()
}

// ---------------------------------------------------------------- registration mode

#[actix_web::test]
async fn a_fresh_install_is_invite_only() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);

    let (status, body) = send!(&app, get("/api/auth/setup/status", None));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["registration_mode"], "invite");

    let (status, body) = send!(&app, get("/api/admin/settings/registration", Some(&admin)));
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["registration_mode"], "invite");
    assert_eq!(body["data"]["upgrade_notice"], false);

    let (status, body) = register!(&app, "stranger", PASSWORD, None::<&str>);
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "Registration is by invitation only");
    assert_eq!(
        count(
            &scratch.pool,
            "SELECT COUNT(*) FROM users WHERE username = $1",
            &"stranger"
        )
        .await,
        0
    );
}

#[actix_web::test]
async fn open_mode_registers_without_an_invitation() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);

    let (status, body) = send!(
        &app,
        put(
            "/api/admin/settings/registration",
            Some(&admin),
            json!({"registration_mode": "open"})
        )
    );
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["registration_mode"], "open");

    let (status, _) = register!(&app, "walk_in", PASSWORD, None::<&str>);
    assert_eq!(status, StatusCode::CREATED);
    // A token that is sent anyway does not matter in open mode
    let (status, _) = register!(&app, "walk_in_two", PASSWORD, Some("not-a-real-token"));
    assert_eq!(status, StatusCode::CREATED);
}

#[actix_web::test]
async fn an_unknown_mode_is_refused() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let (status, body) = send!(
        &app,
        put(
            "/api/admin/settings/registration",
            Some(&admin),
            json!({"registration_mode": "anyone"})
        )
    );
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (_, body) = send!(&app, get("/api/admin/settings/registration", Some(&admin)));
    assert_eq!(body["data"]["registration_mode"], "invite");
}

#[actix_web::test]
async fn closed_mode_stops_invitations_but_not_admins_and_keeps_existing_invites() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);

    let earlier = new_invite!(&app, &admin);
    set_mode(&scratch.pool, "closed").await;

    // Redemption is refused, creation is refused, the check says invalid
    let (status, body) = register!(&app, "latecomer", PASSWORD, Some(earlier.as_str()));
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "Registration is closed");
    let (_, body) = check!(&app, earlier);
    assert_eq!(body["data"]["valid"], false);
    let (status, _) = send!(&app, post("/api/admin/invites", Some(&admin), json!({})));
    assert_eq!(status, StatusCode::CONFLICT);

    // Admins can still create users directly
    let (status, body) = send!(
        &app,
        post(
            "/api/admin/users",
            Some(&admin),
            json!({"username": "made_by_admin", "full_name": "Made By Admin", "password": PASSWORD,
                   "is_admin": false, "is_active": true})
        )
    );
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // The invitation was not deleted: it works again once the mode returns to invite
    assert_eq!(
        count(
            &scratch.pool,
            "SELECT COUNT(*) FROM auth_tokens WHERE purpose = 'invite' AND $1::int = 1",
            &1_i32
        )
        .await,
        1
    );
    set_mode(&scratch.pool, "invite").await;
    let (_, body) = check!(&app, earlier);
    assert_eq!(body["data"]["valid"], true);
    let (status, body) = register!(&app, "latecomer", PASSWORD, Some(earlier.as_str()));
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

// ---------------------------------------------------------------- invitations

#[actix_web::test]
async fn an_invitation_makes_an_ordinary_active_user_exactly_once() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let invite = new_invite!(&app, &admin);

    // Nothing in the request can ask for more than an ordinary account
    let (status, body) = send!(
        &app,
        post(
            "/api/auth/register",
            None,
            json!({"username": "invited_one", "password": PASSWORD, "full_name": "Invited One",
                   "invite_token": invite, "is_admin": true, "is_active": false, "role": "admin"})
        )
    );
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let row = scratch
        .pool
        .get()
        .await
        .unwrap()
        .query_one(
            "SELECT is_admin, is_active FROM users WHERE username = 'invited_one'",
            &[],
        )
        .await
        .unwrap();
    assert!(
        !row.get::<_, bool>(0),
        "an invitation must not create an admin"
    );
    assert!(row.get::<_, bool>(1));

    // The account works straight away
    let token = body["data"]["token"].as_str().unwrap();
    let (status, _) = send!(&app, get("/api/auth/me", Some(token)));
    assert_eq!(status, StatusCode::OK);

    // And the invitation is spent
    let (status, _) = register!(&app, "invited_two", PASSWORD, Some(invite.as_str()));
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, body) = check!(&app, invite);
    assert_eq!(body["data"]["valid"], false);
    assert_eq!(
        count(
            &scratch.pool,
            "SELECT COUNT(*) FROM users WHERE username = $1",
            &"invited_two"
        )
        .await,
        0
    );
}

#[actix_web::test]
async fn a_failed_registration_does_not_use_up_the_invitation() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let invite = new_invite!(&app, &admin);

    let (status, _) = register!(&app, "careful", "short", Some(invite.as_str()));
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = register!(&app, "x", PASSWORD, Some(invite.as_str()));
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = register!(&app, "taken_name", PASSWORD, Some(invite.as_str()));
    assert_eq!(status, StatusCode::CREATED);
    let second = new_invite!(&app, &admin);
    let (status, _) = register!(&app, "taken_name", PASSWORD, Some(second.as_str()));
    assert_eq!(status, StatusCode::CONFLICT);

    // The invitation that met a taken name is still good for another name
    let (_, body) = check!(&app, second);
    assert_eq!(body["data"]["valid"], true);
    let (status, _) = register!(&app, "other_name", PASSWORD, Some(second.as_str()));
    assert_eq!(status, StatusCode::CREATED);
}

#[actix_web::test]
async fn two_simultaneous_registrations_with_one_invitation_create_one_account() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let invite = new_invite!(&app, &admin);

    let before = count(
        &scratch.pool,
        "SELECT COUNT(*) FROM users WHERE $1::int = 1",
        &1_i32,
    )
    .await;
    let (a, b) = tokio::join!(
        async { register!(&app, "racer_a", PASSWORD, Some(invite.as_str())) },
        async { register!(&app, "racer_b", PASSWORD, Some(invite.as_str())) },
    );
    let mut statuses = [a.0.as_u16(), b.0.as_u16()];
    statuses.sort_unstable();
    assert_eq!(statuses, [201, 403], "{a:?} {b:?}");
    let after = count(
        &scratch.pool,
        "SELECT COUNT(*) FROM users WHERE $1::int = 1",
        &1_i32,
    )
    .await;
    assert_eq!(after, before + 1);
}

#[actix_web::test]
async fn every_invalid_invitation_looks_the_same() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let pool = &scratch.pool;

    // Valid format, never issued
    let unknown = "A".repeat(43);

    // Used
    let used = new_invite!(&app, &admin);
    assert_eq!(
        register!(&app, "uses_it", PASSWORD, Some(used.as_str())).0,
        StatusCode::CREATED
    );

    // Expired
    let expired = new_invite!(&app, &admin);
    pool.get()
        .await
        .unwrap()
        .execute(
            "UPDATE auth_tokens SET created_at = NOW() - INTERVAL '3 days',
                                    expires_at = NOW() - INTERVAL '1 minute'
             WHERE purpose = 'invite' AND used_at IS NULL AND id = (
                 SELECT id FROM auth_tokens WHERE purpose = 'invite' AND used_at IS NULL
                 ORDER BY created_at DESC LIMIT 1)",
            &[],
        )
        .await
        .unwrap();

    // Revoked
    let (_, listed) = send!(&app, get("/api/admin/invites", Some(&admin)));
    assert_eq!(
        listed["data"].as_array().unwrap().len(),
        0,
        "expired and used ones are not listed"
    );
    let (revoked, revoked_id) = new_invite_with_id!(&app, &admin);
    let (status, _) = send!(
        &app,
        delete(&format!("/api/admin/invites/{revoked_id}"), Some(&admin))
    );
    assert_eq!(status, StatusCode::OK);

    // A password-reset link is not an invitation
    let someone = common::test_username("someone");
    common::create_test_user(pool, &someone).await;
    let someone_id = user_id(pool, &someone).await;
    let wrong_purpose = reset_link!(&app, &admin, someone_id);

    let invalid: Vec<(&str, String)> = vec![
        ("unknown", unknown),
        ("used", used),
        ("expired", expired),
        ("revoked", revoked),
        ("wrong purpose", wrong_purpose),
        ("malformed", "not a token".to_string()),
        ("empty", String::new()),
        ("too long", "A".repeat(500)),
    ];

    let reference = check!(&app, invalid[0].1);
    assert_eq!(reference.0, StatusCode::OK);
    assert_eq!(reference.1["data"]["valid"], false);
    let reference_register = register!(&app, "probe", PASSWORD, Some(invalid[0].1.as_str()));
    assert_eq!(reference_register.0, StatusCode::FORBIDDEN);

    for (label, token) in &invalid {
        let seen = check!(&app, token);
        assert_eq!(seen, reference, "check differs for {label}");
        let seen = register!(&app, "probe", PASSWORD, Some(token.as_str()));
        assert_eq!(seen, reference_register, "register differs for {label}");
    }
    // None of them created the probe account
    assert_eq!(
        count(
            pool,
            "SELECT COUNT(*) FROM users WHERE username = $1",
            &"probe"
        )
        .await,
        0
    );

    // A good one says so
    let good = new_invite!(&app, &admin);
    let (status, body) = check!(&app, good);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["valid"], true);
}

#[actix_web::test]
async fn listing_and_revoking_invitations() {
    let (scratch, admin, admin_id) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let keep = new_invite!(&app, &admin);
    let _drop = new_invite!(&app, &admin);

    let (status, body) = send!(&app, get("/api/admin/invites", Some(&admin)));
    assert_eq!(status, StatusCode::OK);
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0]["expires_at"].is_string());
    assert!(rows[0]["created_by_username"].is_string());
    // A listing never carries the secret
    assert!(!body.to_string().contains(&keep));

    let id = rows[0]["id"].as_str().unwrap().to_string();
    let (status, _) = send!(
        &app,
        delete(&format!("/api/admin/invites/{id}"), Some(&admin))
    );
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send!(
        &app,
        delete(&format!("/api/admin/invites/{id}"), Some(&admin))
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, body) = send!(&app, get("/api/admin/invites", Some(&admin)));
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // Deleting the creator keeps the invitation (created_by is set to NULL)
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .execute("UPDATE auth_tokens SET created_by = created_by", &[])
        .await
        .unwrap();
    let second_admin = common::test_username("admin2");
    common::create_admin_user(&scratch.pool, &second_admin).await;
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .execute("DELETE FROM users WHERE id = $1", &[&admin_id])
        .await
        .unwrap();
    assert_eq!(
        count(
            &scratch.pool,
            "SELECT COUNT(*) FROM auth_tokens
             WHERE purpose = 'invite' AND created_by IS NULL
               AND used_at IS NULL AND revoked_at IS NULL AND $1::int = 1",
            &1_i32
        )
        .await,
        1
    );
    let (status, _) = register!(&app, "after_creator_left", PASSWORD, Some(keep.as_str()));
    assert_eq!(status, StatusCode::CREATED);
}

// ---------------------------------------------------------------- access and input rules

#[actix_web::test]
async fn only_admins_manage_registration_and_links() {
    let (scratch, _admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let plain = common::test_username("plain");
    common::create_test_user(&scratch.pool, &plain).await;
    let plain_jwt = common::get_test_token(&scratch.pool, &plain).await;
    let any_id = Uuid::new_v4();

    let calls: Vec<(&str, String, Value)> = vec![
        (
            "GET",
            "/api/admin/settings/registration".into(),
            json!(null),
        ),
        (
            "PUT",
            "/api/admin/settings/registration".into(),
            json!({"registration_mode": "open"}),
        ),
        ("POST", "/api/admin/invites".into(), json!({})),
        ("GET", "/api/admin/invites".into(), json!(null)),
        (
            "DELETE",
            format!("/api/admin/invites/{any_id}"),
            json!(null),
        ),
        (
            "POST",
            format!("/api/admin/users/{any_id}/reset-link"),
            json!({}),
        ),
    ];
    for (method, uri, body) in &calls {
        for (who, bearer, expected) in [
            ("nobody", None, StatusCode::UNAUTHORIZED),
            (
                "a regular user",
                Some(plain_jwt.as_str()),
                StatusCode::FORBIDDEN,
            ),
        ] {
            let req = match *method {
                "GET" => get(uri, bearer),
                "PUT" => put(uri, bearer, body.clone()),
                "POST" => post(uri, bearer, body.clone()),
                _ => delete(uri, bearer),
            };
            let (status, _) = send!(&app, req);
            assert_eq!(status, expected, "{method} {uri} as {who}");
        }
    }
    // And the mode really did not change
    let (_, body) = send!(&app, get("/api/auth/setup/status", None));
    assert_eq!(body["data"]["registration_mode"], "invite");
}

#[actix_web::test]
async fn expiry_is_validated_on_the_server() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let someone = common::test_username("someone");
    common::create_test_user(&scratch.pool, &someone).await;
    let someone_id = user_id(&scratch.pool, &someone).await;

    for hours in [0_i64, -5, 721, 100_000] {
        let (status, _) = send!(
            &app,
            post(
                "/api/admin/invites",
                Some(&admin),
                json!({"expires_in_hours": hours})
            )
        );
        assert_eq!(status, StatusCode::BAD_REQUEST, "invite {hours}h");
    }
    for hours in [1_i64, 720] {
        let (status, _) = send!(
            &app,
            post(
                "/api/admin/invites",
                Some(&admin),
                json!({"expires_in_hours": hours})
            )
        );
        assert_eq!(status, StatusCode::CREATED, "invite {hours}h");
    }
    for hours in [0_i64, -1, 169, 100_000] {
        let (status, _) = send!(
            &app,
            post(
                &format!("/api/admin/users/{someone_id}/reset-link"),
                Some(&admin),
                json!({"expires_in_hours": hours})
            )
        );
        assert_eq!(status, StatusCode::BAD_REQUEST, "reset {hours}h");
    }
    for hours in [1_i64, 168] {
        let (status, _) = send!(
            &app,
            post(
                &format!("/api/admin/users/{someone_id}/reset-link"),
                Some(&admin),
                json!({"expires_in_hours": hours})
            )
        );
        assert_eq!(status, StatusCode::CREATED, "reset {hours}h");
    }

    // Defaults: 7 days for an invitation, 24 hours for a reset link
    let within = |body: &Value, hours: i64| {
        let at = chrono::DateTime::parse_from_rfc3339(body["data"]["expires_at"].as_str().unwrap())
            .unwrap();
        let wanted = chrono::Utc::now() + chrono::Duration::hours(hours);
        (at.timestamp() - wanted.timestamp()).abs() < 120
    };
    let (_, invite) = send!(&app, post("/api/admin/invites", Some(&admin), json!({})));
    assert!(within(&invite, 7 * 24), "{invite}");
    let (_, reset) = send!(
        &app,
        post(
            &format!("/api/admin/users/{someone_id}/reset-link"),
            Some(&admin),
            json!({})
        )
    );
    assert!(within(&reset, 24), "{reset}");
}

#[actix_web::test]
async fn tokens_are_stored_only_as_hashes() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let invite = new_invite!(&app, &admin);
    let someone = common::test_username("someone");
    common::create_test_user(&scratch.pool, &someone).await;
    let reset = reset_link!(&app, &admin, user_id(&scratch.pool, &someone).await);

    for token in [&invite, &reset] {
        assert_eq!(
            token.len(),
            43,
            "32 random bytes, base64url without padding"
        );
        assert!(token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    }
    assert_ne!(invite, reset);

    let hashes: Vec<String> = scratch
        .pool
        .get()
        .await
        .unwrap()
        .query("SELECT token_hash FROM auth_tokens", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(hashes.len(), 2);
    for hash in &hashes {
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(!hash.contains(&invite) && !hash.contains(&reset));
    }
}

// ---------------------------------------------------------------- reset links

#[actix_web::test]
async fn a_reset_link_sets_a_new_password_and_signs_every_session_out() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let name = common::test_username("forgetful");
    let (_, old_password) = common::create_test_user(&scratch.pool, &name).await;
    let old_jwt = common::get_test_token(&scratch.pool, &name).await;
    let id = user_id(&scratch.pool, &name).await;
    let db = DatabaseService::new(scratch.pool.clone());
    let version_before = db.get_user_by_id(id).await.unwrap().unwrap().token_version;

    let link = reset_link!(&app, &admin, id);
    let (status, body) = reset!(&app, link, NEW_PASSWORD);
    assert_eq!(status, StatusCode::OK, "{body}");
    // Resetting does not sign anyone in
    assert!(body["data"].get("token").is_none(), "{body}");

    let login = |password: &str| {
        post(
            "/api/auth/login",
            None,
            json!({"username": name, "password": password}),
        )
    };
    assert_eq!(
        send!(&app, login(&old_password)).0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = send!(&app, login(NEW_PASSWORD));
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["token"].is_string());

    // The session from before the reset is dead
    let (status, _) = send!(&app, get("/api/auth/me", Some(&old_jwt)));
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let version_after = db.get_user_by_id(id).await.unwrap().unwrap().token_version;
    assert_eq!(version_after, version_before + 1);

    // A used link is spent
    let (status, body) = reset!(&app, link, "AnotherPassword789!");
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        send!(&app, login("AnotherPassword789!")).0,
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn a_reset_link_follows_the_normal_password_policy_and_survives_a_rejection() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let name = common::test_username("policy");
    common::create_test_user(&scratch.pool, &name).await;
    let id = user_id(&scratch.pool, &name).await;
    let link = reset_link!(&app, &admin, id);

    let (status, body) = reset!(&app, link, "short");
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["message"], "Invalid password");
    let (status, body) = reset!(&app, link, NEW_PASSWORD);
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[actix_web::test]
async fn a_new_reset_link_replaces_the_previous_one_and_other_states_fail() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let name = common::test_username("replace");
    common::create_test_user(&scratch.pool, &name).await;
    let id = user_id(&scratch.pool, &name).await;

    let first = reset_link!(&app, &admin, id);
    let second = reset_link!(&app, &admin, id);
    assert_eq!(reset!(&app, first, NEW_PASSWORD).0, StatusCode::BAD_REQUEST);
    assert_eq!(reset!(&app, second, NEW_PASSWORD).0, StatusCode::OK);

    // Expired
    let third = reset_link!(&app, &admin, id);
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .execute(
            "UPDATE auth_tokens SET created_at = NOW() - INTERVAL '2 days',
                                    expires_at = NOW() - INTERVAL '1 minute'
             WHERE purpose = 'password_reset' AND used_at IS NULL AND revoked_at IS NULL",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(reset!(&app, third, NEW_PASSWORD).0, StatusCode::BAD_REQUEST);

    // An invitation is not a reset link, and nothing at all is not one either
    let invite = new_invite!(&app, &admin);
    assert_eq!(
        reset!(&app, invite, NEW_PASSWORD).0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(reset!(&app, "x", NEW_PASSWORD).0, StatusCode::BAD_REQUEST);
    assert_eq!(
        reset!(&app, "A".repeat(43), NEW_PASSWORD).0,
        StatusCode::BAD_REQUEST
    );
}

#[actix_web::test]
async fn a_reset_does_not_bypass_two_factor_authentication() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let db = DatabaseService::new(scratch.pool.clone());
    let name = common::test_username("twofactor");
    common::create_test_user(&scratch.pool, &name).await;
    let user = db.get_user_by_username(&name).await.unwrap().unwrap();
    let setup = home_registry::auth::totp::generate_totp_setup(&name).unwrap();
    db.create_totp_settings(user.id, &setup.encrypted_secret)
        .await
        .unwrap();
    db.enable_totp(user.id, "both").await.unwrap();

    let link = reset_link!(&app, &admin, user.id);
    assert_eq!(reset!(&app, link, NEW_PASSWORD).0, StatusCode::OK);

    let (status, body) = send!(
        &app,
        post(
            "/api/auth/login",
            None,
            json!({"username": name, "password": NEW_PASSWORD})
        )
    );
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["requires_totp"], true, "{body}");
    assert!(
        body["data"].get("token").is_none(),
        "no full session without the second factor"
    );
}

#[actix_web::test]
async fn deleting_a_user_kills_their_reset_link() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let name = common::test_username("leaving");
    common::create_test_user(&scratch.pool, &name).await;
    let id = user_id(&scratch.pool, &name).await;
    let link = reset_link!(&app, &admin, id);

    let (status, body) = send!(
        &app,
        delete(&format!("/api/admin/users/{id}"), Some(&admin))
    );
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(reset!(&app, link, NEW_PASSWORD).0, StatusCode::BAD_REQUEST);
    assert_eq!(
        count(
            &scratch.pool,
            "SELECT COUNT(*) FROM auth_tokens WHERE user_id = $1",
            &id
        )
        .await,
        0
    );
}

#[actix_web::test]
async fn a_reset_link_for_a_missing_user_is_not_found() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let (status, _) = send!(
        &app,
        post(
            &format!("/api/admin/users/{}/reset-link", Uuid::new_v4()),
            Some(&admin),
            json!({})
        )
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------- notice and startup

#[actix_web::test]
async fn the_upgrade_notice_shows_until_dismissed_or_the_mode_is_chosen() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let show = || async {
        scratch
            .pool
            .get()
            .await
            .unwrap()
            .execute(
                "INSERT INTO app_settings (key, value) VALUES ('registration_upgrade_notice', 'show')
                 ON CONFLICT (key) DO UPDATE SET value = 'show'",
                &[],
            )
            .await
            .unwrap();
    };

    show().await;
    let (_, body) = send!(&app, get("/api/admin/settings/registration", Some(&admin)));
    assert_eq!(body["data"]["upgrade_notice"], true);
    let (status, _) = send!(
        &app,
        put(
            "/api/admin/settings/registration",
            Some(&admin),
            json!({"dismiss_upgrade_notice": true})
        )
    );
    assert_eq!(status, StatusCode::OK);
    let (_, body) = send!(&app, get("/api/admin/settings/registration", Some(&admin)));
    assert_eq!(body["data"]["upgrade_notice"], false);
    assert_eq!(body["data"]["registration_mode"], "invite");

    // Choosing a mode is an answer to the notice
    show().await;
    let (_, body) = send!(
        &app,
        put(
            "/api/admin/settings/registration",
            Some(&admin),
            json!({"registration_mode": "closed"})
        )
    );
    assert_eq!(body["data"]["upgrade_notice"], false);
    assert_eq!(body["data"]["registration_mode"], "closed");
}

#[actix_web::test]
async fn startup_warns_once_that_registration_is_now_invite_only() {
    install_logger();
    let (scratch, _, _) = scratch_with_admin().await;
    let sql = |statement: &'static str| {
        let pool = scratch.pool.clone();
        async move {
            pool.get()
                .await
                .unwrap()
                .execute(statement, &[])
                .await
                .unwrap()
        }
    };
    let warnings = || {
        logged()
            .into_iter()
            .filter(|(level, _, line)| *level == log::Level::Warn && line.contains("invite-only"))
            .count()
    };

    // A fresh install has nothing to warn about
    home_registry::startup::warn_registration_now_invite_only(&scratch.pool).await;
    assert_eq!(warnings(), 0);

    // An upgrade does, and only on the first start
    sql(
        "INSERT INTO app_settings (key, value) VALUES ('registration_upgrade_notice', 'show')
         ON CONFLICT (key) DO UPDATE SET value = 'show'",
    )
    .await;
    home_registry::startup::warn_registration_now_invite_only(&scratch.pool).await;
    assert_eq!(warnings(), 1);
    let message = logged()
        .into_iter()
        .find(|(_, _, line)| line.contains("invite-only"))
        .unwrap()
        .2;
    assert!(
        message.contains("User Management") || message.contains("registration"),
        "{message}"
    );
    home_registry::startup::warn_registration_now_invite_only(&scratch.pool).await;
    home_registry::startup::warn_registration_now_invite_only(&scratch.pool).await;
    assert_eq!(warnings(), 1, "the warning is logged once");
}

// ---------------------------------------------------------------- audit and logs

#[actix_web::test]
async fn audit_lines_name_the_actor_and_the_objects_but_never_a_token() {
    install_logger();
    let (scratch, admin, admin_id) = scratch_with_admin().await;
    let app = app!(scratch.pool);

    let invite = new_invite!(&app, &admin);
    let (status, body) = register!(&app, "audited", PASSWORD, Some(invite.as_str()));
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let new_id = user_id(&scratch.pool, "audited").await;
    let link = reset_link!(&app, &admin, new_id);
    assert_eq!(reset!(&app, link, NEW_PASSWORD).0, StatusCode::OK);
    let (_, listed) = send!(&app, post("/api/admin/invites", Some(&admin), json!({})));
    let revoke_id = listed["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        send!(
            &app,
            delete(&format!("/api/admin/invites/{revoke_id}"), Some(&admin))
        )
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send!(
            &app,
            put(
                "/api/admin/settings/registration",
                Some(&admin),
                json!({"registration_mode": "open"})
            )
        )
        .0,
        StatusCode::OK
    );

    let all = logged();
    let audit: Vec<&String> = all
        .iter()
        .filter(|(_, target, _)| target == "audit")
        .map(|(_, _, line)| line)
        .collect();
    let wants = [
        ("invite_created", admin_id.to_string()),
        ("invite_redeemed", new_id.to_string()),
        ("reset_link_issued", admin_id.to_string()),
        ("password_reset_by_link", new_id.to_string()),
        ("invite_revoked", admin_id.to_string()),
        ("registration_mode_changed", admin_id.to_string()),
    ];
    for (event, id) in wants {
        assert!(
            audit
                .iter()
                .any(|line| line.contains(event) && line.contains(&id)),
            "no audit line `{event}` carrying {id}: {audit:?}"
        );
    }
    // Both ids on the lines that have an acting admin and an object
    assert!(audit.iter().any(|line| line.contains("reset_link_issued")
        && line.contains(&admin_id.to_string())
        && line.contains(&new_id.to_string())));

    // No log line anywhere holds a token or its hash
    let hashes: Vec<String> = scratch
        .pool
        .get()
        .await
        .unwrap()
        .query("SELECT token_hash FROM auth_tokens", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    for (_, _, line) in &all {
        for secret in [&invite, &link].into_iter().chain(hashes.iter()) {
            assert!(
                !line.contains(secret.as_str()),
                "secret in log line: {line}"
            );
        }
    }
}

// ---------------------------------------------------------------- backups

#[actix_web::test]
async fn restoring_a_backup_never_makes_a_token_redeemable_again() {
    let (scratch, admin, _) = scratch_with_admin().await;
    let app = app!(scratch.pool);
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = std::env::temp_dir().join(format!("hr_token_backup_{}", Uuid::new_v4().simple()));

    // Before the backup: one invitation used, one revoked, one waiting, one reset link waiting
    let used = new_invite!(&app, &admin);
    assert_eq!(
        register!(&app, "before_backup", PASSWORD, Some(used.as_str())).0,
        StatusCode::CREATED
    );
    let (revoked, revoked_id) = new_invite_with_id!(&app, &admin);
    assert_eq!(
        send!(
            &app,
            delete(&format!("/api/admin/invites/{revoked_id}"), Some(&admin))
        )
        .0,
        StatusCode::OK
    );
    let waiting = new_invite!(&app, &admin);
    let someone = common::test_username("resetme");
    common::create_test_user(&scratch.pool, &someone).await;
    let reset = reset_link!(&app, &admin, user_id(&scratch.pool, &someone).await);

    let backup = create_backup_file(&db, "home_registry", &dir)
        .await
        .unwrap();

    // After the backup: the waiting invitation and the reset link get used
    let survivor = waiting.clone();
    assert_eq!(
        register!(&app, "after_backup", PASSWORD, Some(survivor.as_str())).0,
        StatusCode::CREATED
    );
    assert_eq!(reset!(&app, reset, NEW_PASSWORD).0, StatusCode::OK);

    let report = restore_backup_file(
        &db,
        &dir,
        &backup.name,
        &RestoreRequest {
            restore_without_2fa: false,
        },
    )
    .await
    .unwrap();
    assert!(report.sessions_ended);

    // The backup held the survivor and the reset link as unused; the restore revoked them
    for (label, token) in [
        ("waiting at backup time", &survivor),
        ("used", &used),
        ("revoked", &revoked),
    ] {
        let (_, body) = check!(&app, token);
        assert_eq!(
            body["data"]["valid"], false,
            "{label} must not be redeemable after a restore"
        );
    }
    assert_eq!(reset!(&app, reset, NEW_PASSWORD).0, StatusCode::BAD_REQUEST);
    let (status, _) = register!(&app, "after_restore", PASSWORD, Some(survivor.as_str()));
    assert_eq!(status, StatusCode::FORBIDDEN);
    let unredeemable = count(
        &scratch.pool,
        "SELECT COUNT(*) FROM auth_tokens WHERE used_at IS NULL AND revoked_at IS NULL AND $1::int = 1",
        &1_i32,
    )
    .await;
    assert_eq!(unredeemable, 0);
    // The registration made after the backup went away with the restore
    assert_eq!(
        count(
            &scratch.pool,
            "SELECT COUNT(*) FROM users WHERE username = $1",
            &"after_backup"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &scratch.pool,
            "SELECT COUNT(*) FROM users WHERE username = $1",
            &"before_backup"
        )
        .await,
        1
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[actix_web::test]
async fn restoring_a_backup_keeps_the_current_registration_mode() {
    let (scratch, _admin, _) = scratch_with_admin().await;
    let db = DatabaseService::new(scratch.pool.clone());
    let dir = std::env::temp_dir().join(format!("hr_mode_backup_{}", Uuid::new_v4().simple()));

    // (mode when the backup was taken, mode when it is restored): the restore never changes who
    // may register, in either direction
    for (at_backup, now) in [
        ("open", "closed"),
        ("open", "invite"),
        ("closed", "open"),
        ("invite", "closed"),
    ] {
        set_mode(&scratch.pool, at_backup).await;
        let backup = create_backup_file(&db, "home_registry", &dir)
            .await
            .unwrap();
        set_mode(&scratch.pool, now).await;

        restore_backup_file(
            &db,
            &dir,
            &backup.name,
            &RestoreRequest {
                restore_without_2fa: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(
            db.get_registration_mode().await.unwrap().as_str(),
            now,
            "backup taken in {at_backup} mode, restored in {now} mode"
        );
    }

    // A server that has never had a mode chosen is invite only, so a backup that was open does
    // not open it; the stored row from the backup is gone rather than carried over
    set_mode(&scratch.pool, "open").await;
    let backup = create_backup_file(&db, "home_registry", &dir)
        .await
        .unwrap();
    scratch
        .pool
        .get()
        .await
        .unwrap()
        .execute(
            "DELETE FROM app_settings WHERE key = 'registration_mode'",
            &[],
        )
        .await
        .unwrap();
    restore_backup_file(
        &db,
        &dir,
        &backup.name,
        &RestoreRequest {
            restore_without_2fa: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(db.get_registration_mode().await.unwrap().as_str(), "invite");
    assert_eq!(
        count(
            &scratch.pool,
            "SELECT COUNT(*) FROM app_settings WHERE key = $1",
            &"registration_mode"
        )
        .await,
        0
    );

    let _ = std::fs::remove_dir_all(&dir);
}
