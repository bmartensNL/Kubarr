//! Integration tests for HTTP security hardening
//!
//! Covers:
//! - Security headers middleware (all 5 headers present on every response)
//! - Rate limiting on auth endpoints (429 after burst is exhausted)
//! - CORS restricted methods (only the configured set is allowed)

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::util::ServiceExt; // for `oneshot`

mod common;
use common::create_test_db_with_seed;

use kubarr::endpoints::create_router;
use kubarr::services::audit::AuditService;
use kubarr::services::catalog::AppCatalog;
use kubarr::services::chart_sync::ChartSyncService;
use kubarr::services::notification::NotificationService;
use kubarr::state::AppState;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Create a minimal AppState suitable for testing
async fn create_test_state() -> AppState {
    let db = create_test_db_with_seed().await;
    let k8s_client = Arc::new(RwLock::new(None));
    let catalog = Arc::new(RwLock::new(AppCatalog::default()));
    let chart_sync = Arc::new(ChartSyncService::new(catalog.clone()));
    let audit = AuditService::new();
    let notification = NotificationService::new();

    AppState::new(Some(db), k8s_client, catalog, chart_sync, audit, notification)
}

// ==========================================================================
// Security Headers Tests
// ==========================================================================

#[tokio::test]
async fn test_security_headers_on_health_endpoint() {
    let app = create_router(create_test_state().await);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let h = response.headers();
    assert_eq!(h.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(h.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(
        h.get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
    assert_eq!(h.get("x-xss-protection").unwrap(), "1; mode=block");
    assert!(h.contains_key("content-security-policy"));
}

#[tokio::test]
async fn test_security_headers_on_protected_endpoint() {
    // Even a 401 response should carry security headers
    let app = create_router(create_test_state().await);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Unauthenticated → 401, but headers must still be present
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let h = response.headers();
    assert!(
        h.contains_key("x-frame-options"),
        "missing x-frame-options on 401"
    );
    assert!(
        h.contains_key("x-content-type-options"),
        "missing x-content-type-options on 401"
    );
    assert!(
        h.contains_key("content-security-policy"),
        "missing content-security-policy on 401"
    );
}

#[tokio::test]
async fn test_csp_contains_required_directives() {
    let app = create_router(create_test_state().await);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let csp = response
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();

    assert!(csp.contains("default-src 'self'"), "CSP missing default-src");
    assert!(csp.contains("script-src 'self'"), "CSP missing script-src");
    assert!(
        csp.contains("style-src 'self' 'unsafe-inline'"),
        "CSP missing style-src"
    );
}

// ==========================================================================
// Rate Limiting Tests
// ==========================================================================

#[tokio::test]
async fn test_auth_endpoint_rate_limited_after_burst() {
    let app = create_router(create_test_state().await);

    // The burst size is 10, so the 11th request (index 10) should be rate-limited.
    // All clones share the same Arc<GovernorConfig>, hence the same rate-limit state.
    let mut last_status = StatusCode::OK;

    for i in 0..=10u32 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"x","password":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        if i == 10 {
            last_status = response.status();
        }
    }

    assert_eq!(
        last_status,
        StatusCode::TOO_MANY_REQUESTS,
        "expected 429 after burst is exhausted"
    );
}

#[tokio::test]
async fn test_rate_limited_response_has_retry_after_header() {
    let app = create_router(create_test_state().await);

    // Exhaust the burst
    for _ in 0..10u32 {
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"x","password":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // 11th request should be rate-limited
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"username":"x","password":"x"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    // tower-governor includes a Retry-After header on 429 responses
    assert!(
        response.headers().contains_key("retry-after"),
        "429 response should contain Retry-After header"
    );
}

#[tokio::test]
async fn test_health_endpoint_not_rate_limited() {
    // The health endpoint is NOT under /auth/, so it must never hit the auth rate limiter.
    let app = create_router(create_test_state().await);

    // Send well over the burst_size=10 requests; all should succeed.
    for _ in 0..15u32 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "health endpoint should never be rate-limited"
        );
    }
}

// ==========================================================================
// CORS Tests
// ==========================================================================

#[tokio::test]
async fn test_cors_options_request_succeeds() {
    let app = create_router(create_test_state().await);

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/health")
                .header("Origin", "http://localhost:3000")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // CORS preflight must not return 405 Method Not Allowed
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "OPTIONS should be an allowed CORS method"
    );
}

#[tokio::test]
async fn test_delete_method_allowed_by_cors() {
    let app = create_router(create_test_state().await);

    // DELETE is now explicitly in the allow_methods list.
    // A preflight for DELETE should not be refused.
    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/users/1")
                .header("Origin", "http://localhost:3000")
                .header("Access-Control-Request-Method", "DELETE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "DELETE should be in CORS allowed methods"
    );
}

// ==========================================================================
// Response body sanity on 429
// ==========================================================================

#[tokio::test]
async fn test_rate_limited_response_body() {
    let app = create_router(create_test_state().await);

    // Exhaust burst
    for _ in 0..10u32 {
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"x","password":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"username":"x","password":"x"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    // The response body should be non-empty (tower-governor provides one)
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!body_bytes.is_empty(), "429 response should have a body");
}
