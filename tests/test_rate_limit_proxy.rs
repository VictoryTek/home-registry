// Separate test binary: RateLimits::new sets a process-wide "trust proxy headers" flag, so the
// opt-in case must not share a process with the tests that expect it to be off.

use actix_web::{http::StatusCode, test, App};
use home_registry::api;
use home_registry::rate_limit::{RateLimitSettings, RateLimits};

#[actix_web::test]
async fn forwarded_address_identifies_the_client_when_trusted() {
    let limits = RateLimits::new(RateLimitSettings::from_values(
        Some("1000"),
        Some("2"),
        Some("100"),
        Some("true"),
    ));
    let app = test::init_service(App::new().service(limits.protect(api::init_routes()))).await;
    let proxy = "10.4.0.1:5000".parse().unwrap();

    // Two requests per forwarded client, regardless of the shared proxy address
    for client in ["198.51.100.1", "198.51.100.2"] {
        for _ in 0..2 {
            let req = test::TestRequest::get()
                .uri("/api/health")
                .peer_addr(proxy)
                .insert_header(("X-Forwarded-For", client))
                .to_request();
            assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);
        }
    }
    let req = test::TestRequest::get()
        .uri("/api/health")
        .peer_addr(proxy)
        .insert_header(("X-Forwarded-For", "198.51.100.1"))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}
