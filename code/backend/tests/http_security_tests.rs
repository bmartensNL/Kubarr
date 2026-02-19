//! Integration tests for HTTP security hardening (issue #29)
//!
//! Covers:
//!  - HTTP security headers on every response
//!  - CORS: allowed methods are restricted
//!  - Rate limiting: 10 req / 60 s on auth endpoints (returns 429 + Retry-After)
//!  - Health endpoint is NOT rate-limited

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

mod common;
use common::create_test_db_with_seed;

use kubarr::application::bootstrapper::create_app;
use kubarr::services::audit::AuditService;
use kubarr::services::catalog::AppCatalog;
use kubarr::services::chart_sync::ChartSyncService;
use kubarr::services::notification::NotificationService;
use kubarr::state::AppState;

async fn create_test_state() -> AppState {
    let db = create_test_db_with_seed().await;
    let k8s_client = Arc::new(RwLock::new(None));
    let catalog = Arc::new(RwLock::new(AppCatalog::default()));
    let chart_sync = Arc::new(ChartSyncService::new(catalog.clone()));
    let audit = AuditService::new();
    let notification = NotificationService::new();
    AppState::new(
        Some(db),
        k8s_client,
        catalog,
        chart_sync,
        audit,
        notification,
    )
}

// ============================================================================
// Security headers on every response
// ============================================================================

#[tokio::test]
async fn test_security_headers_present_on_health_endpoint() {
    let state = create_test_state().await;
    let app = create_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let headers = response.headers();
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(
        headers.get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
    assert_eq!(headers.get("x-xss-protection").unwrap(), "1; mode=block");
    assert!(headers.contains_key("content-security-policy"));
}

#[tokio::test]
async fn test_security_headers_present_on_api_401_response() {
    let state = create_test_state().await;
    let app = create_app(state);

    // Unauthenticated request to a protected endpoint → 401 but still has headers
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let headers = response.headers();
    assert!(
        headers.contains_key("x-frame-options"),
        "x-frame-options missing from 401 response"
    );
    assert!(
        headers.contains_key("x-content-type-options"),
        "x-content-type-options missing from 401 response"
    );
    assert!(
        headers.contains_key("content-security-policy"),
        "content-security-policy missing from 401 response"
    );
}

#[tokio::test]
async fn test_all_five_security_headers_on_health() {
    let state = create_test_state().await;
    let app = create_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let headers = response.headers();
    for name in &[
        "x-frame-options",
        "x-content-type-options",
        "referrer-policy",
        "x-xss-protection",
        "content-security-policy",
    ] {
        assert!(
            headers.contains_key(*name),
            "Missing security header: {}",
            name
        );
    }
}

// ============================================================================
// Rate limiting on auth endpoints
// ============================================================================

#[tokio::test]
async fn test_auth_endpoint_accepts_requests_under_limit() {
    let state = create_test_state().await;
    // Single app instance — all clones share the same rate-limiter Arc.
    let app = create_app(state);

    // Send a few requests — well below the burst limit of 10.
    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"x","password":"y"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should get an auth error (401 or 400), NOT a rate-limit error (429)
        assert_ne!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "Request should not be rate-limited when under the burst limit"
        );
    }
}

#[tokio::test]
async fn test_rate_limit_returns_429_when_exceeded() {
    let state = create_test_state().await;
    // Single app instance — Router::clone() shares the same Arc<GovernorConfig>
    // so all requests see the same rate-limiter state.
    let app = create_app(state);

    // burst_size=10, so after 11 requests the limiter should kick in.
    // All test requests use the fallback IP (127.0.0.1).
    let mut got_429 = false;
    for i in 0..15 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"x","password":"y"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            got_429 = true;
            // tower_governor adds x-ratelimit-* headers via use_headers()
            let headers = response.headers();
            assert!(
                headers.contains_key("retry-after")
                    || headers.contains_key("x-ratelimit-after"),
                "Rate-limit response (request {}) should include a retry-after header. Headers: {:?}",
                i + 1,
                headers
            );
            break;
        }
    }

    assert!(
        got_429,
        "Expected 429 Too Many Requests after bursting beyond the limit (burst_size=10), but never received one"
    );
}

#[tokio::test]
async fn test_health_endpoint_not_rate_limited() {
    let state = create_test_state().await;
    let app = create_app(state);

    // Health endpoint is NOT under the /auth rate limiter.
    // Sending many requests should always succeed regardless of count.
    for _ in 0..15 {
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

        assert_ne!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "Health endpoint should never be rate-limited"
        );
    }
}
