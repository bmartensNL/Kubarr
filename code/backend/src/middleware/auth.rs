//! Authentication middleware for API routes
//!
//! Requires valid session cookie for all endpoints except `/auth/*`.
//! Session tokens contain only a session ID - user data is looked up from the database.

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::models::prelude::*;
use crate::models::{role_app_permission, role_permission, session, user, user_role};
use crate::services::security::decode_session_token;
use crate::state::AppState;

/// Cookie name for session token
pub const SESSION_COOKIE_NAME: &str = "kubarr_session";

/// Authenticated user with permissions, stored in request extensions
#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub user: user::Model,
    pub permissions: Vec<String>,
}

impl AuthenticatedUser {
    /// Check if user has a specific permission
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(&permission.to_string())
    }

    /// Check if user has access to a specific app
    pub fn has_app_access(&self, app_name: &str) -> bool {
        // Check for app.* wildcard or specific app permission
        self.permissions.contains(&"app.*".to_string())
            || self.permissions.contains(&format!("app.{}", app_name))
    }
}

/// Auth middleware that validates session cookies and fetches permissions
///
/// Skips authentication for `/auth/*` routes.
/// Returns 401 Unauthorized if session is missing or invalid.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();

    // Skip authentication for /auth/* routes
    if path.starts_with("/auth") {
        return next.run(req).await;
    }

    // Extract session token from cookie
    let token = match extract_token(&req) {
        Some(t) => t,
        None => {
            return unauthorized_response("Missing or invalid session");
        }
    };

    // Validate session and get user with permissions
    let auth_user = match authenticate_session(&state, &token).await {
        Ok(u) => u,
        Err(msg) => {
            return unauthorized_response(&msg);
        }
    };

    // Add authenticated user to request extensions
    req.extensions_mut().insert(auth_user);

    next.run(req).await
}

/// Extract session token from cookie
fn extract_token(req: &Request) -> Option<String> {
    let cookies = req.headers().get(header::COOKIE)?;
    let cookie_str = cookies.to_str().ok()?;

    for cookie in cookie_str.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(&format!("{}=", SESSION_COOKIE_NAME)) {
            return Some(value.to_string());
        }
    }
    None
}

/// Authenticate using session token (from cookie)
/// Validates the signed JWT, looks up session in database, and updates last_accessed_at
async fn authenticate_session(state: &AppState, token: &str) -> Result<AuthenticatedUser, String> {
    // Decode and validate the session token
    let claims =
        decode_session_token(token).map_err(|_| "Invalid or expired session".to_string())?;

    // Look up session in database
    let session = Session::find_by_id(&claims.sid)
        .one(&state.db)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Session not found".to_string())?;

    // Check if session is revoked
    if session.is_revoked {
        return Err("Session has been revoked".to_string());
    }

    // Check if session is expired (double-check against DB value)
    if session.expires_at < Utc::now() {
        return Err("Session has expired".to_string());
    }

    // Fetch user from database
    let user = User::find_by_id(session.user_id)
        .filter(user::Column::IsActive.eq(true))
        .filter(user::Column::IsApproved.eq(true))
        .one(&state.db)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "User not found or inactive".to_string())?;

    // Update last_accessed_at (fire and forget - don't block on this)
    let session_id = session.id.clone();
    let db = state.db.clone();
    tokio::spawn(async move {
        let update = session::ActiveModel {
            id: Set(session_id),
            last_accessed_at: Set(Utc::now()),
            ..Default::default()
        };
        let _ = update.update(&db).await;
    });

    // Fetch all permissions for this user from their roles
    let permissions = fetch_user_permissions(state, session.user_id).await;

    Ok(AuthenticatedUser { user, permissions })
}

/// Fetch all permissions for a user from their roles
async fn fetch_user_permissions(state: &AppState, user_id: i64) -> Vec<String> {
    // Get all role IDs for this user
    let user_roles = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .all(&state.db)
        .await
        .unwrap_or_default();

    let role_ids: Vec<i64> = user_roles.iter().map(|ur| ur.role_id).collect();

    if role_ids.is_empty() {
        return Vec::new();
    }

    // Get all permissions from all roles
    let permissions = RolePermission::find()
        .filter(role_permission::Column::RoleId.is_in(role_ids.clone()))
        .all(&state.db)
        .await
        .unwrap_or_default();

    let mut perms: Vec<String> = permissions.iter().map(|p| p.permission.clone()).collect();

    // Get app permissions and convert to app.{name} format
    let app_permissions = RoleAppPermission::find()
        .filter(role_app_permission::Column::RoleId.is_in(role_ids))
        .all(&state.db)
        .await
        .unwrap_or_default();

    for app_perm in app_permissions {
        perms.push(format!("app.{}", app_perm.app_name));
    }

    // Deduplicate
    perms.sort();
    perms.dedup();
    perms
}

/// Create a 401 Unauthorized JSON response
fn unauthorized_response(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "detail": message
        })),
    )
        .into_response()
}
