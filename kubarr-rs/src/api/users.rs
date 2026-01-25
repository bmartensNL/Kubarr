use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::api::extractors::{AdminUser, AuthUser};
use crate::db::{DbPool, Invite, Role, User};
use crate::error::{AppError, Result};
use crate::services::hash_password;
use crate::state::AppState;

/// Create users routes
pub fn users_routes(state: AppState) -> Router {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/me", get(get_current_user_info))
        .route("/pending", get(list_pending_users))
        .route("/invites", get(list_invites).post(create_invite))
        .route("/invites/:invite_id", delete(delete_invite))
        .route("/:user_id", get(get_user).patch(update_user).delete(delete_user))
        .route("/:user_id/approve", post(approve_user))
        .route("/:user_id/reject", post(reject_user))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub skip: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub role_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub is_active: Option<bool>,
    pub is_admin: Option<bool>,
    pub is_approved: Option<bool>,
    pub role_ids: Option<Vec<i64>>,
}

#[derive(Debug, Serialize)]
pub struct RoleInfo {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub is_active: bool,
    pub is_admin: bool,
    pub is_approved: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub roles: Vec<RoleInfo>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvite {
    #[serde(default = "default_invite_days")]
    pub expires_in_days: i32,
}

fn default_invite_days() -> i32 {
    7
}

#[derive(Debug, Serialize)]
pub struct InviteResponse {
    pub id: i64,
    pub code: String,
    pub created_by_username: String,
    pub used_by_username: Option<String>,
    pub is_used: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub used_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn get_user_with_roles(pool: &DbPool, user_id: i64) -> Result<UserResponse> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let roles: Vec<Role> = sqlx::query_as(
        r#"
        SELECT r.* FROM roles r
        INNER JOIN user_roles ur ON r.id = ur.role_id
        WHERE ur.user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(UserResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        is_active: user.is_active,
        is_admin: user.is_admin,
        is_approved: user.is_approved,
        created_at: user.created_at,
        updated_at: user.updated_at,
        roles: roles
            .into_iter()
            .map(|r| RoleInfo {
                id: r.id,
                name: r.name,
                description: r.description,
            })
            .collect(),
    })
}

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// List all users (admin only)
async fn list_users(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
    AdminUser(_): AdminUser,
) -> Result<Json<Vec<UserResponse>>> {
    let skip = params.skip.unwrap_or(0);
    let limit = params.limit.unwrap_or(100);

    let users: Vec<User> = sqlx::query_as("SELECT * FROM users LIMIT ? OFFSET ?")
        .bind(limit)
        .bind(skip)
        .fetch_all(&state.pool)
        .await?;

    let mut responses = Vec::new();
    for user in users {
        responses.push(get_user_with_roles(&state.pool, user.id).await?);
    }

    Ok(Json(responses))
}

/// Get current user info
async fn get_current_user_info(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<UserResponse>> {
    let response = get_user_with_roles(&state.pool, user.id).await?;
    Ok(Json(response))
}

/// List pending users (admin only)
async fn list_pending_users(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
) -> Result<Json<Vec<UserResponse>>> {
    let users: Vec<User> = sqlx::query_as("SELECT * FROM users WHERE is_approved = 0")
        .fetch_all(&state.pool)
        .await?;

    let mut responses = Vec::new();
    for user in users {
        responses.push(get_user_with_roles(&state.pool, user.id).await?);
    }

    Ok(Json(responses))
}

/// Create a new user (admin only)
async fn create_user(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
    Json(data): Json<CreateUser>,
) -> Result<Json<UserResponse>> {
    // Check if username exists
    let existing: Option<User> = sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(&data.username)
        .fetch_optional(&state.pool)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Username already exists".to_string()));
    }

    // Check if email exists
    let existing: Option<User> = sqlx::query_as("SELECT * FROM users WHERE email = ?")
        .bind(&data.email)
        .fetch_optional(&state.pool)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Email already exists".to_string()));
    }

    let hashed = hash_password(&data.password)?;

    // Create user
    let result = sqlx::query(
        r#"
        INSERT INTO users (username, email, hashed_password, is_admin, is_active, is_approved, created_at, updated_at)
        VALUES (?, ?, ?, ?, 1, 1, datetime('now'), datetime('now'))
        "#,
    )
    .bind(&data.username)
    .bind(&data.email)
    .bind(&hashed)
    .bind(data.is_admin)
    .execute(&state.pool)
    .await?;

    let user_id = result.last_insert_rowid();

    // Assign roles
    for role_id in &data.role_ids {
        sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES (?, ?)")
            .bind(user_id)
            .bind(role_id)
            .execute(&state.pool)
            .await?;
    }

    let response = get_user_with_roles(&state.pool, user_id).await?;
    Ok(Json(response))
}

/// Get user by ID (admin only)
async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    AdminUser(_): AdminUser,
) -> Result<Json<UserResponse>> {
    let response = get_user_with_roles(&state.pool, user_id).await?;
    Ok(Json(response))
}

/// Update user (admin only)
async fn update_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    AdminUser(_): AdminUser,
    Json(data): Json<UpdateUser>,
) -> Result<Json<UserResponse>> {
    // Check user exists
    let _: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Build update query dynamically
    if let Some(email) = &data.email {
        sqlx::query("UPDATE users SET email = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(email)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(is_active) = data.is_active {
        sqlx::query("UPDATE users SET is_active = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(is_active)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(is_admin) = data.is_admin {
        sqlx::query("UPDATE users SET is_admin = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(is_admin)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(is_approved) = data.is_approved {
        sqlx::query("UPDATE users SET is_approved = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(is_approved)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
    }

    // Update roles if provided
    if let Some(role_ids) = &data.role_ids {
        // Delete existing roles
        sqlx::query("DELETE FROM user_roles WHERE user_id = ?")
            .bind(user_id)
            .execute(&state.pool)
            .await?;

        // Add new roles
        for role_id in role_ids {
            sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES (?, ?)")
                .bind(user_id)
                .bind(role_id)
                .execute(&state.pool)
                .await?;
        }
    }

    let response = get_user_with_roles(&state.pool, user_id).await?;
    Ok(Json(response))
}

/// Approve a user registration (admin only)
async fn approve_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    AdminUser(_): AdminUser,
) -> Result<Json<UserResponse>> {
    let _: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    sqlx::query(
        "UPDATE users SET is_approved = 1, is_active = 1, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(user_id)
    .execute(&state.pool)
    .await?;

    let response = get_user_with_roles(&state.pool, user_id).await?;
    Ok(Json(response))
}

/// Reject a user registration (admin only)
async fn reject_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    AdminUser(_): AdminUser,
) -> Result<Json<serde_json::Value>> {
    let _: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(user_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "User rejected and deleted"})))
}

/// Delete a user (admin only)
async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    AdminUser(admin): AdminUser,
) -> Result<Json<serde_json::Value>> {
    if user_id == admin.id {
        return Err(AppError::BadRequest("Cannot delete yourself".to_string()));
    }

    let _: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(user_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "User deleted"})))
}

/// List all invites (admin only)
async fn list_invites(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
) -> Result<Json<Vec<InviteResponse>>> {
    let invites: Vec<Invite> =
        sqlx::query_as("SELECT * FROM invites ORDER BY created_at DESC")
            .fetch_all(&state.pool)
            .await?;

    let mut responses = Vec::new();
    for invite in invites {
        let created_by: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
            .bind(invite.created_by_id)
            .fetch_optional(&state.pool)
            .await?;

        let used_by: Option<User> = if let Some(used_by_id) = invite.used_by_id {
            sqlx::query_as("SELECT * FROM users WHERE id = ?")
                .bind(used_by_id)
                .fetch_optional(&state.pool)
                .await?
        } else {
            None
        };

        responses.push(InviteResponse {
            id: invite.id,
            code: invite.code,
            created_by_username: created_by.map(|u| u.username).unwrap_or_else(|| "Unknown".to_string()),
            used_by_username: used_by.map(|u| u.username),
            is_used: invite.is_used,
            expires_at: invite.expires_at,
            created_at: invite.created_at,
            used_at: invite.used_at,
        });
    }

    Ok(Json(responses))
}

/// Create an invite (admin only)
async fn create_invite(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(data): Json<CreateInvite>,
) -> Result<Json<InviteResponse>> {
    use crate::services::generate_random_string;

    let code = generate_random_string(32);
    let expires_at = if data.expires_in_days > 0 {
        Some(Utc::now() + Duration::days(data.expires_in_days as i64))
    } else {
        None
    };

    sqlx::query(
        r#"
        INSERT INTO invites (code, created_by_id, expires_at, created_at)
        VALUES (?, ?, ?, datetime('now'))
        "#,
    )
    .bind(&code)
    .bind(admin.id)
    .bind(expires_at)
    .execute(&state.pool)
    .await?;

    let invite: Invite = sqlx::query_as("SELECT * FROM invites WHERE code = ?")
        .bind(&code)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(InviteResponse {
        id: invite.id,
        code: invite.code,
        created_by_username: admin.username,
        used_by_username: None,
        is_used: invite.is_used,
        expires_at: invite.expires_at,
        created_at: invite.created_at,
        used_at: invite.used_at,
    }))
}

/// Delete an invite (admin only)
async fn delete_invite(
    State(state): State<AppState>,
    Path(invite_id): Path<i64>,
    AdminUser(_): AdminUser,
) -> Result<Json<serde_json::Value>> {
    let _: Invite = sqlx::query_as("SELECT * FROM invites WHERE id = ?")
        .bind(invite_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Invite not found".to_string()))?;

    sqlx::query("DELETE FROM invites WHERE id = ?")
        .bind(invite_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({"message": "Invite deleted"})))
}
