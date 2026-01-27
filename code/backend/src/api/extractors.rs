use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait, Set};

use crate::models::prelude::*;
use crate::models::{role, role_app_permission, role_permission, user, user_role};
use crate::error::AppError;
use crate::services::security::{decode_token, generate_random_string, hash_password};
use crate::state::{AppState, DbConn};

/// Extractor for authenticated users
pub struct AuthUser(pub user::Model);

/// Extractor for admin users
pub struct AdminUser(pub user::Model);

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = extract_user_from_token(parts, &state.db).await?;

        match user {
            Some(u) => Ok(AuthUser(u)),
            None => Err(AppError::Unauthorized(
                "Authentication required".to_string(),
            )),
        }
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = extract_user_from_token(parts, &state.db).await?;

        match user {
            Some(u) => {
                // Check if user has admin role
                let has_admin_role = UserRole::find()
                    .filter(user_role::Column::UserId.eq(u.id))
                    .join(JoinType::InnerJoin, user_role::Relation::Role.def())
                    .filter(role::Column::Name.eq("admin"))
                    .one(&state.db)
                    .await
                    .map(|r| r.is_some())
                    .unwrap_or(false);

                if has_admin_role {
                    Ok(AdminUser(u))
                } else {
                    Err(AppError::Forbidden("Admin access required".to_string()))
                }
            }
            None => Err(AppError::Unauthorized(
                "Authentication required".to_string(),
            )),
        }
    }
}

/// Extract user from auth proxy headers, Authorization header, or cookie
async fn extract_user_from_token(
    parts: &Parts,
    db: &DbConn,
) -> Result<Option<user::Model>, AppError> {
    // First, try oauth2-proxy headers (X-Auth-Request-Email, X-Auth-Request-User)
    let email_from_proxy = parts
        .headers
        .get("X-Auth-Request-Email")
        .or_else(|| parts.headers.get("X-Auth-Request-User"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(ref email) = email_from_proxy {
        tracing::debug!("Found auth proxy header with email: {}", email);

        // Look up user by email
        let found_user = User::find()
            .filter(user::Column::Email.eq(email.as_str()))
            .one(db)
            .await
            .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        if let Some(existing_user) = found_user {
            // User exists - check if active and approved
            if existing_user.is_active && existing_user.is_approved {
                return Ok(Some(existing_user));
            }
            // User exists but is inactive or not approved
            tracing::warn!("User {} exists but is inactive or not approved", email);
            return Ok(None);
        }

        // User doesn't exist - auto-create from oauth2-proxy authentication
        tracing::info!("Auto-creating user from oauth2-proxy: {}", email);

        // Generate username from email
        let username = email
            .split('@')
            .next()
            .unwrap_or("user")
            .to_lowercase()
            .replace('.', "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();

        // Make username unique if needed
        let mut final_username = username.clone();
        let mut counter = 1;
        while User::find()
            .filter(user::Column::Username.eq(&final_username))
            .one(db)
            .await
            .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?
            .is_some()
        {
            final_username = format!("{}_{}", username, counter);
            counter += 1;
        }

        // Create user with random password (they'll use oauth2-proxy to login)
        let random_password = generate_random_string(32);
        let password_hash = hash_password(&random_password)
            .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))?;

        let now = Utc::now();
        let new_user = user::ActiveModel {
            username: Set(final_username),
            email: Set(email.clone()),
            hashed_password: Set(password_hash),
            is_active: Set(true),
            is_approved: Set(true), // Auto-approve users from oauth2-proxy
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        let created_user = new_user
            .insert(db)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create user: {}", e)))?;

        tracing::info!("Created user {} from oauth2-proxy", created_user.email);
        return Ok(Some(created_user));
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
        // Try cookie (kubarr_session or access_token)
        parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|c| c.to_str().ok())
            .and_then(|cookies| {
                cookies.split(';').find_map(|cookie| {
                    let cookie = cookie.trim();
                    if let Some(value) = cookie.strip_prefix("kubarr_session=") {
                        Some(value.to_string())
                    } else if let Some(value) = cookie.strip_prefix("access_token=") {
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
    let user_id = claims.sub.parse::<i64>().unwrap_or(0);
    let found_user = User::find_by_id(user_id)
        .filter(user::Column::IsActive.eq(true))
        .one(db)
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    Ok(found_user)
}

/// Check if user has a specific permission
pub async fn user_has_permission(db: &DbConn, user_id: i64, permission: &str) -> bool {
    // First check if user has admin role (admin bypasses all permission checks)
    let has_admin_role = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .join(JoinType::InnerJoin, user_role::Relation::Role.def())
        .filter(role::Column::Name.eq("admin"))
        .one(db)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false);

    if has_admin_role {
        return true;
    }

    // Check role_permissions via user_roles join
    // Get all role IDs for this user
    let user_roles = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .all(db)
        .await
        .unwrap_or_default();

    let role_ids: Vec<i64> = user_roles.iter().map(|ur| ur.role_id).collect();

    if role_ids.is_empty() {
        return false;
    }

    // Check if any of the user's roles have the permission
    RolePermission::find()
        .filter(role_permission::Column::RoleId.is_in(role_ids))
        .filter(role_permission::Column::Permission.eq(permission))
        .one(db)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false)
}

/// Check if user has access to a specific app
#[allow(dead_code)]
pub async fn user_has_app_access(db: &DbConn, user_id: i64, app_name: &str) -> bool {
    // First check if user has admin role (admin has access to all apps)
    let has_admin_role = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .join(JoinType::InnerJoin, user_role::Relation::Role.def())
        .filter(role::Column::Name.eq("admin"))
        .one(db)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false);

    if has_admin_role {
        return true;
    }

    // Get all role IDs for this user
    let user_roles = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .all(db)
        .await
        .unwrap_or_default();

    let role_ids: Vec<i64> = user_roles.iter().map(|ur| ur.role_id).collect();

    if role_ids.is_empty() {
        return false;
    }

    // Check if any of the user's roles have access to this app
    RoleAppPermission::find()
        .filter(role_app_permission::Column::RoleId.is_in(role_ids))
        .filter(role_app_permission::Column::AppName.eq(app_name))
        .one(db)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false)
}

/// Get all permissions for a user (from all their roles)
/// Includes app.* permissions based on role_app_permissions
pub async fn get_user_permissions(db: &DbConn, user_id: i64) -> Vec<String> {
    // First check if user has admin role
    let has_admin_role = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .join(JoinType::InnerJoin, user_role::Relation::Role.def())
        .filter(role::Column::Name.eq("admin"))
        .one(db)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false);

    if has_admin_role {
        // Return all permissions for admin (including app.* wildcard)
        return vec![
            "apps.view".to_string(),
            "apps.install".to_string(),
            "apps.delete".to_string(),
            "apps.restart".to_string(),
            "storage.view".to_string(),
            "storage.write".to_string(),
            "storage.delete".to_string(),
            "storage.download".to_string(),
            "logs.view".to_string(),
            "monitoring.view".to_string(),
            "users.view".to_string(),
            "users.manage".to_string(),
            "roles.view".to_string(),
            "roles.manage".to_string(),
            "settings.view".to_string(),
            "settings.manage".to_string(),
            "app.*".to_string(), // Admin has access to all apps
        ];
    }

    // Get all role IDs for this user
    let user_roles = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .all(db)
        .await
        .unwrap_or_default();

    let role_ids: Vec<i64> = user_roles.iter().map(|ur| ur.role_id).collect();

    if role_ids.is_empty() {
        return vec![];
    }

    // Get all permissions from all roles
    let permissions = RolePermission::find()
        .filter(role_permission::Column::RoleId.is_in(role_ids.clone()))
        .all(db)
        .await
        .unwrap_or_default();

    let mut unique_perms: Vec<String> = permissions.iter().map(|p| p.permission.clone()).collect();

    // Get app permissions and convert to app.* format
    let app_permissions = RoleAppPermission::find()
        .filter(role_app_permission::Column::RoleId.is_in(role_ids))
        .all(db)
        .await
        .unwrap_or_default();

    for app_perm in app_permissions {
        unique_perms.push(format!("app.{}", app_perm.app_name));
    }

    // Deduplicate and return
    unique_perms.sort();
    unique_perms.dedup();
    unique_perms
}

/// Get all app names a user has access to
pub async fn get_user_app_access(db: &DbConn, user_id: i64) -> Vec<String> {
    // First check if user has admin role (admin has access to all apps)
    let has_admin_role = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .join(JoinType::InnerJoin, user_role::Relation::Role.def())
        .filter(role::Column::Name.eq("admin"))
        .one(db)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false);

    if has_admin_role {
        // Admin has access to all apps - return empty to indicate "all"
        // The caller should interpret empty list as "all apps" for admin
        return vec!["*".to_string()];
    }

    // Get all role IDs for this user
    let user_roles = UserRole::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .all(db)
        .await
        .unwrap_or_default();

    let role_ids: Vec<i64> = user_roles.iter().map(|ur| ur.role_id).collect();

    if role_ids.is_empty() {
        return vec![];
    }

    // Get all app permissions from all roles
    let app_permissions = RoleAppPermission::find()
        .filter(role_app_permission::Column::RoleId.is_in(role_ids))
        .all(db)
        .await
        .unwrap_or_default();

    // Deduplicate and return
    let mut unique_apps: Vec<String> = app_permissions.iter().map(|p| p.app_name.clone()).collect();
    unique_apps.sort();
    unique_apps.dedup();
    unique_apps
}
