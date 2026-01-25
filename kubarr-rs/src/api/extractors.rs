use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use sqlx::SqlitePool;

use crate::db::User;
use crate::error::AppError;
use crate::services::security::decode_token;
use crate::state::AppState;

/// Extractor for authenticated users
pub struct AuthUser(pub User);

/// Extractor for admin users
pub struct AdminUser(pub User);

/// Extractor for optional authentication
pub struct OptionalUser(pub Option<User>);

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = extract_user_from_token(parts, &state.pool).await?;

        match user {
            Some(u) => Ok(AuthUser(u)),
            None => Err(AppError::Unauthorized("Authentication required".to_string())),
        }
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = extract_user_from_token(parts, &state.pool).await?;

        match user {
            Some(u) if u.is_admin => Ok(AdminUser(u)),
            Some(_) => Err(AppError::Forbidden("Admin access required".to_string())),
            None => Err(AppError::Unauthorized("Authentication required".to_string())),
        }
    }
}

#[async_trait]
impl FromRequestParts<AppState> for OptionalUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = extract_user_from_token(parts, &state.pool).await?;
        Ok(OptionalUser(user))
    }
}

/// Extract user from oauth2-proxy headers, Authorization header, or cookie
async fn extract_user_from_token(parts: &Parts, pool: &SqlitePool) -> Result<Option<User>, AppError> {
    // First, try oauth2-proxy headers (X-Auth-Request-User or X-Auth-Request-Email)
    // These are set by oauth2-proxy after successful authentication
    let email_from_proxy = parts
        .headers
        .get("X-Auth-Request-Email")
        .or_else(|| parts.headers.get("X-Auth-Request-User"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    if let Some(email) = email_from_proxy {
        tracing::debug!("Found oauth2-proxy header with email: {}", email);

        // Look up user by email
        let user: Option<User> = sqlx::query_as(
            "SELECT * FROM users WHERE email = ? AND is_active = 1 AND is_approved = 1"
        )
        .bind(&email)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        if user.is_some() {
            return Ok(user);
        }

        tracing::warn!("User not found for email from oauth2-proxy: {}", email);
    }

    // Fall back to Authorization header
    let token = if let Some(auth_header) = parts.headers.get(AUTHORIZATION) {
        let auth_str = auth_header
            .to_str()
            .map_err(|_| AppError::BadRequest("Invalid authorization header".to_string()))?;

        if let Some(token) = auth_str.strip_prefix("Bearer ") {
            Some(token.to_string())
        } else {
            None
        }
    } else {
        // Try cookie
        parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|c| c.to_str().ok())
            .and_then(|cookies| {
                cookies
                    .split(';')
                    .find_map(|cookie| {
                        let cookie = cookie.trim();
                        if let Some(value) = cookie.strip_prefix("access_token=") {
                            Some(value.to_string())
                        } else {
                            None
                        }
                    })
            })
    };

    let token = match token {
        Some(t) => t,
        None => return Ok(None),
    };

    // Decode and validate the token
    let claims = match decode_token(&token) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    // Check if it's a refresh token (not allowed for API access)
    if claims.token_type.as_deref() == Some("refresh") {
        return Ok(None);
    }

    // Fetch user from database
    let user: Option<User> = sqlx::query_as(
        "SELECT * FROM users WHERE id = ? AND is_active = 1"
    )
    .bind(claims.sub.parse::<i64>().unwrap_or(0))
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    Ok(user)
}
