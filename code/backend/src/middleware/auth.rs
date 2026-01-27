//! Authentication middleware for API routes
//!
//! Requires valid Bearer token for all endpoints except `/auth/*`.
//! Fetches user permissions once and stores them in request extensions.

use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::models::prelude::*;
use crate::models::{role_app_permission, role_permission, user, user_role};
use crate::services::security::decode_token;
use crate::state::AppState;

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

/// Auth middleware that validates Bearer tokens and fetches permissions
///
/// Skips authentication for `/auth/*` routes.
/// Returns 401 Unauthorized if token is missing or invalid.
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

    // Extract Bearer token from Authorization header
    let token = match extract_bearer_token(&req) {
        Some(t) => t,
        None => {
            return unauthorized_response("Missing or invalid Authorization header");
        }
    };

    // Validate token and get user with permissions
    let auth_user = match authenticate_user(&state, &token).await {
        Ok(u) => u,
        Err(msg) => {
            return unauthorized_response(&msg);
        }
    };

    // Add authenticated user to request extensions
    req.extensions_mut().insert(auth_user);

    next.run(req).await
}

/// Extract Bearer token from Authorization header
fn extract_bearer_token(req: &Request) -> Option<String> {
    let auth_header = req.headers().get(AUTHORIZATION)?;
    let auth_str = auth_header.to_str().ok()?;
    let token = auth_str.strip_prefix("Bearer ")?;
    Some(token.to_string())
}

/// Validate JWT token, fetch user and their permissions
async fn authenticate_user(state: &AppState, token: &str) -> Result<AuthenticatedUser, String> {
    // Decode and validate the token
    let claims = decode_token(token).map_err(|_| "Invalid or expired token".to_string())?;

    // Check if it's a refresh token (not allowed for API access)
    if claims.token_type.as_deref() == Some("refresh") {
        return Err("Refresh tokens cannot be used for API access".to_string());
    }

    // Parse user ID from subject
    let user_id: i64 = claims
        .sub
        .parse()
        .map_err(|_| "Invalid token subject".to_string())?;

    // Fetch user from database
    let user = User::find_by_id(user_id)
        .filter(user::Column::IsActive.eq(true))
        .filter(user::Column::IsApproved.eq(true))
        .one(&state.db)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "User not found or inactive".to_string())?;

    // Fetch all permissions for this user from their roles
    let permissions = fetch_user_permissions(state, user_id).await;

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
