//! Health endpoint integration tests
//!
//! Covers:
//! - GET /api/health — simple liveness probe
//! - GET /api/system/health — detailed status including setup readiness

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::util::ServiceExt;

mod common;
use common::{build_app_state, create_test_db_with_seed, create_test_user_with_role};

use kubarr::endpoints::create_router;

// ============================================================================
// GET /api/health
// ============================================================================

#[tokio::test]
async fn test_health_check_returns_200_ok() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /api/health must return 200"
    );
}

#[tokio::test]
async fn test_health_check_body_is_ok() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&body_bytes);

    assert_eq!(body.trim(), "OK", "GET /api/health body must be \"OK\"");
}

#[tokio::test]
async fn test_health_check_no_auth_required() {
    // Health endpoint must be accessible without any authentication
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/health")
        .method("GET")
        // No cookie / authorization header
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /api/health must not require authentication"
    );
}

// ============================================================================
// GET /api/system/health
// ============================================================================

#[tokio::test]
async fn test_system_health_returns_200() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/system/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /api/system/health must return 200"
    );
}

#[tokio::test]
async fn test_system_health_returns_status_ok_field() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/system/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("Response must be valid JSON");

    assert_eq!(
        body["status"], "ok",
        "system/health must contain {{\"status\": \"ok\"}}"
    );
}

#[tokio::test]
async fn test_system_health_without_admin_reports_setup_required() {
    // No admin user → setup_required: true, ready: false
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/system/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("Response must be valid JSON");

    assert_eq!(
        body["setup_required"],
        serde_json::Value::Bool(true),
        "setup_required must be true when no admin exists"
    );
    assert_eq!(
        body["ready"],
        serde_json::Value::Bool(false),
        "ready must be false when no admin exists"
    );
}

#[tokio::test]
async fn test_system_health_with_admin_reports_ready() {
    // Admin user exists → setup_required: false, ready: true
    let db = create_test_db_with_seed().await;
    create_test_user_with_role(&db, "healthadmin", "ha@example.com", "pw", "admin").await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/system/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("Response must be valid JSON");

    assert_eq!(
        body["setup_required"],
        serde_json::Value::Bool(false),
        "setup_required must be false when an admin exists"
    );
    assert_eq!(
        body["ready"],
        serde_json::Value::Bool(true),
        "ready must be true when an admin exists"
    );
}

#[tokio::test]
async fn test_system_health_no_auth_required() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/system/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /api/system/health must not require authentication"
    );
}
