//! Authentication middleware for API routes
//!
//! Requires valid Bearer token for all endpoints except `/auth/*`.

use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::models::prelude::*;
use crate::models::user;
use crate::services::security::decode_token;
use crate::state::AppState;

/// Authenticated user stored in request extensions
#[derive(Clone)]
pub struct AuthenticatedUser(pub user::Model);

/// Auth middleware that validates Bearer tokens
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

    // Validate token and get user
    let user = match validate_token_and_get_user(&state, &token).await {
        Ok(u) => u,
        Err(msg) => {
            return unauthorized_response(&msg);
        }
    };

    // Add authenticated user to request extensions
    req.extensions_mut().insert(AuthenticatedUser(user));

    next.run(req).await
}

/// Extract Bearer token from Authorization header
fn extract_bearer_token(req: &Request) -> Option<String> {
    let auth_header = req.headers().get(AUTHORIZATION)?;
    let auth_str = auth_header.to_str().ok()?;
    let token = auth_str.strip_prefix("Bearer ")?;
    Some(token.to_string())
}

/// Validate JWT token and fetch user from database
async fn validate_token_and_get_user(state: &AppState, token: &str) -> Result<user::Model, String> {
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
    let found_user = User::find_by_id(user_id)
        .filter(user::Column::IsActive.eq(true))
        .filter(user::Column::IsApproved.eq(true))
        .one(&state.db)
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    found_user.ok_or_else(|| "User not found or inactive".to_string())
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
