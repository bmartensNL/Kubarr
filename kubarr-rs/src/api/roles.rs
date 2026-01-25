use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::extractors::{AdminUser, AuthUser};
use crate::db::{DbPool, Role, RoleAppPermission};
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Create roles routes
pub fn roles_routes(state: AppState) -> Router {
    Router::new()
        .route("/", get(list_roles).post(create_role))
        .route("/:role_id", get(get_role).patch(update_role).delete(delete_role))
        .route("/:role_id/apps", put(set_role_apps))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateRole {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub app_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRole {
    pub name: Option<String>,
    pub description: Option<String>,
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
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub app_names: Vec<String>,
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn get_role_with_apps(pool: &DbPool, role_id: i64) -> Result<RoleWithAppsResponse> {
    let role: Role = sqlx::query_as("SELECT * FROM roles WHERE id = ?")
        .bind(role_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    let permissions: Vec<RoleAppPermission> =
        sqlx::query_as("SELECT * FROM role_app_permissions WHERE role_id = ?")
            .bind(role_id)
            .fetch_all(pool)
            .await?;

    Ok(RoleWithAppsResponse {
        id: role.id,
        name: role.name,
        description: role.description,
        is_system: role.is_system,
        created_at: role.created_at,
        app_names: permissions.into_iter().map(|p| p.app_name).collect(),
    })
}

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// List all roles
async fn list_roles(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Vec<RoleWithAppsResponse>>> {
    let roles: Vec<Role> = sqlx::query_as("SELECT * FROM roles")
        .fetch_all(&state.pool)
        .await?;

    let mut responses = Vec::new();
    for role in roles {
        responses.push(get_role_with_apps(&state.pool, role.id).await?);
    }

    Ok(Json(responses))
}

/// Get role by ID
async fn get_role(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    AuthUser(_): AuthUser,
) -> Result<Json<RoleWithAppsResponse>> {
    let response = get_role_with_apps(&state.pool, role_id).await?;
    Ok(Json(response))
}

/// Create a new role (admin only)
async fn create_role(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
    Json(data): Json<CreateRole>,
) -> Result<Json<RoleWithAppsResponse>> {
    // Check if role name exists
    let existing: Option<Role> = sqlx::query_as("SELECT * FROM roles WHERE name = ?")
        .bind(&data.name)
        .fetch_optional(&state.pool)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Role name already exists".to_string()));
    }

    // Create role
    let result = sqlx::query(
        r#"
        INSERT INTO roles (name, description, is_system, created_at)
        VALUES (?, ?, 0, datetime('now'))
        "#,
    )
    .bind(&data.name)
    .bind(&data.description)
    .execute(&state.pool)
    .await?;

    let role_id = result.last_insert_rowid();

    // Add app permissions
    for app_name in &data.app_names {
        sqlx::query("INSERT INTO role_app_permissions (role_id, app_name) VALUES (?, ?)")
            .bind(role_id)
            .bind(app_name)
            .execute(&state.pool)
            .await?;
    }

    let response = get_role_with_apps(&state.pool, role_id).await?;
    Ok(Json(response))
}

/// Update role (admin only)
async fn update_role(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    AdminUser(_): AdminUser,
    Json(data): Json<UpdateRole>,
) -> Result<Json<RoleWithAppsResponse>> {
    let role: Role = sqlx::query_as("SELECT * FROM roles WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    // Prevent renaming system roles
    if role.is_system && data.name.is_some() && data.name.as_ref() != Some(&role.name) {
        return Err(AppError::BadRequest(
            "Cannot rename system roles".to_string(),
        ));
    }

    // Check for duplicate name
    if let Some(ref new_name) = data.name {
        if new_name != &role.name {
            let existing: Option<Role> = sqlx::query_as("SELECT * FROM roles WHERE name = ?")
                .bind(new_name)
                .fetch_optional(&state.pool)
                .await?;

            if existing.is_some() {
                return Err(AppError::BadRequest("Role name already exists".to_string()));
            }
        }
    }

    // Update fields
    if let Some(name) = &data.name {
        sqlx::query("UPDATE roles SET name = ? WHERE id = ?")
            .bind(name)
            .bind(role_id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(description) = &data.description {
        sqlx::query("UPDATE roles SET description = ? WHERE id = ?")
            .bind(description)
            .bind(role_id)
            .execute(&state.pool)
            .await?;
    }

    let response = get_role_with_apps(&state.pool, role_id).await?;
    Ok(Json(response))
}

/// Delete a role (admin only)
async fn delete_role(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    AdminUser(_): AdminUser,
) -> Result<Json<serde_json::Value>> {
    let role: Role = sqlx::query_as("SELECT * FROM roles WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    if role.is_system {
        return Err(AppError::BadRequest(
            "Cannot delete system roles".to_string(),
        ));
    }

    sqlx::query("DELETE FROM roles WHERE id = ?")
        .bind(role_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "Role deleted"})))
}

/// Set app permissions for a role (admin only)
async fn set_role_apps(
    State(state): State<AppState>,
    Path(role_id): Path<i64>,
    AdminUser(_): AdminUser,
    Json(data): Json<SetRoleApps>,
) -> Result<Json<RoleWithAppsResponse>> {
    let _: Role = sqlx::query_as("SELECT * FROM roles WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    // Delete existing permissions
    sqlx::query("DELETE FROM role_app_permissions WHERE role_id = ?")
        .bind(role_id)
        .execute(&state.pool)
        .await?;

    // Add new permissions
    for app_name in &data.app_names {
        sqlx::query("INSERT INTO role_app_permissions (role_id, app_name) VALUES (?, ?)")
            .bind(role_id)
            .bind(app_name)
            .execute(&state.pool)
            .await?;
    }

    let response = get_role_with_apps(&state.pool, role_id).await?;
    Ok(Json(response))
}
