//! Setup and bootstrap flow integration tests
//!
//! Covers:
//! - GET /api/setup/required            — whether initial setup is needed
//! - GET /api/setup/bootstrap/status    — bootstrap component status (pre-setup only)
//! - GET /api/setup/generate-credentials — auto-generated admin credentials (pre-setup only)
//! - POST /api/setup/bootstrap/start    — starts bootstrap (before setup)
//! - Self-disabling: most setup endpoints return 403 once an admin user exists

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
// Helpers
// ============================================================================

async fn get(
    state: kubarr::state::AppState,
    uri: &str,
) -> (StatusCode, serde_json::Value) {
    let app = create_router(state);
    let request = Request::builder()
        .uri(uri)
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::json!({}));
    (status, body)
}

// ============================================================================
// GET /api/setup/required
// ============================================================================

#[tokio::test]
async fn test_setup_required_returns_200() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/setup/required")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /api/setup/required must return 200"
    );
}

#[tokio::test]
async fn test_setup_required_true_before_admin() {
    // No admin user → setup is required
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let (status, body) = get(state, "/api/setup/required").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["setup_required"],
        serde_json::Value::Bool(true),
        "setup_required must be true when no admin exists"
    );
    assert_eq!(
        body["database_pending"],
        serde_json::Value::Bool(false),
        "database_pending must be false when the DB is available"
    );
}

#[tokio::test]
async fn test_setup_required_false_after_admin() {
    // Admin user exists → setup is complete
    let db = create_test_db_with_seed().await;
    create_test_user_with_role(&db, "setupadmin", "sa@example.com", "pw", "admin").await;
    let state = build_app_state(db);
    let (status, body) = get(state, "/api/setup/required").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["setup_required"],
        serde_json::Value::Bool(false),
        "setup_required must be false once an admin user exists"
    );
}

#[tokio::test]
async fn test_setup_required_always_accessible() {
    // Even after setup this endpoint must remain publicly accessible
    let db = create_test_db_with_seed().await;
    create_test_user_with_role(&db, "setupchk", "sc@example.com", "pw", "admin").await;
    let state = build_app_state(db);
    let (status, _) = get(state, "/api/setup/required").await;

    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "/api/setup/required must not return 403 after setup"
    );
    assert_eq!(
        status,
        StatusCode::OK,
        "/api/setup/required must remain accessible after setup"
    );
}

// ============================================================================
// GET /api/setup/bootstrap/status
// ============================================================================

#[tokio::test]
async fn test_bootstrap_status_accessible_before_setup() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let (status, _) = get(state, "/api/setup/bootstrap/status").await;

    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "/api/setup/bootstrap/status must be accessible before setup"
    );
    assert_eq!(
        status,
        StatusCode::OK,
        "/api/setup/bootstrap/status must return 200 before setup"
    );
}

#[tokio::test]
async fn test_bootstrap_status_returns_components() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let (status, body) = get(state, "/api/setup/bootstrap/status").await;

    assert_eq!(status, StatusCode::OK);
    // Response must include a components array and completion flags
    assert!(
        body["components"].is_array(),
        "bootstrap status must include a components array"
    );
    assert!(
        body["complete"].is_boolean(),
        "bootstrap status must include a complete flag"
    );
    assert!(
        body["started"].is_boolean(),
        "bootstrap status must include a started flag"
    );
}

#[tokio::test]
async fn test_bootstrap_status_blocked_after_setup() {
    // Once an admin exists the bootstrap status endpoint must return 403
    let db = create_test_db_with_seed().await;
    create_test_user_with_role(&db, "bsadmin", "bs@example.com", "pw", "admin").await;
    let state = build_app_state(db);
    let (status, _) = get(state, "/api/setup/bootstrap/status").await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "/api/setup/bootstrap/status must return 403 after setup is complete"
    );
}

// ============================================================================
// GET /api/setup/generate-credentials
// ============================================================================

#[tokio::test]
async fn test_generate_credentials_accessible_before_setup() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let (status, _) = get(state, "/api/setup/generate-credentials").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "/api/setup/generate-credentials must return 200 before setup"
    );
}

#[tokio::test]
async fn test_generate_credentials_returns_credential_fields() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let (status, body) = get(state, "/api/setup/generate-credentials").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body["admin_username"].is_string(),
        "credentials must include admin_username"
    );
    assert!(
        body["admin_email"].is_string(),
        "credentials must include admin_email"
    );
    assert!(
        body["admin_password"].is_string(),
        "credentials must include admin_password"
    );

    let password = body["admin_password"].as_str().unwrap();
    assert!(
        !password.is_empty(),
        "generated admin_password must not be empty"
    );
}

#[tokio::test]
async fn test_generate_credentials_blocked_after_setup() {
    let db = create_test_db_with_seed().await;
    create_test_user_with_role(&db, "gcadmin", "gc@example.com", "pw", "admin").await;
    let state = build_app_state(db);
    let (status, _) = get(state, "/api/setup/generate-credentials").await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "/api/setup/generate-credentials must return 403 after setup is complete"
    );
}

// ============================================================================
// POST /api/setup/bootstrap/start
// ============================================================================

#[tokio::test]
async fn test_bootstrap_start_accessible_before_setup() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/setup/bootstrap/start")
        .method("POST")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // Should return 200 (started) before setup is complete
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "POST /api/setup/bootstrap/start must be accessible before setup"
    );
}

#[tokio::test]
async fn test_bootstrap_start_returns_started_flag() {
    let db = create_test_db_with_seed().await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/setup/bootstrap/start")
        .method("POST")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(
        body["started"].is_boolean(),
        "bootstrap start response must include a started flag"
    );
    assert!(
        body["message"].is_string(),
        "bootstrap start response must include a message"
    );
}

// ============================================================================
// Self-disabling: all gated setup endpoints return 403 after admin creation
// ============================================================================

#[tokio::test]
async fn test_setup_status_blocked_after_setup() {
    let db = create_test_db_with_seed().await;
    create_test_user_with_role(&db, "ssadmin", "ss@example.com", "pw", "admin").await;
    let state = build_app_state(db);
    let (status, _) = get(state, "/api/setup/status").await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "/api/setup/status must return 403 once setup is complete"
    );
}

#[tokio::test]
async fn test_calling_setup_endpoints_twice_returns_403() {
    // Simulate the scenario where setup is called a second time
    // after an admin already exists — all gated endpoints must refuse.
    let db = create_test_db_with_seed().await;
    create_test_user_with_role(&db, "double", "double@example.com", "pw", "admin").await;
    let state = build_app_state(db);

    let gated_endpoints = vec![
        "/api/setup/status",
        "/api/setup/bootstrap/status",
        "/api/setup/generate-credentials",
    ];

    for endpoint in gated_endpoints {
        let (status, _) = get(state.clone(), endpoint).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{} must return 403 on second attempt (admin already exists)",
            endpoint
        );
    }
}
