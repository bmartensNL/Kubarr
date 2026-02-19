//! Account Lockout Integration Tests
//!
//! Tests for brute-force protection via account lockout after repeated
//! failed login attempts.
//!
//! Covers:
//! - Failed login attempts increment `failed_login_count`
//! - Account is locked after reaching the threshold (default: 10)
//! - Locked account returns HTTP 429 with `Retry-After` header
//! - Successful login resets the failure counter
//! - Admin can unlock a locked account via `POST /api/users/{id}/unlock`

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::util::ServiceExt;

mod common;
use common::{create_test_db_with_seed, create_test_user, create_test_user_with_role};

use kubarr::endpoints::create_router;
use kubarr::models::prelude::*;
use kubarr::models::{session, user};
use kubarr::services::audit::AuditService;
use kubarr::services::catalog::AppCatalog;
use kubarr::services::chart_sync::ChartSyncService;
use kubarr::services::notification::NotificationService;
use kubarr::services::security::init_jwt_keys;
use kubarr::state::AppState;

/// Create a test AppState with the correct 6-parameter signature
async fn create_test_state() -> AppState {
    let db = create_test_db_with_seed().await;
    let k8s_client = Arc::new(RwLock::new(None));
    let catalog = Arc::new(RwLock::new(AppCatalog::default()));
    let chart_sync = Arc::new(ChartSyncService::new(catalog.clone()));
    let audit = AuditService::new();
    let notification = NotificationService::new();

    let state = AppState::new(Some(db.clone()), k8s_client, catalog, chart_sync, audit, notification);
    // Initialize audit service with db
    state.audit.set_db(db).await;
    state
}

/// Create a test AppState with JWT keys initialized (needed for session token creation)
async fn create_test_state_with_jwt() -> AppState {
    let db = create_test_db_with_seed().await;

    // Initialize JWT keys so we can create session tokens
    init_jwt_keys(&db).await.expect("Failed to init JWT keys");

    let k8s_client = Arc::new(RwLock::new(None));
    let catalog = Arc::new(RwLock::new(AppCatalog::default()));
    let chart_sync = Arc::new(ChartSyncService::new(catalog.clone()));
    let audit = AuditService::new();
    let notification = NotificationService::new();

    let state = AppState::new(Some(db.clone()), k8s_client, catalog, chart_sync, audit, notification);
    state.audit.set_db(db).await;
    state
}

/// Make a POST /auth/login request and return (status, body, headers)
async fn make_login_request(
    state: AppState,
    username: &str,
    password: &str,
) -> (StatusCode, String, axum::http::HeaderMap) {
    let app = create_router(state);
    let body = serde_json::json!({
        "username": username,
        "password": password
    });

    let request = Request::builder()
        .uri("/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    (status, body_str, headers)
}

/// Create an authenticated admin session and return the session cookie value
async fn create_admin_session(state: &AppState) -> String {
    use kubarr::services::security::create_session_token;

    let db = state.get_db().await.unwrap();

    let admin = create_test_user_with_role(&db, "admin_lock", "admin_lock@test.com", "Admin1234!", "admin").await;

    let session_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    let session_model = session::ActiveModel {
        id: Set(session_id.clone()),
        user_id: Set(admin.id),
        user_agent: Set(None),
        ip_address: Set(None),
        created_at: Set(now),
        expires_at: Set(now + Duration::days(7)),
        last_accessed_at: Set(now),
        is_revoked: Set(false),
    };
    session_model.insert(&db).await.unwrap();

    let token = create_session_token(&session_id).expect("Failed to create session token");

    // Return cookie header value in the format used by the app
    format!("kubarr_session_0={}; kubarr_active=0", token)
}

// ============================================================================
// Test: Failed login increments counter
// ============================================================================

#[tokio::test]
async fn test_failed_login_increments_counter() {
    let state = create_test_state().await;
    let db = state.get_db().await.unwrap();

    let _user = create_test_user(&db, "locktest1", "locktest1@test.com", "CorrectPass1!", true).await;

    // Make one failed login attempt
    let (status, _body, _headers) =
        make_login_request(state.clone(), "locktest1", "WrongPassword").await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Failed login should return 401"
    );

    // Check the counter was incremented
    let updated_user = User::find()
        .filter(user::Column::Username.eq("locktest1"))
        .one(&db)
        .await
        .unwrap()
        .expect("User not found");

    assert_eq!(
        updated_user.failed_login_count, 1,
        "Failed login count should be incremented to 1"
    );
    assert!(
        updated_user.locked_until.is_none(),
        "Account should not be locked after 1 failure"
    );
}

// ============================================================================
// Test: Account locks after threshold failures
// ============================================================================

#[tokio::test]
async fn test_lockout_after_threshold_failures() {
    let state = create_test_state().await;
    let db = state.get_db().await.unwrap();

    create_test_user(&db, "locktest2", "locktest2@test.com", "CorrectPass2!", true).await;

    // Make 10 failed login attempts (default threshold)
    // All 10 should return 401; the lockout is recorded in DB after the 10th
    for i in 0..10 {
        let (status, _body, _headers) =
            make_login_request(state.clone(), "locktest2", "WrongPassword").await;

        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Attempt {} should return 401 (lockout recorded after this request, not during)",
            i + 1
        );
    }

    // The 10th failure should trigger lockout - check DB state
    let locked_user = User::find()
        .filter(user::Column::Username.eq("locktest2"))
        .one(&db)
        .await
        .unwrap()
        .expect("User not found");

    assert!(
        locked_user.locked_until.is_some(),
        "Account should be locked after 10 failures"
    );
    assert_eq!(
        locked_user.failed_login_count, 0,
        "Failure count should be reset to 0 after lockout is applied"
    );

    // The lockout should be in the future
    let locked_until = locked_user.locked_until.unwrap();
    assert!(
        locked_until > Utc::now(),
        "locked_until should be in the future"
    );
}

// ============================================================================
// Test: Locked account returns 429 with Retry-After header
// ============================================================================

#[tokio::test]
async fn test_locked_account_returns_429() {
    let state = create_test_state().await;
    let db = state.get_db().await.unwrap();

    // Create user with lockout already applied (simulating post-lockout state)
    let test_user = create_test_user(&db, "locktest3", "locktest3@test.com", "CorrectPass3!", true).await;

    // Manually set the lockout
    let locked_until = Utc::now() + Duration::minutes(15);
    let mut user_model: user::ActiveModel = test_user.into();
    user_model.locked_until = Set(Some(locked_until));
    user_model.failed_login_count = Set(0);
    user_model.updated_at = Set(Utc::now());
    user_model.update(&db).await.unwrap();

    // Attempt login while locked - should get 429
    let (status, body, headers) =
        make_login_request(state.clone(), "locktest3", "CorrectPass3!").await;

    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "Locked account should return 429, body: {}",
        body
    );

    // Verify Retry-After header is present
    assert!(
        headers.contains_key("retry-after"),
        "Response should include Retry-After header"
    );

    let retry_after = headers
        .get("retry-after")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();

    assert!(
        retry_after > 0 && retry_after <= 900,
        "Retry-After should be a positive number of seconds (got {})",
        retry_after
    );

    // Verify the response body contains a useful message
    assert!(
        body.contains("locked") || body.contains("minute"),
        "Response body should mention lockout, got: {}",
        body
    );
}

// ============================================================================
// Test: Locked account does not increment counter further
// ============================================================================

#[tokio::test]
async fn test_locked_account_does_not_increment_counter() {
    let state = create_test_state().await;
    let db = state.get_db().await.unwrap();

    let test_user = create_test_user(&db, "locktest4", "locktest4@test.com", "CorrectPass4!", true).await;

    // Set lockout
    let locked_until = Utc::now() + Duration::minutes(15);
    let mut user_model: user::ActiveModel = test_user.into();
    user_model.locked_until = Set(Some(locked_until));
    user_model.failed_login_count = Set(0);
    user_model.updated_at = Set(Utc::now());
    user_model.update(&db).await.unwrap();

    // Try login multiple times while locked
    for _ in 0..3 {
        let (status, _body, _headers) =
            make_login_request(state.clone(), "locktest4", "WrongPassword").await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    }

    // Counter should still be 0 (not incremented during lockout)
    let user_after = User::find()
        .filter(user::Column::Username.eq("locktest4"))
        .one(&db)
        .await
        .unwrap()
        .expect("User not found");

    assert_eq!(
        user_after.failed_login_count, 0,
        "Counter should not increment during active lockout"
    );
}

// ============================================================================
// Test: Successful login resets failure counter
// ============================================================================

#[tokio::test]
async fn test_successful_login_resets_counter() {
    let state = create_test_state_with_jwt().await;
    let db = state.get_db().await.unwrap();

    let test_user = create_test_user(&db, "locktest5", "locktest5@test.com", "CorrectPass5!", true).await;

    // Manually set a non-zero failure count (but not locked)
    let mut user_model: user::ActiveModel = test_user.into();
    user_model.failed_login_count = Set(5);
    user_model.updated_at = Set(Utc::now());
    user_model.update(&db).await.unwrap();

    // Successful login
    let (status, _body, _headers) =
        make_login_request(state.clone(), "locktest5", "CorrectPass5!").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "Login with correct credentials should succeed"
    );

    // Counter should be reset
    let user_after = User::find()
        .filter(user::Column::Username.eq("locktest5"))
        .one(&db)
        .await
        .unwrap()
        .expect("User not found");

    assert_eq!(
        user_after.failed_login_count, 0,
        "Failure counter should be reset to 0 after successful login"
    );
    assert!(
        user_after.locked_until.is_none(),
        "locked_until should be cleared after successful login"
    );
}

// ============================================================================
// Test: Unlock endpoint requires authentication
// ============================================================================

#[tokio::test]
async fn test_unlock_endpoint_requires_auth() {
    let state = create_test_state().await;

    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/users/1/unlock")
        .method("POST")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Unlock endpoint should require authentication"
    );
}

// ============================================================================
// Test: Admin can unlock a locked account
// ============================================================================

#[tokio::test]
async fn test_admin_can_unlock_account() {
    let state = create_test_state_with_jwt().await;
    let db = state.get_db().await.unwrap();

    // Create a regular user and lock them
    let locked_user =
        create_test_user(&db, "locktest6", "locktest6@test.com", "CorrectPass6!", true).await;
    let user_id = locked_user.id;

    let locked_until = Utc::now() + Duration::minutes(15);
    let mut user_model: user::ActiveModel = locked_user.into();
    user_model.locked_until = Set(Some(locked_until));
    user_model.failed_login_count = Set(0);
    user_model.updated_at = Set(Utc::now());
    user_model.update(&db).await.unwrap();

    // Create an admin session
    let session_cookie = create_admin_session(&state).await;

    let app = create_router(state.clone());

    let request = Request::builder()
        .uri(format!("/api/users/{}/unlock", user_id))
        .method("POST")
        .header("cookie", session_cookie)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    assert_eq!(
        status,
        StatusCode::OK,
        "Admin should be able to unlock account, body: {}",
        body
    );

    // Verify account is actually unlocked
    let user_after = User::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .expect("User not found");

    assert!(
        user_after.locked_until.is_none(),
        "locked_until should be cleared after admin unlock"
    );
    assert_eq!(
        user_after.failed_login_count, 0,
        "failed_login_count should be 0 after admin unlock"
    );
}

// ============================================================================
// Test: Unlock endpoint returns 404 for non-existent user
// ============================================================================

#[tokio::test]
async fn test_unlock_nonexistent_user_returns_404() {
    let state = create_test_state_with_jwt().await;

    let session_cookie = create_admin_session(&state).await;
    let app = create_router(state);

    let request = Request::builder()
        .uri("/api/users/99999/unlock")
        .method("POST")
        .header("cookie", session_cookie)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Unlocking non-existent user should return 404"
    );
}

// ============================================================================
// Test: Expired lockout is ignored (lockout in the past)
// ============================================================================

#[tokio::test]
async fn test_expired_lockout_allows_login() {
    let state = create_test_state_with_jwt().await;
    let db = state.get_db().await.unwrap();

    let test_user = create_test_user(&db, "locktest7", "locktest7@test.com", "CorrectPass7!", true).await;

    // Set lockout in the past (already expired)
    let locked_until = Utc::now() - Duration::minutes(5);
    let mut user_model: user::ActiveModel = test_user.into();
    user_model.locked_until = Set(Some(locked_until));
    user_model.failed_login_count = Set(0);
    user_model.updated_at = Set(Utc::now());
    user_model.update(&db).await.unwrap();

    // Login should succeed even with past lockout
    let (status, body, _headers) =
        make_login_request(state.clone(), "locktest7", "CorrectPass7!").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "Login should succeed when lockout has expired, body: {}",
        body
    );
}
