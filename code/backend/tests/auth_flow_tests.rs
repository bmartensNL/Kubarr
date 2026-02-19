//! Auth flow integration tests
//!
//! Covers the full authentication lifecycle:
//! - POST /auth/login  — valid credentials, invalid credentials, inactive/unapproved account
//! - POST /auth/logout — invalidates session cookie
//! - GET  /auth/sessions        — lists active sessions (authenticated)
//! - DELETE /auth/sessions/:id  — revokes a session (authenticated)

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::util::ServiceExt;

mod common;
use common::{
    build_app_state, create_test_db_with_seed, create_test_session, create_test_user,
    create_test_user_with_role, init_test_jwt_keys,
};

use kubarr::endpoints::create_router;
use kubarr::models::user;
use kubarr::services::security::hash_password;
use sea_orm::{ActiveModelTrait, Set};

// ============================================================================
// Helpers
// ============================================================================

/// POST /auth/login with the supplied credentials.
/// Returns (status, response_body_json, optional_session_cookie).
async fn do_login(
    state: kubarr::state::AppState,
    username: &str,
    password: &str,
) -> (StatusCode, serde_json::Value, Option<String>) {
    let app = create_router(state);
    let payload = serde_json::json!({"username": username, "password": password}).to_string();

    let request = Request::builder()
        .uri("/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(payload))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();

    // Extract the legacy session cookie (kubarr_session=<token>) before consuming body
    let cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .find_map(|h| {
            let s = h.to_str().ok()?;
            // Match "kubarr_session=..." but NOT "kubarr_session_0=..." (indexed)
            if s.starts_with("kubarr_session=") && !s.starts_with("kubarr_session_") {
                let token = s
                    .strip_prefix("kubarr_session=")?
                    .splitn(2, ';')
                    .next()?;
                Some(format!("kubarr_session={}", token))
            } else {
                None
            }
        });

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::json!({}));

    (status, body, cookie)
}

// ============================================================================
// Login — valid credentials
// ============================================================================

#[tokio::test]
async fn test_login_valid_credentials_returns_200() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    create_test_user_with_role(&db, "loginuser", "login@example.com", "correct_pw", "admin").await;

    let state = build_app_state(db);
    let (status, body, cookie) = do_login(state, "loginuser", "correct_pw").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "Valid login must return 200. Body: {}",
        body
    );
    assert!(cookie.is_some(), "Login must set a session cookie");
}

#[tokio::test]
async fn test_login_returns_user_info() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    create_test_user_with_role(&db, "infouser", "info@example.com", "pw123", "admin").await;

    let state = build_app_state(db);
    let (status, body, _) = do_login(state, "infouser", "pw123").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], "infouser");
    assert_eq!(body["email"], "info@example.com");
    assert!(body["user_id"].is_number(), "response must include user_id");
}

#[tokio::test]
async fn test_login_sets_session_cookie() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    create_test_user_with_role(&db, "cookieuser", "cookie@example.com", "pw", "admin").await;

    let state = build_app_state(db);
    let (_, _, cookie) = do_login(state, "cookieuser", "pw").await;

    let cookie_str = cookie.expect("Session cookie must be present after successful login");
    assert!(
        cookie_str.starts_with("kubarr_session="),
        "Cookie must use the kubarr_session name"
    );
    // Token part must not be empty
    let token = cookie_str.strip_prefix("kubarr_session=").unwrap();
    assert!(!token.is_empty(), "Session token must not be empty");
}

// ============================================================================
// Login — invalid / rejected credentials
// ============================================================================

#[tokio::test]
async fn test_login_wrong_password_returns_401() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    create_test_user_with_role(&db, "wrongpw", "wp@example.com", "correct", "admin").await;

    let state = build_app_state(db);
    let (status, _, _) = do_login(state, "wrongpw", "incorrect").await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Wrong password must return 401"
    );
}

#[tokio::test]
async fn test_login_nonexistent_user_returns_401() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;

    let state = build_app_state(db);
    let (status, _, _) = do_login(state, "nobody", "anything").await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Unknown user must return 401"
    );
}

#[tokio::test]
async fn test_login_unapproved_account_returns_401() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    // is_approved = false
    create_test_user(&db, "pending", "pending@example.com", "pw", false).await;

    let state = build_app_state(db);
    let (status, _, _) = do_login(state, "pending", "pw").await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Unapproved account must return 401"
    );
}

#[tokio::test]
async fn test_login_inactive_account_returns_401() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;

    // Create a user with is_active = false directly
    let hashed = hash_password("pw").unwrap();
    let now = chrono::Utc::now();
    let inactive_user = user::ActiveModel {
        username: Set("inactive".to_string()),
        email: Set("inactive@example.com".to_string()),
        hashed_password: Set(hashed),
        is_active: Set(false),
        is_approved: Set(true),
        totp_secret: Set(None),
        totp_enabled: Set(false),
        totp_verified_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    inactive_user.insert(&db).await.unwrap();

    let state = build_app_state(db);
    let (status, _, _) = do_login(state, "inactive", "pw").await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Inactive account must return 401"
    );
}

// ============================================================================
// Logout
// ============================================================================

#[tokio::test]
async fn test_logout_returns_200() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    let state = build_app_state(db);
    let app = create_router(state);

    // Logout without a session is valid (no-op)
    let request = Request::builder()
        .uri("/auth/logout")
        .method("POST")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "POST /auth/logout must return 200"
    );
}

#[tokio::test]
async fn test_logout_sets_clear_cookie() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/auth/logout")
        .method("POST")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The response must clear the session cookie (Max-Age=0)
    let clears_cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .any(|h| {
            let s = h.to_str().unwrap_or("");
            s.contains("Max-Age=0") || s.contains("kubarr_session=;")
        });

    assert!(
        clears_cookie,
        "POST /auth/logout must clear the session cookie"
    );
}

#[tokio::test]
async fn test_logout_after_login_returns_200() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    create_test_user_with_role(&db, "logoutuser", "logout@example.com", "pw", "admin").await;

    // Login first
    let state = build_app_state(db);
    let (login_status, _, cookie) = do_login(state.clone(), "logoutuser", "pw").await;
    assert_eq!(login_status, StatusCode::OK, "Login must succeed");
    let cookie_str = cookie.expect("Cookie must be present");

    // Then logout with the session cookie
    let app = create_router(state);
    let request = Request::builder()
        .uri("/auth/logout")
        .method("POST")
        .header("cookie", &cookie_str)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Logout with active session must return 200"
    );
}

// ============================================================================
// GET /auth/sessions — list sessions
// ============================================================================

#[tokio::test]
async fn test_list_sessions_without_auth_returns_401() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/auth/sessions")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /auth/sessions without auth must return 401"
    );
}

#[tokio::test]
async fn test_list_sessions_authenticated_returns_200() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    create_test_user_with_role(&db, "sessuser", "sess@example.com", "pw", "admin").await;

    // Login to get a session cookie
    let state = build_app_state(db);
    let (login_status, _, cookie) = do_login(state.clone(), "sessuser", "pw").await;
    assert_eq!(login_status, StatusCode::OK, "Login must succeed");
    let cookie_str = cookie.expect("Cookie must be present");

    // Use the cookie to list sessions
    let app = create_router(state);
    let request = Request::builder()
        .uri("/auth/sessions")
        .method("GET")
        .header("cookie", &cookie_str)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Authenticated GET /auth/sessions must return 200"
    );

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let sessions: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("Response must be valid JSON");

    assert!(sessions.is_array(), "sessions response must be an array");
    assert!(
        !sessions.as_array().unwrap().is_empty(),
        "There must be at least one active session after login"
    );
}

#[tokio::test]
async fn test_list_sessions_contains_current_session() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    create_test_user_with_role(&db, "currsess", "cs@example.com", "pw", "admin").await;

    let state = build_app_state(db);
    let (_, _, cookie) = do_login(state.clone(), "currsess", "pw").await;
    let cookie_str = cookie.expect("Cookie must be present");

    let app = create_router(state);
    let request = Request::builder()
        .uri("/auth/sessions")
        .method("GET")
        .header("cookie", &cookie_str)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let sessions: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    // At least one session must be marked as the current one
    let has_current = sessions
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["is_current"] == serde_json::Value::Bool(true));

    assert!(
        has_current,
        "Session list must contain an entry with is_current: true"
    );
}

// ============================================================================
// DELETE /auth/sessions/:id — revoke a session
// ============================================================================

#[tokio::test]
async fn test_revoke_session_without_auth_returns_401() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    let state = build_app_state(db);
    let app = create_router(state);

    let request = Request::builder()
        .uri("/auth/sessions/some-session-id")
        .method("DELETE")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "DELETE /auth/sessions/:id without auth must return 401"
    );
}

#[tokio::test]
async fn test_revoke_nonexistent_session_returns_404() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    create_test_user_with_role(&db, "revuser", "rev@example.com", "pw", "admin").await;

    let state = build_app_state(db.clone());
    let (login_status, _, cookie) = do_login(state.clone(), "revuser", "pw").await;
    assert_eq!(login_status, StatusCode::OK);
    let cookie_str = cookie.expect("Cookie must be present");

    // Attempt to revoke a non-existent session ID
    let app = create_router(state);
    let request = Request::builder()
        .uri("/auth/sessions/00000000-0000-0000-0000-000000000000")
        .method("DELETE")
        .header("cookie", &cookie_str)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Revoking a non-existent session must return 404"
    );
}

// ============================================================================
// Session-based authentication via direct session insertion
// ============================================================================

#[tokio::test]
async fn test_authenticated_request_via_session_cookie() {
    let db = create_test_db_with_seed().await;
    init_test_jwt_keys(&db).await;
    let admin = create_test_user_with_role(&db, "directauth", "da@example.com", "pw", "admin").await;

    // Create session directly in DB (bypasses login endpoint)
    let cookie = create_test_session(&db, admin.id).await;
    let state = build_app_state(db);

    // Use the session to access a protected endpoint that the admin has access to
    let app = create_router(state);
    let request = Request::builder()
        .uri("/auth/sessions")
        .method("GET")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Direct session cookie must grant access to /auth/sessions"
    );
}
