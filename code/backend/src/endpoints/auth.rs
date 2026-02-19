use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::config::CONFIG;
use crate::error::{AppError, Result};
use crate::middleware::auth::{
    ACTIVE_SESSION_COOKIE, MAX_SESSIONS, SESSION_COOKIE_BASE, SESSION_COOKIE_NAME,
};
use crate::models::prelude::*;
use crate::models::{role, session, two_factor_recovery_code, user, user_role};
use crate::services::{
    create_session_token, decode_session_token, verify_password, verify_recovery_code, verify_totp,
};
use crate::state::AppState;

/// Create auth routes for session management
pub fn auth_routes(state: AppState) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{session_id}", delete(revoke_session))
        .route("/switch/{slot}", post(switch_session))
        .route("/accounts", get(list_accounts))
        .route("/2fa/recover", post(recover_with_code))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub totp_code: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    pub user_id: i64,
    pub username: String,
    pub email: String,
    pub session_slot: usize,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AccountInfo {
    pub slot: usize,
    pub user_id: i64,
    pub username: String,
    pub email: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SessionInfo {
    pub id: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: String,
    pub last_accessed_at: String,
    pub is_current: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RecoverRequest {
    pub username: String,
    pub password: String,
    pub recovery_code: String,
}

// ============================================================================
// Session Cookie Helpers
// ============================================================================

/// Create an indexed session cookie with the given token
fn create_session_cookie_for_slot(slot: usize, token: &str, secure: bool) -> HeaderValue {
    let cookie = format!(
        "{}_{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=604800{}",
        SESSION_COOKIE_BASE,
        slot,
        token,
        if secure { "; Secure" } else { "" }
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// Create the active session cookie
fn create_active_session_cookie(slot: usize, secure: bool) -> HeaderValue {
    let cookie = format!(
        "{}={}; SameSite=Lax; Path=/; Max-Age=604800{}",
        ACTIVE_SESSION_COOKIE,
        slot,
        if secure { "; Secure" } else { "" }
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// Create a cookie that clears an indexed session
#[allow(dead_code)]
fn clear_session_cookie_for_slot(slot: usize) -> HeaderValue {
    let cookie = format!(
        "{}_{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        SESSION_COOKIE_BASE, slot
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// Legacy: Create a session cookie with the given token (for backwards compatibility)
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

/// Parse existing session cookies from headers to find used slots and their user IDs
async fn get_existing_sessions(state: &AppState, headers: &HeaderMap) -> Vec<(usize, i64, String)> {
    let mut sessions = Vec::new();

    let db = match state.get_db().await {
        Ok(db) => db,
        Err(_) => return sessions,
    };

    let cookies = match headers.get(header::COOKIE) {
        Some(c) => c,
        None => return sessions,
    };
    let cookie_str = match cookies.to_str() {
        Ok(s) => s,
        Err(_) => return sessions,
    };

    for i in 0..MAX_SESSIONS {
        let prefix = format!("{}_{}=", SESSION_COOKIE_BASE, i);
        for cookie in cookie_str.split(';') {
            let cookie = cookie.trim();
            if let Some(token) = cookie.strip_prefix(&prefix) {
                // Decode token to get session ID, then look up user
                if let Ok(claims) = decode_session_token(token) {
                    if let Ok(Some(session)) = Session::find_by_id(&claims.sid).one(&db).await {
                        if !session.is_revoked && session.expires_at > Utc::now() {
                            if let Ok(Some(user)) = User::find_by_id(session.user_id).one(&db).await
                            {
                                sessions.push((i, user.id, user.username.clone()));
                            }
                        }
                    }
                }
                break;
            }
        }
    }

    sessions
}

/// Find the next available session slot
fn find_available_slot(existing: &[(usize, i64, String)], user_id: i64) -> usize {
    // If user already has a session, return that slot
    for (slot, uid, _) in existing {
        if *uid == user_id {
            return *slot;
        }
    }
    // Find first unused slot
    let used_slots: std::collections::HashSet<usize> =
        existing.iter().map(|(s, _, _)| *s).collect();
    for i in 0..MAX_SESSIONS {
        if !used_slots.contains(&i) {
            return i;
        }
    }
    // All slots used, reuse slot 0
    0
}

// ============================================================================
// Session Management Endpoints
// ============================================================================

/// Check if any of a user's roles require 2FA
async fn user_role_requires_2fa(db: &sea_orm::DatabaseConnection, user_id: i64) -> bool {
    let roles: Vec<role::Model> = Role::find()
        .inner_join(UserRole)
        .filter(user_role::Column::UserId.eq(user_id))
        .all(db)
        .await
        .unwrap_or_default();
    roles.iter().any(|r| r.requires_2fa)
}

/// Login with username and password, returns session cookie
#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, body = LoginResponse)
    )
)]
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response> {
    let db = state.get_db().await?;

    // Find user by username or email
    let found_user = User::find()
        .filter(
            user::Column::Username
                .eq(&request.username)
                .or(user::Column::Email.eq(&request.username)),
        )
        .one(&db)
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

    // If role requires 2FA but user hasn't set it up, block login
    if !found_user.totp_enabled && user_role_requires_2fa(&db, found_user.id).await {
        return Err(AppError::BadRequest(
            "Two-factor authentication setup required".to_string(),
        ));
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

    // Create session record in database
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    let expires_at = now + Duration::days(7);

    // Extract user agent and IP from headers
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.chars().take(255).collect::<String>());

    let ip_address = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());

    let session = session::ActiveModel {
        id: Set(session_id.clone()),
        user_id: Set(found_user.id),
        user_agent: Set(user_agent),
        ip_address: Set(ip_address),
        created_at: Set(now),
        expires_at: Set(expires_at),
        last_accessed_at: Set(now),
        is_revoked: Set(false),
    };
    session.insert(&db).await?;

    // Create minimal session token (JWT containing only session ID)
    let session_token = create_session_token(&session_id)?;

    // Find available slot for this session
    let existing_sessions = get_existing_sessions(&state, &headers).await;
    let slot = find_available_slot(&existing_sessions, found_user.id);

    // Build response with cookie
    let response = Json(LoginResponse {
        user_id: found_user.id,
        username: found_user.username.clone(),
        email: found_user.email.clone(),
        session_slot: slot,
    });

    // Determine if we should set Secure flag (check if running behind HTTPS)
    let secure = CONFIG.auth.oauth2_issuer_url.starts_with("https://");

    tracing::info!(
        user_id = found_user.id,
        username = found_user.username,
        slot = slot,
        "User logged in, session created in slot {}",
        slot
    );

    // Set both the indexed session cookie and the active session cookie
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        create_session_cookie_for_slot(slot, &session_token, secure),
    );
    response_headers.append(
        header::SET_COOKIE,
        create_active_session_cookie(slot, secure),
    );
    // Also set legacy cookie for backwards compatibility
    response_headers.append(
        header::SET_COOKIE,
        create_session_cookie(&session_token, secure),
    );

    Ok((response_headers, response).into_response())
}

/// Logout - revokes the session and clears the cookie
#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "Auth",
    responses(
        (status = 200, body = serde_json::Value)
    )
)]
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    // Try to get and revoke the current session
    if let Some(token) = extract_session_token(&headers) {
        if let Ok(claims) = decode_session_token(&token) {
            // Revoke the session in the database
            if let Ok(db) = state.get_db().await {
                let _ = Session::delete_by_id(&claims.sid).exec(&db).await;
                tracing::info!(session_id = claims.sid, "Session revoked on logout");
            }
        }
    }

    Ok((
        [(header::SET_COOKIE, clear_session_cookie())],
        Json(serde_json::json!({"message": "Logged out"})),
    )
        .into_response())
}

/// List all active sessions for the current user
#[utoipa::path(
    get,
    path = "/auth/sessions",
    tag = "Auth",
    responses(
        (status = 200, body = Vec<SessionInfo>)
    )
)]
async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SessionInfo>>> {
    let db = state.get_db().await?;

    // Get current session from cookie
    let token = extract_session_token(&headers)
        .ok_or_else(|| AppError::Unauthorized("Not authenticated".to_string()))?;

    let claims = decode_session_token(&token)
        .map_err(|_| AppError::Unauthorized("Invalid or expired session".to_string()))?;

    // Get the current session to find user_id
    let current_session = Session::find_by_id(&claims.sid)
        .one(&db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Session not found".to_string()))?;

    // Get all active sessions for this user
    let sessions = Session::find()
        .filter(session::Column::UserId.eq(current_session.user_id))
        .filter(session::Column::IsRevoked.eq(false))
        .filter(session::Column::ExpiresAt.gt(Utc::now()))
        .all(&db)
        .await?;

    let session_infos: Vec<SessionInfo> = sessions
        .into_iter()
        .map(|s| SessionInfo {
            id: s.id.clone(),
            user_agent: s.user_agent,
            ip_address: s.ip_address,
            created_at: s.created_at.to_rfc3339(),
            last_accessed_at: s.last_accessed_at.to_rfc3339(),
            is_current: s.id == claims.sid,
        })
        .collect();

    Ok(Json(session_infos))
}

/// Revoke a specific session (must belong to current user)
#[utoipa::path(
    delete,
    path = "/auth/sessions/{session_id}",
    tag = "Auth",
    params(
        ("session_id" = String, Path, description = "Session ID to revoke")
    ),
    responses(
        (status = 200, body = serde_json::Value)
    )
)]
async fn revoke_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let db = state.get_db().await?;

    // Get current session from cookie
    let token = extract_session_token(&headers)
        .ok_or_else(|| AppError::Unauthorized("Not authenticated".to_string()))?;

    let claims = decode_session_token(&token)
        .map_err(|_| AppError::Unauthorized("Invalid or expired session".to_string()))?;

    // Get the current session to find user_id
    let current_session = Session::find_by_id(&claims.sid)
        .one(&db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Session not found".to_string()))?;

    // Find the session to revoke
    let target_session = Session::find_by_id(&session_id)
        .one(&db)
        .await?
        .ok_or_else(|| AppError::NotFound("Session not found".to_string()))?;

    // Verify it belongs to the same user
    if target_session.user_id != current_session.user_id {
        return Err(AppError::Forbidden(
            "Cannot revoke another user's session".to_string(),
        ));
    }

    // Don't allow revoking current session (use logout instead)
    if target_session.id == claims.sid {
        return Err(AppError::BadRequest(
            "Cannot revoke current session. Use logout instead.".to_string(),
        ));
    }

    // Delete the session
    Session::delete_by_id(&session_id).exec(&db).await?;

    tracing::info!(session_id = session_id, "Session revoked by user");

    Ok(Json(serde_json::json!({"message": "Session revoked"})))
}

/// Extract session token from cookie header
fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?;
    let cookie_str = cookies.to_str().ok()?;

    // First try to find active slot
    let mut active_slot: Option<usize> = None;
    for cookie in cookie_str.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(&format!("{}=", ACTIVE_SESSION_COOKIE)) {
            active_slot = value.parse().ok();
            break;
        }
    }

    // Look for indexed session cookie
    if let Some(slot) = active_slot {
        let prefix = format!("{}_{}=", SESSION_COOKIE_BASE, slot);
        for cookie in cookie_str.split(';') {
            let cookie = cookie.trim();
            if let Some(value) = cookie.strip_prefix(&prefix) {
                return Some(value.to_string());
            }
        }
    }

    // Fallback to legacy cookie
    for cookie in cookie_str.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(&format!("{}=", SESSION_COOKIE_NAME)) {
            return Some(value.to_string());
        }
    }
    None
}

/// Switch to a different session slot
#[utoipa::path(
    post,
    path = "/auth/sessions/{session_id}/switch",
    tag = "Auth",
    params(
        ("session_id" = usize, Path, description = "Session slot to switch to")
    ),
    responses(
        (status = 200, body = LoginResponse)
    )
)]
async fn switch_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slot): Path<usize>,
) -> Result<Response> {
    if slot >= MAX_SESSIONS {
        return Err(AppError::BadRequest(format!(
            "Invalid session slot. Max is {}",
            MAX_SESSIONS - 1
        )));
    }

    // Verify that the requested slot has a valid session
    let existing_sessions = get_existing_sessions(&state, &headers).await;
    let slot_exists = existing_sessions.iter().any(|(s, _, _)| *s == slot);

    if !slot_exists {
        return Err(AppError::NotFound("No session in that slot".to_string()));
    }

    let secure = CONFIG.auth.oauth2_issuer_url.starts_with("https://");

    tracing::info!(slot = slot, "User switched to session slot {}", slot);

    Ok((
        [(
            header::SET_COOKIE,
            create_active_session_cookie(slot, secure),
        )],
        Json(serde_json::json!({"message": "Switched session", "slot": slot})),
    )
        .into_response())
}

/// List all signed-in accounts
#[utoipa::path(
    get,
    path = "/auth/accounts",
    tag = "Auth",
    responses(
        (status = 200, body = Vec<AccountInfo>)
    )
)]
async fn list_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AccountInfo>>> {
    let db = state.get_db().await?;
    let existing_sessions = get_existing_sessions(&state, &headers).await;

    // Get active slot
    let cookies = headers.get(header::COOKIE);
    let active_slot: usize = cookies
        .and_then(|c| c.to_str().ok())
        .and_then(|s| {
            for cookie in s.split(';') {
                let cookie = cookie.trim();
                if let Some(value) = cookie.strip_prefix(&format!("{}=", ACTIVE_SESSION_COOKIE)) {
                    return value.parse().ok();
                }
            }
            None
        })
        .unwrap_or(0);

    let mut accounts = Vec::new();
    for (slot, user_id, username) in &existing_sessions {
        // Get full user info
        if let Ok(Some(user)) = User::find_by_id(*user_id).one(&db).await {
            accounts.push(AccountInfo {
                slot: *slot,
                user_id: *user_id,
                username: username.clone(),
                email: user.email,
                is_active: *slot == active_slot,
            });
        }
    }

    Ok(Json(accounts))
}

/// Login using a recovery code instead of a TOTP code
#[utoipa::path(
    post,
    path = "/auth/2fa/recover",
    tag = "Auth",
    request_body = RecoverRequest,
    responses(
        (status = 200, body = LoginResponse)
    )
)]
async fn recover_with_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RecoverRequest>,
) -> Result<Response> {
    let db = state.get_db().await?;

    // Find user by username or email
    let found_user = User::find()
        .filter(
            user::Column::Username
                .eq(&request.username)
                .or(user::Column::Email.eq(&request.username)),
        )
        .one(&db)
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

    // Only allow recovery code login if 2FA is enabled
    if !found_user.totp_enabled {
        return Err(AppError::BadRequest(
            "Two-factor authentication is not enabled on this account".to_string(),
        ));
    }

    // Find all unused recovery codes for this user
    let recovery_codes = TwoFactorRecoveryCode::find()
        .filter(two_factor_recovery_code::Column::UserId.eq(found_user.id))
        .filter(two_factor_recovery_code::Column::UsedAt.is_null())
        .all(&db)
        .await?;

    if recovery_codes.is_empty() {
        return Err(AppError::BadRequest(
            "No recovery codes available. Please contact your administrator.".to_string(),
        ));
    }

    // Find the matching recovery code
    let normalized_code = request.recovery_code.to_uppercase();
    let matched_code = recovery_codes
        .iter()
        .find(|rc| verify_recovery_code(&normalized_code, &rc.code_hash));

    let matched_code = matched_code.ok_or_else(|| {
        AppError::Unauthorized("Invalid recovery code".to_string())
    })?;

    // Mark the code as used
    let now = Utc::now();
    let mut code_model: two_factor_recovery_code::ActiveModel = matched_code.clone().into();
    code_model.used_at = Set(Some(now));
    code_model.update(&db).await?;

    // Count remaining unused codes; if none left, disable 2FA
    let remaining = TwoFactorRecoveryCode::find()
        .filter(two_factor_recovery_code::Column::UserId.eq(found_user.id))
        .filter(two_factor_recovery_code::Column::UsedAt.is_null())
        .count(&db)
        .await?;

    if remaining == 0 {
        // All codes used: disable 2FA so user can re-enroll
        let mut user_model: user::ActiveModel = found_user.clone().into();
        user_model.totp_enabled = Set(false);
        user_model.totp_secret = Set(None);
        user_model.totp_verified_at = Set(None);
        user_model.updated_at = Set(now);
        user_model.update(&db).await?;

        tracing::warn!(
            user_id = found_user.id,
            "All 2FA recovery codes used - 2FA disabled, user must re-enroll"
        );
    }

    // Create session record in database
    let session_id = uuid::Uuid::new_v4().to_string();
    let expires_at = now + Duration::days(7);

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.chars().take(255).collect::<String>());

    let ip_address = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());

    let session = session::ActiveModel {
        id: Set(session_id.clone()),
        user_id: Set(found_user.id),
        user_agent: Set(user_agent),
        ip_address: Set(ip_address),
        created_at: Set(now),
        expires_at: Set(expires_at),
        last_accessed_at: Set(now),
        is_revoked: Set(false),
    };
    session.insert(&db).await?;

    let session_token = create_session_token(&session_id)?;

    let existing_sessions = get_existing_sessions(&state, &headers).await;
    let slot = find_available_slot(&existing_sessions, found_user.id);

    let response = Json(LoginResponse {
        user_id: found_user.id,
        username: found_user.username.clone(),
        email: found_user.email.clone(),
        session_slot: slot,
    });

    let secure = CONFIG.auth.oauth2_issuer_url.starts_with("https://");

    tracing::info!(
        user_id = found_user.id,
        username = found_user.username,
        "User logged in via recovery code"
    );

    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        create_session_cookie_for_slot(slot, &session_token, secure),
    );
    response_headers.append(
        header::SET_COOKIE,
        create_active_session_cookie(slot, secure),
    );
    response_headers.append(
        header::SET_COOKIE,
        create_session_cookie(&session_token, secure),
    );

    Ok((response_headers, response).into_response())
}
