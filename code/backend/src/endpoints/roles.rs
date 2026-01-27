use axum::{
    extract::{Extension, Path, State},
    routing::{get, put},
    Json, Router,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::endpoints::extractors::user_has_permission;
use crate::middleware::AuthenticatedUser;
use crate::models::prelude::*;
use crate::models::{role, role_app_permission, role_permission};
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Create roles routes
pub fn roles_routes(state: AppState) -> Router {
    Router::new()
        .route("/", get(list_roles).post(create_role))
        .route("/permissions", get(list_all_permissions))
        .route(
            "/:role_id",
            get(get_role).patch(update_role).delete(delete_role),
        )
        .route("/:role_id/apps", put(set_role_apps))
        .route(
            "/:role_id/permissions",
            get(get_role_permissions).put(set_role_permissions),
        )
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub app_names: Vec<String>,
    #[serde(default)]
    pub requires_2fa: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub requires_2fa: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SetRoleApps {
    pub app_names: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RoleWithAppsResponse {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub requires_2fa: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub app_names: Vec<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetRolePermissions {
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PermissionInfo {
    pub key: String,
    pub category: String,
    pub description: String,
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn get_role_with_apps(state: &AppState, role_id: i64) -> Result<RoleWithAppsResponse> {
    let found_role = Role::find_by_id(role_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    let app_permissions = RoleAppPermission::find()
        .filter(role_app_permission::Column::RoleId.eq(role_id))
        .all(&state.db)
        .await?;

    let role_perms = RolePermission::find()
        .filter(role_permission::Column::RoleId.eq(role_id))
        .all(&state.db)
        .await?;

    // Convert app permissions to app.* format and merge with regular permissions
    let app_names: Vec<String> = app_permissions.iter().map(|p| p.app_name.clone()).collect();
    let mut permissions: Vec<String> = role_perms.into_iter().map(|p| p.permission).collect();

    // Add app.* permissions derived from role_app_permissions
    for app_name in &app_names {
        permissions.push(format!("app.{}", app_name));
    }

    Ok(RoleWithAppsResponse {
        id: found_role.id,
        name: found_role.name,
        description: found_role.description,
        is_system: found_role.is_system,
        requires_2fa: found_role.requires_2fa,
        created_at: found_role.created_at,
        app_names,
        permissions,
    })
}

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// List all roles (requires roles.view permission)
async fn list_roles(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<RoleWithAppsResponse>>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.view").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.view required".to_string(),
        ));
    }
    let roles = Role::find().all(&state.db).await?;

    let mut responses = Vec::new();
    for r in roles {
        responses.push(get_role_with_apps(&state, r.id).await?);
    }

    Ok(Json(responses))
}

/// Get role by ID (requires roles.view permission)
async fn get_role(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<RoleWithAppsResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.view").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.view required".to_string(),
        ));
    }
    let response = get_role_with_apps(&state, role_id).await?;
    Ok(Json(response))
}

/// Create a new role (requires roles.manage permission)
async fn create_role(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<CreateRoleRequest>,
) -> Result<Json<RoleWithAppsResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.manage required".to_string(),
        ));
    }
    // Check if role name exists
    let existing = Role::find()
        .filter(role::Column::Name.eq(&data.name))
        .one(&state.db)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Role name already exists".to_string()));
    }

    let now = Utc::now();

    // Create role
    let new_role = role::ActiveModel {
        name: Set(data.name),
        description: Set(data.description),
        is_system: Set(false),
        requires_2fa: Set(data.requires_2fa),
        created_at: Set(now),
        ..Default::default()
    };

    let created_role = new_role.insert(&state.db).await?;

    // Add app permissions
    for app_name in &data.app_names {
        let permission = role_app_permission::ActiveModel {
            role_id: Set(created_role.id),
            app_name: Set(app_name.clone()),
            ..Default::default()
        };
        permission.insert(&state.db).await?;
    }

    let response = get_role_with_apps(&state, created_role.id).await?;
    Ok(Json(response))
}

/// Update role (requires roles.manage permission)
async fn update_role(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<UpdateRoleRequest>,
) -> Result<Json<RoleWithAppsResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.manage required".to_string(),
        ));
    }
    let existing_role = Role::find_by_id(role_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    // Prevent renaming system roles
    if existing_role.is_system
        && data.name.is_some()
        && data.name.as_ref() != Some(&existing_role.name)
    {
        return Err(AppError::BadRequest(
            "Cannot rename system roles".to_string(),
        ));
    }

    // Check for duplicate name
    if let Some(ref new_name) = data.name {
        if new_name != &existing_role.name {
            let existing = Role::find()
                .filter(role::Column::Name.eq(new_name))
                .one(&state.db)
                .await?;

            if existing.is_some() {
                return Err(AppError::BadRequest("Role name already exists".to_string()));
            }
        }
    }

    // Update fields
    let mut role_model: role::ActiveModel = existing_role.into();

    if let Some(name) = data.name {
        role_model.name = Set(name);
    }
    if let Some(description) = data.description {
        role_model.description = Set(Some(description));
    }
    if let Some(requires_2fa) = data.requires_2fa {
        role_model.requires_2fa = Set(requires_2fa);
    }

    role_model.update(&state.db).await?;

    let response = get_role_with_apps(&state, role_id).await?;
    Ok(Json(response))
}

/// Delete a role (requires roles.manage permission)
async fn delete_role(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<serde_json::Value>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.manage required".to_string(),
        ));
    }
    let existing_role = Role::find_by_id(role_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    if existing_role.is_system {
        return Err(AppError::BadRequest(
            "Cannot delete system roles".to_string(),
        ));
    }

    existing_role.delete(&state.db).await?;

    Ok(Json(serde_json::json!({"message": "Role deleted"})))
}

/// Set app permissions for a role (requires roles.manage permission)
async fn set_role_apps(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<SetRoleApps>,
) -> Result<Json<RoleWithAppsResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.manage required".to_string(),
        ));
    }
    // Verify role exists
    let _ = Role::find_by_id(role_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    // Delete existing permissions
    RoleAppPermission::delete_many()
        .filter(role_app_permission::Column::RoleId.eq(role_id))
        .exec(&state.db)
        .await?;

    // Add new permissions
    for app_name in &data.app_names {
        let permission = role_app_permission::ActiveModel {
            role_id: Set(role_id),
            app_name: Set(app_name.clone()),
            ..Default::default()
        };
        permission.insert(&state.db).await?;
    }

    let response = get_role_with_apps(&state, role_id).await?;
    Ok(Json(response))
}

/// Get all available permissions with descriptions
async fn list_all_permissions(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<PermissionInfo>>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.view").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.view required".to_string(),
        ));
    }

    let mut permissions = vec![
        // Apps permissions
        PermissionInfo {
            key: "apps.view".to_string(),
            category: "Apps".to_string(),
            description: "View app catalog and installed apps".to_string(),
        },
        PermissionInfo {
            key: "apps.install".to_string(),
            category: "Apps".to_string(),
            description: "Install new applications".to_string(),
        },
        PermissionInfo {
            key: "apps.delete".to_string(),
            category: "Apps".to_string(),
            description: "Delete installed applications".to_string(),
        },
        PermissionInfo {
            key: "apps.restart".to_string(),
            category: "Apps".to_string(),
            description: "Restart application pods".to_string(),
        },
        // Storage permissions
        PermissionInfo {
            key: "storage.view".to_string(),
            category: "Storage".to_string(),
            description: "Browse storage and files".to_string(),
        },
        PermissionInfo {
            key: "storage.write".to_string(),
            category: "Storage".to_string(),
            description: "Create directories".to_string(),
        },
        PermissionInfo {
            key: "storage.delete".to_string(),
            category: "Storage".to_string(),
            description: "Delete files and directories".to_string(),
        },
        PermissionInfo {
            key: "storage.download".to_string(),
            category: "Storage".to_string(),
            description: "Download files".to_string(),
        },
        // Logs permissions
        PermissionInfo {
            key: "logs.view".to_string(),
            category: "Logs".to_string(),
            description: "View pod and application logs".to_string(),
        },
        // Monitoring permissions
        PermissionInfo {
            key: "monitoring.view".to_string(),
            category: "Monitoring".to_string(),
            description: "View metrics and monitoring data".to_string(),
        },
        // Users permissions
        PermissionInfo {
            key: "users.view".to_string(),
            category: "Users".to_string(),
            description: "View user list".to_string(),
        },
        PermissionInfo {
            key: "users.manage".to_string(),
            category: "Users".to_string(),
            description: "Create, edit, and delete users".to_string(),
        },
        // Roles permissions
        PermissionInfo {
            key: "roles.view".to_string(),
            category: "Roles".to_string(),
            description: "View roles".to_string(),
        },
        PermissionInfo {
            key: "roles.manage".to_string(),
            category: "Roles".to_string(),
            description: "Create, edit, and delete roles".to_string(),
        },
        // Settings permissions
        PermissionInfo {
            key: "settings.view".to_string(),
            category: "Settings".to_string(),
            description: "View system settings".to_string(),
        },
        PermissionInfo {
            key: "settings.manage".to_string(),
            category: "Settings".to_string(),
            description: "Modify system settings".to_string(),
        },
    ];

    // Add app access permissions
    // These are the apps that nginx can route to
    let app_permissions = vec![
        ("sonarr", "Access Sonarr TV show manager"),
        ("radarr", "Access Radarr movie manager"),
        ("qbittorrent", "Access qBittorrent download client"),
        ("transmission", "Access Transmission download client"),
        ("deluge", "Access Deluge download client"),
        ("rutorrent", "Access ruTorrent web UI"),
        ("jellyfin", "Access Jellyfin media server"),
        ("plex", "Access Plex media server"),
        ("jackett", "Access Jackett indexer proxy"),
        ("jellyseerr", "Access Jellyseerr request manager"),
        ("sabnzbd", "Access SABnzbd Usenet client"),
        ("grafana", "Access Grafana dashboards"),
        ("victoriametrics", "Access VictoriaMetrics"),
        ("loki", "Access Loki log aggregation"),
        ("kubernetes-dashboard", "Access Kubernetes Dashboard"),
    ];

    for (app_name, description) in app_permissions {
        permissions.push(PermissionInfo {
            key: format!("app.{}", app_name),
            category: "App Access".to_string(),
            description: description.to_string(),
        });
    }

    Ok(Json(permissions))
}

/// Get permissions for a specific role
async fn get_role_permissions(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<String>>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.view").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.view required".to_string(),
        ));
    }

    // Verify role exists
    let _ = Role::find_by_id(role_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    let role_perms = RolePermission::find()
        .filter(role_permission::Column::RoleId.eq(role_id))
        .all(&state.db)
        .await?;

    let permissions: Vec<String> = role_perms.into_iter().map(|p| p.permission).collect();
    Ok(Json(permissions))
}

/// Set permissions for a role (requires roles.manage permission)
/// Handles both regular permissions and app.* permissions
/// App permissions (app.sonarr, app.radarr, etc.) are synced with role_app_permissions table
async fn set_role_permissions(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<SetRolePermissions>,
) -> Result<Json<RoleWithAppsResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "roles.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: roles.manage required".to_string(),
        ));
    }

    // Verify role exists
    let _ = Role::find_by_id(role_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    // Separate app.* permissions from regular permissions
    let mut regular_permissions = Vec::new();
    let mut app_names = Vec::new();

    for permission in &data.permissions {
        if let Some(app_name) = permission.strip_prefix("app.") {
            app_names.push(app_name.to_string());
        } else {
            regular_permissions.push(permission.clone());
        }
    }

    // Delete existing regular permissions
    RolePermission::delete_many()
        .filter(role_permission::Column::RoleId.eq(role_id))
        .exec(&state.db)
        .await?;

    // Add new regular permissions
    for permission in &regular_permissions {
        let perm = role_permission::ActiveModel {
            role_id: Set(role_id),
            permission: Set(permission.clone()),
            ..Default::default()
        };
        perm.insert(&state.db).await?;
    }

    // Delete existing app permissions
    RoleAppPermission::delete_many()
        .filter(role_app_permission::Column::RoleId.eq(role_id))
        .exec(&state.db)
        .await?;

    // Add new app permissions
    for app_name in &app_names {
        let app_perm = role_app_permission::ActiveModel {
            role_id: Set(role_id),
            app_name: Set(app_name.clone()),
            ..Default::default()
        };
        app_perm.insert(&state.db).await?;
    }

    let response = get_role_with_apps(&state, role_id).await?;
    Ok(Json(response))
}
