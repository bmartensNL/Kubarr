use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
};
use sea_orm::{ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait};

use crate::api::middleware::AuthenticatedUser;
use crate::models::prelude::*;
use crate::models::{role, role_app_permission, role_permission, user, user_role};
use crate::error::AppError;
use crate::state::{AppState, DbConn};

/// Extractor for admin users - reads from extensions and checks admin role
pub struct AdminUser(pub user::Model);

#[async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Get user from extensions (set by auth middleware)
        let auth_user = parts
            .extensions
            .get::<AuthenticatedUser>()
            .ok_or_else(|| AppError::Unauthorized("Authentication required".to_string()))?;

        let user = auth_user.0.clone();

        // Check if user has admin role
        let has_admin_role = UserRole::find()
            .filter(user_role::Column::UserId.eq(user.id))
            .join(JoinType::InnerJoin, user_role::Relation::Role.def())
            .filter(role::Column::Name.eq("admin"))
            .one(&state.db)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false);

        if has_admin_role {
            Ok(AdminUser(user))
        } else {
            Err(AppError::Forbidden("Admin access required".to_string()))
        }
    }
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
