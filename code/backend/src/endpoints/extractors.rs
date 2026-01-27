use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::models::prelude::*;
use crate::models::{role_app_permission, role_permission, user_role};
use crate::state::DbConn;

/// Get all permissions for a user (from all their roles)
/// Includes app.* permissions based on role_app_permissions
pub async fn get_user_permissions(db: &DbConn, user_id: i64) -> Vec<String> {
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

    // Get app permissions and convert to app.{name} format
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
/// Returns vec!["*"] if user has app.* permission (all apps access)
pub async fn get_user_app_access(db: &DbConn, user_id: i64) -> Vec<String> {
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

    // Check for app.* wildcard permission
    let has_wildcard = RolePermission::find()
        .filter(role_permission::Column::RoleId.is_in(role_ids.clone()))
        .filter(role_permission::Column::Permission.eq("app.*"))
        .one(db)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false);

    if has_wildcard {
        return vec!["*".to_string()];
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
