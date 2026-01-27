use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

use crate::endpoints::extractors::{get_user_app_access, get_user_permissions};
use crate::middleware::auth::SESSION_COOKIE_NAME;
use crate::config::CONFIG;
use crate::models::prelude::*;
use crate::models::user;
use crate::error::{AppError, Result};
use crate::services::{create_access_token, verify_password, verify_totp};
use crate::state::AppState;

/// Create auth routes for session management
pub fn auth_routes(state: AppState) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/refresh", post(refresh_session))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub totp_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user_id: i64,
    pub username: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: Option<String>,
}

// ============================================================================
// Session Cookie Helpers
// ============================================================================

/// Create a session cookie with the given token
fn create_session_cookie(token: &str, secure: bool) -> HeaderValue {
    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=604800{}",
        SESSION_COOKIE_NAME,
        token,
        if secure { "; Secure" } else { "" }
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// Create a cookie that clears the session
fn clear_session_cookie() -> HeaderValue {
    let cookie = format!(
        "{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        SESSION_COOKIE_NAME
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

// ============================================================================
// Session Management Endpoints
// ============================================================================

/// Login with username and password, returns session cookie
async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Response> {
    // Find user by username or email
    let found_user = User::find()
        .filter(
            user::Column::Username
                .eq(&request.username)
                .or(user::Column::Email.eq(&request.username)),
        )
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid credentials".to_string()))?;

    // Check if user is active and approved
    if !found_user.is_active {
        return Err(AppError::Unauthorized("Account is disabled".to_string()));
    }
    if !found_user.is_approved {
        return Err(AppError::Unauthorized(
            "Account is pending approval".to_string(),
        ));
    }

    // Verify password
    if !verify_password(&request.password, &found_user.hashed_password) {
        return Err(AppError::Unauthorized("Invalid credentials".to_string()));
    }

    // Check TOTP if enabled
    if found_user.totp_enabled {
        let totp_code = request.totp_code.as_ref().ok_or_else(|| {
            AppError::BadRequest("Two-factor authentication code required".to_string())
        })?;

        let totp_secret = found_user.totp_secret.as_ref().ok_or_else(|| {
            AppError::Internal("TOTP enabled but no secret configured".to_string())
        })?;

        if !verify_totp(totp_secret, totp_code, &found_user.email)? {
            return Err(AppError::Unauthorized("Invalid TOTP code".to_string()));
        }
    }

    // Get user permissions for token
    let permissions = get_user_permissions(&state.db, found_user.id).await;
    let allowed_apps = get_user_app_access(&state.db, found_user.id).await;

    // Create access token (7 days for session)
    let access_token = create_access_token(
        &found_user.id.to_string(),
        Some(&found_user.email),
        None,
        None,
        Some(604800), // 7 days
        Some(permissions),
        Some(allowed_apps),
    )?;

    // Build response with cookie
    let response = Json(LoginResponse {
        user_id: found_user.id,
        username: found_user.username,
        email: found_user.email,
    });

    // Determine if we should set Secure flag (check if running behind HTTPS)
    let secure = CONFIG.oauth2_issuer_url.starts_with("https://");

    Ok((
        [(header::SET_COOKIE, create_session_cookie(&access_token, secure))],
        response,
    )
        .into_response())
}

/// Logout - clears the session cookie
async fn logout() -> Response {
    ([(header::SET_COOKIE, clear_session_cookie())], Json(serde_json::json!({"message": "Logged out"}))).into_response()
}

/// Refresh the session token
async fn refresh_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RefreshRequest>,
) -> Result<Response> {
    use crate::services::security::decode_token;

    // Get current token from cookie or request body
    let current_token = extract_session_token(&headers)
        .or(request.refresh_token)
        .ok_or_else(|| AppError::Unauthorized("No session to refresh".to_string()))?;

    // Decode and validate current token
    let claims = decode_token(&current_token)
        .map_err(|_| AppError::Unauthorized("Invalid or expired session".to_string()))?;

    // Don't allow refresh tokens to be used for session refresh
    if claims.token_type.as_deref() == Some("refresh") {
        return Err(AppError::BadRequest(
            "Cannot use refresh token for session refresh".to_string(),
        ));
    }

    // Get user ID from token
    let user_id: i64 = claims
        .sub
        .parse()
        .map_err(|_| AppError::Unauthorized("Invalid session".to_string()))?;

    // Verify user still exists and is active
    let found_user = User::find_by_id(user_id)
        .filter(user::Column::IsActive.eq(true))
        .filter(user::Column::IsApproved.eq(true))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found or inactive".to_string()))?;

    // Get fresh permissions
    let permissions = get_user_permissions(&state.db, found_user.id).await;
    let allowed_apps = get_user_app_access(&state.db, found_user.id).await;

    // Create new access token
    let new_token = create_access_token(
        &found_user.id.to_string(),
        Some(&found_user.email),
        None,
        None,
        Some(604800), // 7 days
        Some(permissions),
        Some(allowed_apps),
    )?;

    let secure = CONFIG.oauth2_issuer_url.starts_with("https://");

    Ok((
        [(header::SET_COOKIE, create_session_cookie(&new_token, secure))],
        Json(serde_json::json!({"message": "Session refreshed"})),
    )
        .into_response())
}

/// Extract session token from cookie header
fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?;
    let cookie_str = cookies.to_str().ok()?;

    for cookie in cookie_str.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(&format!("{}=", SESSION_COOKIE_NAME)) {
            return Some(value.to_string());
        }
    }
    None
}
