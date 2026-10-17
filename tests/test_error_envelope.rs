// Every extractor failure under /api answers with the standard error envelope
// `{success: false, error, message}`, never actix's plain-text default, and never echoes the
// received values. Needs no database: extraction fails before any handler runs.

mod common;

use actix_web::{http::StatusCode, test, web, App};
use home_registry::api;
use serde::Deserialize;
use serde_json::{json, Value};

const SENTINEL: &str = "SENTINEL-7f3a9c";

macro_rules! app {
    () => {
        test::init_service(
            App::new()
                .app_data(web::Data::new(common::create_test_pool()))
                .service(api::init_routes()),
        )
        .await
    };
}

/// Handlers whose types reject unknown fields, which no real request type does, mounted with the
/// same extractor config as the API scope.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictBody {
    #[allow(dead_code)]
    known: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictQuery {
    #[allow(dead_code)]
    known: Option<i32>,
}

macro_rules! strict_app {
    () => {
        test::init_service(
            App::new().service(
                web::scope("/strict")
                    .app_data(api::extractors::json_config())
                    .app_data(api::extractors::query_config())
                    .route(
                        "/body",
                        web::post().to(|_: web::Json<StrictBody>| async { "ok" }),
                    )
                    .route(
                        "/query",
                        web::get().to(|_: web::Query<StrictQuery>| async { "ok" }),
                    ),
            ),
        )
        .await
    };
}

/// Send a request; yields (status, content-type, raw body text).
macro_rules! send {
    ($app:expr, $req:expr) => {{
        let resp = test::call_service($app, $req.to_request()).await;
        let status = resp.status();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = String::from_utf8_lossy(&test::read_body(resp).await).to_string();
        (status, content_type, body)
    }};
}

/// Assert the response is the standard envelope and return it parsed.
fn envelope(status: StatusCode, content_type: &str, body: &str, expected: StatusCode) -> Value {
    assert_eq!(status, expected, "body: {body}");
    assert!(
        content_type.starts_with("application/json"),
        "expected a JSON error but got content-type '{content_type}': {body}"
    );
    let value: Value = serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {body}"));
    assert_eq!(value["success"], false, "{body}");
    assert!(value["error"].is_string(), "{body}");
    assert!(
        value["message"].is_string() || value["message"].is_null(),
        "{body}"
    );
    // Exactly the three fields of ErrorResponse, so the frontend sees one shape
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["error", "message", "success"], "{body}");
    value
}

/// Nothing that identifies the implementation may appear in a client-facing message.
fn assert_no_internals(body: &str) {
    for forbidden in [
        "CreateItemRequest",
        "LoginRequest",
        "InventoryReportRequest",
        "home_registry",
        "src/",
        ".rs",
        "line ",
        "column",
        "serde",
        "panicked",
        "Json deserialize error",
        "JSON payload error",
    ] {
        assert!(!body.contains(forbidden), "'{forbidden}' leaked: {body}");
    }
}

fn post_json(uri: &str, body: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri(uri)
        .insert_header(("Content-Type", "application/json"))
        .set_payload(body.to_string())
}

#[actix_web::test]
async fn malformed_and_empty_json_get_the_envelope() {
    let app = app!();
    for body in [r#"{"username": "a","#, "{", "not json at all", ""] {
        let (status, ct, text) = send!(&app, post_json("/api/auth/login", body));
        let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
        assert_eq!(value["error"], "Invalid request body", "{text}");
        assert_eq!(
            value["message"], "The request body is not valid JSON",
            "{text}"
        );
        assert_no_internals(&text);
    }
}

#[actix_web::test]
async fn a_wrong_content_type_gets_the_envelope() {
    let app = app!();
    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .insert_header(("Content-Type", "text/plain"))
        .set_payload(r#"{"username": "a", "password": "b"}"#);
    let (status, ct, text) = send!(&app, req);
    let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
    assert!(
        value["message"]
            .as_str()
            .unwrap()
            .contains("application/json"),
        "{text}"
    );
}

#[actix_web::test]
async fn a_charset_parameter_on_the_content_type_is_still_accepted() {
    let app = app!();
    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .insert_header(("Content-Type", "application/json; charset=utf-8"))
        .set_payload(r#"{"username": "nobody", "password": "x"}"#);
    let (status, _ct, text) = send!(&app, req);
    // The body parsed, so this reached the handler (which may answer 401, or 500 without a
    // database), and it was not rejected as a malformed body
    assert_ne!(status, StatusCode::BAD_REQUEST, "{text}");
    assert_ne!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{text}");
    assert!(!text.contains("Invalid request body"), "{text}");
}

#[actix_web::test]
async fn a_missing_field_is_named_and_a_wrong_type_is_described_without_the_value() {
    let app = app!();

    let (status, ct, text) = send!(&app, post_json("/api/items", r#"{"name": "Drill"}"#));
    let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
    assert_eq!(value["error"], "Invalid request body");
    assert!(
        value["message"]
            .as_str()
            .unwrap()
            .contains("missing field `inventory_id`"),
        "{text}"
    );
    assert_no_internals(&text);

    // Wrong types: the field's expected type is reported, the received value never is
    for (sent, expect) in [
        (
            format!(r#""{SENTINEL}""#),
            "invalid type: string, expected i32",
        ),
        (
            "1.5".to_string(),
            "invalid type: floating point, expected i32",
        ),
        ("true".to_string(), "invalid type: boolean, expected i32"),
        ("null".to_string(), "invalid type: null, expected i32"),
        ("[1, 2]".to_string(), "invalid type: sequence, expected i32"),
        (
            format!(r#"{{"{SENTINEL}": 1}}"#),
            "invalid type: map, expected i32",
        ),
        (
            "99999999999".to_string(),
            "invalid value: integer, expected i32",
        ),
    ] {
        let body = format!(r#"{{"inventory_id": {sent}, "name": "Drill"}}"#);
        let (status, ct, text) = send!(&app, post_json("/api/items", &body));
        let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
        let message = value["message"].as_str().unwrap();
        assert!(message.contains(expect), "wanted '{expect}' in '{message}'");
        assert!(!text.contains(SENTINEL), "received value echoed: {text}");
        assert!(
            !text.contains("99999999999"),
            "received value echoed: {text}"
        );
        assert_no_internals(&text);
    }
}

#[actix_web::test]
async fn an_oversized_body_is_a_413_envelope() {
    let app = app!();
    // Larger than the 15 MiB JSON limit
    let big = format!(r#"{{"username": "{}"}}"#, "a".repeat(16 * 1024 * 1024));
    let (status, ct, text) = send!(&app, post_json("/api/auth/login", &big));
    let value = envelope(status, &ct, &text, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(value["error"], "Request body too large", "{text}");
    assert_no_internals(&text);
}

#[actix_web::test]
async fn bad_query_strings_and_path_ids_get_the_envelope_without_echoing_them() {
    let app = app!();

    let (status, ct, text) = send!(
        &app,
        test::TestRequest::get().uri(&format!("/api/reports/inventory?min_price={SENTINEL}"))
    );
    let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
    assert_eq!(value["message"], "invalid float literal", "{text}");
    assert!(!text.contains(SENTINEL), "{text}");
    assert_no_internals(&text);

    let (status, ct, text) = send!(
        &app,
        test::TestRequest::get().uri(&format!("/api/reports/inventory?inventory_id={SENTINEL}"))
    );
    let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
    assert_eq!(value["message"], "invalid digit found in string", "{text}");
    assert!(!text.contains(SENTINEL), "{text}");

    // A non-numeric id: the framework's own status (404) is kept
    let (status, ct, text) = send!(
        &app,
        test::TestRequest::get().uri(&format!("/api/inventories/{SENTINEL}"))
    );
    let value = envelope(status, &ct, &text, StatusCode::NOT_FOUND);
    assert_eq!(value["error"], "Invalid path parameter", "{text}");
    assert_eq!(
        value["message"], "The path parameter must be a valid i32",
        "{text}"
    );
    assert!(!text.contains(SENTINEL), "{text}");
    assert_no_internals(&text);

    // A format that is not on the allowlist (a malformed uuid) gets the generic message
    let (status, ct, text) = send!(
        &app,
        test::TestRequest::get().uri(&format!("/api/admin/users/{SENTINEL}"))
    );
    let value = envelope(status, &ct, &text, StatusCode::NOT_FOUND);
    assert_eq!(value["message"], "Invalid path parameter", "{text}");
    assert!(!text.contains(SENTINEL), "{text}");
}

#[actix_web::test]
async fn an_unknown_enum_variant_sent_by_the_client_is_not_echoed() {
    let app = app!();
    let body =
        format!(r#"{{"shared_with_username": "someone", "permission_level": "{SENTINEL}"}}"#);
    let (status, ct, text) = send!(&app, post_json("/api/inventories/1/shares", &body));
    let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
    assert_eq!(
        value["message"], "unknown variant, expected one of `view`, `edit_items`, `edit_inventory`",
        "{text}"
    );
    assert!(!text.contains(SENTINEL), "{text}");
    assert_no_internals(&text);
}

#[actix_web::test]
async fn an_unknown_field_name_sent_by_the_client_is_not_echoed() {
    let app = strict_app!();

    let body = format!(r#"{{"known": 1, "{SENTINEL}": 2}}"#);
    let (status, ct, text) = send!(&app, post_json("/strict/body", &body));
    let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
    assert_eq!(
        value["message"], "unknown field, expected `known`",
        "{text}"
    );
    assert!(!text.contains(SENTINEL), "{text}");

    let (status, ct, text) = send!(
        &app,
        test::TestRequest::get().uri(&format!("/strict/query?{SENTINEL}=1"))
    );
    let value = envelope(status, &ct, &text, StatusCode::BAD_REQUEST);
    assert_eq!(
        value["message"], "unknown field, expected `known`",
        "{text}"
    );
    assert!(!text.contains(SENTINEL), "{text}");
}

#[actix_web::test]
async fn unknown_routes_and_wrong_methods_under_api_get_the_envelope() {
    let app = app!();

    for req in [
        test::TestRequest::get().uri("/api/no-such-route"),
        test::TestRequest::post().uri("/api/no/such/route"),
    ] {
        let (status, ct, text) = send!(&app, req);
        let value = envelope(status, &ct, &text, StatusCode::NOT_FOUND);
        assert_eq!(value["error"], "Endpoint not found", "{text}");
        // A fixed sentence: the request path is not echoed
        assert_eq!(
            value["message"], "The requested API endpoint does not exist.",
            "{text}"
        );
        assert!(
            !text.contains("no-such-route") && !text.contains("such"),
            "{text}"
        );
    }

    // A real path with the wrong method is also caught by the scope's fallback: an envelope
    // (404, as before this change), not an empty or plain-text response
    let (status, ct, text) = send!(&app, test::TestRequest::delete().uri("/api/health"));
    let value = envelope(status, &ct, &text, StatusCode::NOT_FOUND);
    assert_eq!(value["error"], "Endpoint not found", "{text}");

    // The query string is not echoed back
    let (status, ct, text) = send!(
        &app,
        test::TestRequest::get().uri(&format!("/api/no-such-route?token={SENTINEL}"))
    );
    envelope(status, &ct, &text, StatusCode::NOT_FOUND);
    assert!(!text.contains(SENTINEL), "{text}");
}

#[actix_web::test]
async fn ordinary_requests_are_unaffected() {
    let app = app!();
    let (status, ct, text) = send!(&app, test::TestRequest::get().uri("/api/health"));
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(ct.starts_with("application/json"));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["status"], "ok");
    let _ = json!(null);
}
