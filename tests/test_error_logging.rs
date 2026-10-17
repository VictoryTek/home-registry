// What the server logs when it rejects a request: the method, the route and a category, never
// the body, the query string, the Referer, the User-Agent or serde's message. Also that the rate limiter runs
// before the extractors, so a flood of bad bodies is limited rather than logged without bound.
// Needs no database.

mod common;

use actix_web::{http::StatusCode, test, web, App};
use home_registry::access_log::access_logger;
use home_registry::api;
use home_registry::rate_limit::{RateLimitSettings, RateLimits};
use std::sync::{Mutex, Once};

const SENTINEL: &str = "SENTINEL-4c91d2";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictBody {
    #[allow(dead_code)]
    known: i32,
}

struct CaptureLogger;

static LOGS: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        LOGS.lock()
            .unwrap()
            .push((record.level(), record.args().to_string()));
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

/// Tests that read the shared log run one at a time.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn take_logs() -> Vec<(log::Level, String)> {
    std::mem::take(&mut *LOGS.lock().unwrap())
}

fn rejection_lines(logs: &[(log::Level, String)]) -> Vec<&String> {
    logs.iter()
        .filter(|(level, message)| {
            *level == log::Level::Warn && message.starts_with("Rejected request")
        })
        .map(|(_, message)| message)
        .collect()
}

#[actix_web::test]
async fn secrets_in_bodies_queries_and_headers_never_reach_the_log() {
    install_logger();
    let _serial = SERIAL.lock().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::create_test_pool()))
            .wrap(access_logger())
            .service(api::init_routes()),
    )
    .await;
    take_logs();

    // A wrong-typed JSON value, a bad query value, a bad path id, a Referer and a query string
    // on an ordinary request, all carrying the sentinel
    let requests = [
        test::TestRequest::post()
            .uri("/api/items")
            .insert_header(("Content-Type", "application/json"))
            .set_payload(format!(r#"{{"inventory_id": "{SENTINEL}", "name": "x"}}"#)),
        test::TestRequest::get().uri(&format!("/api/reports/inventory?min_price={SENTINEL}")),
        test::TestRequest::get().uri(&format!("/api/inventories/{SENTINEL}")),
        test::TestRequest::get()
            .uri(&format!("/api/health?token={SENTINEL}"))
            .insert_header((
                "Referer",
                format!("https://example.test/page?secret={SENTINEL}"),
            ))
            .insert_header(("User-Agent", format!("agent/{SENTINEL}"))),
        // Matches no route
        test::TestRequest::get().uri(&format!("/api/no-such-route?q={SENTINEL}")),
    ];
    for req in requests {
        test::call_service(&app, req.to_request()).await;
    }

    let logs = take_logs();
    // The route pattern is logged in place of the path, so the sentinel appears nowhere, not
    // even from the request whose path id carried it
    for (_, line) in &logs {
        assert!(!line.contains(SENTINEL), "received value in log: {line}");
        assert!(
            !line.contains("min_price=")
                && !line.contains("token=")
                && !line.contains("secret=")
                && !line.contains("q="),
            "query string or referer in log: {line}"
        );
    }
    // The wrong-typed JSON value and the bad query value appear nowhere
    let body_and_query_lines = logs
        .iter()
        .filter(|(_, line)| line.contains("/api/items") || line.contains("/api/reports"))
        .count();
    assert!(body_and_query_lines >= 2, "{logs:?}");
    for (_, line) in logs
        .iter()
        .filter(|(_, l)| l.contains("/api/items") || l.contains("/api/reports"))
    {
        assert!(!line.contains(SENTINEL), "received value logged: {line}");
    }

    // Rejections are logged as method + route + category
    let rejected = rejection_lines(&logs);
    assert!(
        rejected
            .iter()
            .any(|l| l.contains("POST \"/api/items\" (json_data)")),
        "{rejected:?}"
    );
    assert!(
        rejected
            .iter()
            .any(|l| l.contains("GET \"/api/reports/inventory\" (query)")),
        "{rejected:?}"
    );
    assert!(
        rejected
            .iter()
            .any(|l| l.contains("GET \"/api/inventories/{id}\" (path)")),
        "{rejected:?}"
    );
    // The access log records the route without the query string or any header text
    assert!(
        logs.iter().any(|(_, l)| l.contains(r#""GET /api/health""#)),
        "access log line with the bare path: {logs:?}"
    );
}

#[actix_web::test]
async fn names_values_and_path_ids_are_logged_nowhere_whatever_the_error_format() {
    install_logger();
    let _serial = SERIAL.lock().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::create_test_pool()))
            .wrap(access_logger())
            .service(api::init_routes())
            .service(
                web::scope("/strict")
                    .app_data(api::extractors::json_config())
                    .route(
                        "/body",
                        web::post().to(|_: web::Json<StrictBody>| async { "ok" }),
                    ),
            ),
    )
    .await;
    take_logs();

    let json = |uri: &str, body: String| {
        test::TestRequest::post()
            .uri(uri)
            .insert_header(("Content-Type", "application/json"))
            .set_payload(body)
    };
    let requests = [
        // An unknown enum variant and an unknown field name
        json(
            "/api/inventories/1/shares",
            format!(r#"{{"shared_with_username": "u", "permission_level": "{SENTINEL}"}}"#),
        ),
        json(
            "/strict/body",
            format!(r#"{{"known": 1, "{SENTINEL}": 2}}"#),
        ),
        // A malformed uuid (a path error format that is not on the allowlist), a non-numeric
        // id in the path, and a bad query value
        test::TestRequest::get().uri(&format!("/api/admin/users/{SENTINEL}")),
        test::TestRequest::get().uri(&format!("/api/inventories/{SENTINEL}")),
        test::TestRequest::get().uri(&format!("/api/reports/inventory?inventory_id={SENTINEL}")),
    ];
    for req in requests {
        test::call_service(&app, req.to_request()).await;
    }

    let logs = take_logs();
    assert!(logs.len() >= 5, "{logs:?}");
    for (_, line) in &logs {
        assert!(!line.contains(SENTINEL), "received value in log: {line}");
    }
    let rejected = rejection_lines(&logs);
    for expected in [
        r#"POST "/api/inventories/{id}/shares" (json_data)"#,
        r#"POST "/strict/body" (json_data)"#,
        r#"GET "/api/admin/users/{id}" (path_unrecognized)"#,
        r#"GET "/api/inventories/{id}" (path)"#,
        r#"GET "/api/reports/inventory" (query)"#,
    ] {
        assert!(
            rejected.iter().any(|l| l.contains(expected)),
            "missing '{expected}' in {rejected:?}"
        );
    }
}

#[actix_web::test]
async fn an_unmatched_path_is_logged_and_echoed_nowhere() {
    install_logger();
    let _serial = SERIAL.lock().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::create_test_pool()))
            .wrap(access_logger())
            .service(api::init_routes()),
    )
    .await;
    take_logs();

    let mut bodies = Vec::new();
    for req in [
        test::TestRequest::get().uri(&format!("/api/{SENTINEL}")),
        test::TestRequest::post().uri(&format!("/api/inventories/{SENTINEL}/nope?q={SENTINEL}")),
        test::TestRequest::delete().uri(&format!("/api/health/{SENTINEL}")),
    ] {
        let resp = test::call_service(&app, req.to_request()).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        bodies.push(String::from_utf8_lossy(&test::read_body(resp).await).to_string());
    }

    for body in &bodies {
        assert!(!body.contains(SENTINEL), "path echoed in response: {body}");
        assert!(
            body.contains("The requested API endpoint does not exist."),
            "{body}"
        );
    }
    let logs = take_logs();
    assert!(logs.len() >= 6, "{logs:?}");
    for (_, line) in &logs {
        assert!(!line.contains(SENTINEL), "path in log: {line}");
    }
    // Method and status are kept; the route is replaced by a constant
    assert!(
        logs.iter()
            .any(|(_, l)| l.contains("API 404: GET <unmatched>")),
        "{logs:?}"
    );
    assert!(
        logs.iter()
            .any(|(_, l)| l.contains(r#""POST <unmatched>" 404"#)),
        "{logs:?}"
    );
}

#[actix_web::test]
async fn an_extractor_failing_on_an_unmatched_request_logs_no_path() {
    install_logger();
    let _serial = SERIAL.lock().await;
    // An extractor on a default service: the request matched no route, so it has no pattern
    let app = test::init_service(
        App::new().service(
            web::scope("/api")
                .app_data(api::extractors::path_config())
                .default_service(web::to(|_: web::Path<i32>| async { "ok" })),
        ),
    )
    .await;
    take_logs();

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/{SENTINEL}"))
            .to_request(),
    )
    .await;
    let body = String::from_utf8_lossy(&test::read_body(resp).await).to_string();
    assert!(!body.contains(SENTINEL), "{body}");

    let logs = take_logs();
    let rejected = rejection_lines(&logs);
    assert_eq!(rejected.len(), 1, "{logs:?}");
    assert!(
        rejected[0].contains(r#"GET "<unmatched>""#),
        "{}",
        rejected[0]
    );
    for (_, line) in &logs {
        assert!(!line.contains(SENTINEL), "path in log: {line}");
    }
}

#[actix_web::test]
async fn an_extension_method_is_logged_as_other() {
    install_logger();
    let _serial = SERIAL.lock().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::create_test_pool()))
            .wrap(access_logger())
            .service(api::init_routes()),
    )
    .await;
    take_logs();

    let method = actix_web::http::Method::from_bytes(format!("X-{SENTINEL}").as_bytes()).unwrap();
    let resp = test::call_service(
        &app,
        test::TestRequest::default()
            .method(method)
            .uri("/api/no-such-route")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    // The access log line is written when the body has been sent
    test::read_body(resp).await;

    let logs = take_logs();
    assert!(logs.len() >= 2, "{logs:?}");
    for (_, line) in &logs {
        assert!(!line.contains(SENTINEL), "method in log: {line}");
    }
    assert!(
        logs.iter()
            .any(|(_, l)| l.contains("API 404: OTHER <unmatched>")),
        "{logs:?}"
    );
    assert!(
        logs.iter()
            .any(|(_, l)| l.contains(r#""OTHER <unmatched>" 404"#)),
        "{logs:?}"
    );
}

#[actix_web::test]
async fn an_uploaded_backup_that_fails_to_parse_is_logged_without_its_content() {
    use home_registry::api::backup::{store_uploaded_backup, UploadError};

    install_logger();
    let _serial = SERIAL.lock().await;
    let dir = std::env::temp_dir().join(format!("hr_upload_log_{}", uuid::Uuid::new_v4().simple()));

    for (content, expected) in [
        // A wrong-typed value, an unknown shape, and plain garbage, all carrying the sentinel
        (
            format!(r#"{{"metadata": "{SENTINEL}", "data": {{}}}}"#),
            "invalid type: string, expected an object",
        ),
        (
            format!(r#"{{"metadata": {{"version": 5, "{SENTINEL}": 1}}, "data": {{}}}}"#),
            "",
        ),
        (
            format!("not json {SENTINEL}"),
            "The request body is not valid JSON",
        ),
    ] {
        take_logs();
        let result = store_uploaded_backup(&dir, "backup.json", content.as_bytes()).await;
        assert!(matches!(result, Err(UploadError::NotABackup)));

        let logs = take_logs();
        let line = logs
            .iter()
            .map(|(_, l)| l)
            .find(|l| l.starts_with("Invalid backup file format"))
            .unwrap_or_else(|| panic!("no log line: {logs:?}"));
        assert!(line.contains(expected), "{line}");
        for (_, l) in &logs {
            assert!(!l.contains(SENTINEL), "upload content in log: {l}");
        }
    }
    assert!(
        !dir.exists(),
        "nothing is written for a file that is not a backup"
    );
}

#[actix_web::test]
async fn control_characters_in_a_path_cannot_forge_log_lines() {
    install_logger();
    let _serial = SERIAL.lock().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::create_test_pool()))
            .service(api::init_routes()),
    )
    .await;
    take_logs();

    for uri in [
        "/api/inventories/%0AWARN%20forged",
        "/api/no-such%0D%0Aroute",
    ] {
        test::call_service(&app, test::TestRequest::get().uri(uri).to_request()).await;
    }
    for (_, line) in take_logs() {
        assert!(
            !line.contains('\n') && !line.contains('\r'),
            "raw line break in log: {line:?}"
        );
    }
}

#[actix_web::test]
async fn the_rate_limiter_runs_before_the_extractors_so_a_flood_is_not_logged_without_bound() {
    install_logger();
    let _serial = SERIAL.lock().await;
    // 3 requests per second per client
    let limits = RateLimits::new(RateLimitSettings::from_values(
        Some("1000"),
        Some("3"),
        Some("1000"),
        None,
    ));
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::create_test_pool()))
            .service(limits.protect(api::init_routes())),
    )
    .await;
    take_logs();

    let mut statuses = Vec::new();
    for _ in 0..12 {
        let req = test::TestRequest::post()
            .uri("/api/auth/login")
            .peer_addr("10.20.0.1:5000".parse().unwrap())
            .insert_header(("Content-Type", "application/json"))
            .set_payload("{ not json")
            .to_request();
        statuses.push(test::call_service(&app, req).await.status());
    }

    let rejected_by_extractor = statuses
        .iter()
        .filter(|s| **s == StatusCode::BAD_REQUEST)
        .count();
    let limited = statuses
        .iter()
        .filter(|s| **s == StatusCode::TOO_MANY_REQUESTS)
        .count();
    assert_eq!(rejected_by_extractor, 3, "{statuses:?}");
    assert_eq!(limited, 9, "{statuses:?}");
    // Only the requests that got past the limiter produced a rejection line
    assert_eq!(rejection_lines(&take_logs()).len(), 3);
}
