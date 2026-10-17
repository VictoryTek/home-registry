// Exercises the real rate-limit middleware stack (RateLimits::protect around the API scope).
// No database is needed: the limiters run before any handler.

use actix_web::{http::StatusCode, test, App};
use home_registry::api;
use home_registry::rate_limit::{RateLimitSettings, RateLimits};
use std::net::SocketAddr;

fn limits(rps: &str, burst: &str, auth_per_minute: &str) -> RateLimits {
    RateLimits::new(RateLimitSettings::from_values(
        Some(rps),
        Some(burst),
        Some(auth_per_minute),
        None,
    ))
}

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap()
}

#[actix_web::test]
async fn burst_limit_applies_per_client_and_returns_retry_after() {
    let limits = limits("1000", "5", "100");
    let app = test::init_service(App::new().service(limits.protect(api::init_routes()))).await;

    for i in 0..5 {
        let req = test::TestRequest::get()
            .uri("/api/health")
            .peer_addr(addr("10.1.0.1:5000"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK, "request {i}");
    }
    let req = test::TestRequest::get()
        .uri("/api/health")
        .peer_addr(addr("10.1.0.1:5000"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(resp.headers().contains_key("retry-after"));

    // Another client has its own bucket
    let req = test::TestRequest::get()
        .uri("/api/health")
        .peer_addr(addr("10.1.0.2:5000"))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);
}

#[actix_web::test]
async fn credential_endpoints_have_their_own_stricter_limit() {
    let limits = limits("1000", "1000", "3");
    let app = test::init_service(App::new().service(limits.protect(api::init_routes()))).await;
    let peer = addr("10.2.0.1:5000");

    // The handler itself fails (no database configured here); only the limiter matters
    for i in 0..3 {
        let req = test::TestRequest::post()
            .uri("/api/auth/login")
            .peer_addr(peer)
            .set_json(serde_json::json!({ "username": "a", "password": "b" }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS, "request {i}");
    }
    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .peer_addr(peer)
        .set_json(serde_json::json!({ "username": "a", "password": "b" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );

    // Other credential endpoints share that client's budget...
    let req = test::TestRequest::post()
        .uri("/api/auth/register")
        .peer_addr(peer)
        .set_json(serde_json::json!({}))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    // ...ordinary API calls from the same client are unaffected...
    let req = test::TestRequest::get()
        .uri("/api/health")
        .peer_addr(peer)
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);
    // ...and so are other clients.
    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .peer_addr(addr("10.2.0.2:5000"))
        .set_json(serde_json::json!({ "username": "a", "password": "b" }))
        .to_request();
    assert_ne!(
        test::call_service(&app, req).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[actix_web::test]
async fn ipv6_clients_in_one_64_share_a_bucket() {
    let limits = limits("1000", "2", "100");
    let app = test::init_service(App::new().service(limits.protect(api::init_routes()))).await;

    for suffix in ["1", "2"] {
        let req = test::TestRequest::get()
            .uri("/api/health")
            .peer_addr(addr(&format!("[2001:db8:5:6::{suffix}]:5000")))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);
    }
    // A third address in the same /64 is over the shared limit
    let req = test::TestRequest::get()
        .uri("/api/health")
        .peer_addr(addr("[2001:db8:5:6:ffff::9]:5000"))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    // A different /64 is separate
    let req = test::TestRequest::get()
        .uri("/api/health")
        .peer_addr(addr("[2001:db8:5:7::1]:5000"))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);
}

#[actix_web::test]
async fn forwarded_headers_are_ignored_by_default() {
    let limits = limits("1000", "2", "100");
    let app = test::init_service(App::new().service(limits.protect(api::init_routes()))).await;
    let peer = addr("10.3.0.1:5000");

    // Varying X-Forwarded-For must not give a client fresh buckets
    for i in 0..2 {
        let req = test::TestRequest::get()
            .uri("/api/health")
            .peer_addr(peer)
            .insert_header(("X-Forwarded-For", format!("203.0.113.{i}")))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);
    }
    let req = test::TestRequest::get()
        .uri("/api/health")
        .peer_addr(peer)
        .insert_header(("X-Forwarded-For", "203.0.113.99"))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[actix_web::test]
async fn percent_encoded_paths_cannot_bypass_the_credential_limit() {
    let limits = limits("1000", "1000", "2");
    let app = test::init_service(App::new().service(limits.protect(api::init_routes()))).await;
    let peer = addr("10.5.0.1:5000");

    // Routing decodes these to /api/auth/login; the limiter must see the same endpoint
    let spellings = [
        "/api/auth/login",
        "/api/auth/%6Cogin",
        "/api/auth/log%69n",
        "/api/auth/login/",
    ];
    let mut statuses = Vec::new();
    for uri in spellings {
        let req = test::TestRequest::post()
            .uri(uri)
            .peer_addr(peer)
            .set_json(serde_json::json!({ "username": "a", "password": "b" }))
            .to_request();
        statuses.push(test::call_service(&app, req).await.status());
    }
    assert_ne!(statuses[0], StatusCode::TOO_MANY_REQUESTS);
    assert_ne!(statuses[1], StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(statuses[2], StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(statuses[3], StatusCode::TOO_MANY_REQUESTS);
}
