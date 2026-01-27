use axum::{
    extract::{Extension, Path, Query, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, QueryOrder, QuerySelect,
    Set,
};
use serde::{Deserialize, Serialize};

use crate::endpoints::extractors::{get_user_app_access, get_user_permissions, user_has_permission};
use crate::middleware::AuthenticatedUser;
use crate::models::prelude::*;
use crate::models::{invite, role, user, user_preferences, user_role};
use crate::error::{AppError, Result};
use crate::services::{
    generate_totp_secret, get_totp_provisioning_uri, hash_password, verify_password, verify_totp,
};
use crate::state::AppState;

/// Create users routes
pub fn users_routes(state: AppState) -> Router {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/me", get(get_current_user_info))
        .route(
            "/me/preferences",
            get(get_my_preferences).patch(update_my_preferences),
        )
        .route("/me/password", patch(change_own_password))
        .route("/me/2fa/setup", post(setup_2fa))
        .route("/me/2fa/enable", post(enable_2fa))
        .route("/me/2fa/disable", post(disable_2fa))
        .route("/me/2fa/status", get(get_2fa_status))
        .route("/pending", get(list_pending_users))
        .route("/invites", get(list_invites).post(create_invite))
        .route("/invites/:invite_id", delete(delete_invite))
        .route(
            "/:user_id",
            get(get_user).patch(update_user).delete(delete_user),
        )
        .route("/:user_id/approve", post(approve_user))
        .route("/:user_id/reject", post(reject_user))
        .route("/:user_id/password", patch(admin_reset_password))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub skip: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub role_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    pub is_active: Option<bool>,
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
pub struct PreferencesResponse {
    pub theme: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePreferences {
    pub theme: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChangeOwnPasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct AdminResetPasswordRequest {
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct TwoFactorSetupResponse {
    pub secret: String,
    pub provisioning_uri: String,
}

#[derive(Debug, Deserialize)]
pub struct Enable2FARequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct Disable2FARequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct TwoFactorStatusResponse {
    pub enabled: bool,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub required_by_role: bool,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub is_active: bool,
    pub is_approved: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub roles: Vec<RoleInfo>,
    pub preferences: PreferencesResponse,
    pub permissions: Vec<String>,
    pub allowed_apps: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInviteRequest {
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

async fn get_user_with_roles(state: &AppState, user_id: i64) -> Result<UserResponse> {
    let found_user = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Get user's roles via the junction table
    let roles: Vec<role::Model> = Role::find()
        .inner_join(UserRole)
        .filter(user_role::Column::UserId.eq(user_id))
        .all(&state.db)
        .await?;

    // Fetch user preferences (or use defaults)
    let preferences = UserPreferences::find_by_id(user_id).one(&state.db).await?;

    let theme = preferences
        .map(|p| p.theme)
        .unwrap_or_else(|| "system".to_string());

    // Get user's permissions and allowed apps
    let permissions = get_user_permissions(&state.db, user_id).await;
    let allowed_apps = get_user_app_access(&state.db, user_id).await;

    Ok(UserResponse {
        id: found_user.id,
        username: found_user.username.clone(),
        email: found_user.email,
        is_active: found_user.is_active,
        is_approved: found_user.is_approved,
        created_at: found_user.created_at,
        updated_at: found_user.updated_at,
        roles: roles
            .into_iter()
            .map(|r| RoleInfo {
                id: r.id,
                name: r.name,
                description: r.description,
            })
            .collect(),
        preferences: PreferencesResponse { theme },
        permissions,
        allowed_apps,
    })
}

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// List all users (requires users.view permission)
async fn list_users(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<UserResponse>>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.view").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.view required".to_string(),
        ));
    }
    let skip = params.skip.unwrap_or(0);
    let limit = params.limit.unwrap_or(100);

    let users = User::find()
        .offset(skip)
        .limit(limit)
        .all(&state.db)
        .await?;

    let mut responses = Vec::new();
    for u in users {
        responses.push(get_user_with_roles(&state, u.id).await?);
    }

    Ok(Json(responses))
}

/// Get current user info
async fn get_current_user_info(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<UserResponse>> {
    let response = get_user_with_roles(&state, auth_user.0.id).await?;
    Ok(Json(response))
}

/// Get current user's preferences
async fn get_my_preferences(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<PreferencesResponse>> {
    let preferences = UserPreferences::find_by_id(auth_user.0.id)
        .one(&state.db)
        .await?;

    let theme = preferences
        .map(|p| p.theme)
        .unwrap_or_else(|| "system".to_string());

    Ok(Json(PreferencesResponse { theme }))
}

/// Update current user's preferences
async fn update_my_preferences(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<UpdatePreferences>,
) -> Result<Json<PreferencesResponse>> {
    // Validate theme value
    if let Some(ref theme) = data.theme {
        if !["system", "light", "dark"].contains(&theme.as_str()) {
            return Err(AppError::BadRequest(
                "Invalid theme value. Must be 'system', 'light', or 'dark'".to_string(),
            ));
        }
    }

    let now = Utc::now();

    // Check if preferences exist
    let existing = UserPreferences::find_by_id(auth_user.0.id)
        .one(&state.db)
        .await?;

    if let Some(existing_prefs) = existing {
        // Update existing preferences
        if let Some(ref theme) = data.theme {
            let mut active_model: user_preferences::ActiveModel = existing_prefs.into();
            active_model.theme = Set(theme.clone());
            active_model.updated_at = Set(now);
            active_model.update(&state.db).await?;
        }
    } else {
        // Insert new preferences
        let theme = data.theme.as_deref().unwrap_or("system");
        let new_prefs = user_preferences::ActiveModel {
            user_id: Set(auth_user.0.id),
            theme: Set(theme.to_string()),
            updated_at: Set(now),
        };
        new_prefs.insert(&state.db).await?;
    }

    // Return updated preferences
    let preferences = UserPreferences::find_by_id(auth_user.0.id)
        .one(&state.db)
        .await?;

    let theme = preferences
        .map(|p| p.theme)
        .unwrap_or_else(|| "system".to_string());

    Ok(Json(PreferencesResponse { theme }))
}

/// List pending users (requires users.view permission)
async fn list_pending_users(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<UserResponse>>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.view").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.view required".to_string(),
        ));
    }
    let users = User::find()
        .filter(user::Column::IsApproved.eq(false))
        .all(&state.db)
        .await?;

    let mut responses = Vec::new();
    for u in users {
        responses.push(get_user_with_roles(&state, u.id).await?);
    }

    Ok(Json(responses))
}

/// Create a new user (requires users.manage permission)
async fn create_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.manage required".to_string(),
        ));
    }
    // Check if username exists
    let existing = User::find()
        .filter(user::Column::Username.eq(&data.username))
        .one(&state.db)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Username already exists".to_string()));
    }

    // Check if email exists
    let existing = User::find()
        .filter(user::Column::Email.eq(&data.email))
        .one(&state.db)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Email already exists".to_string()));
    }

    let hashed = hash_password(&data.password)?;
    let now = Utc::now();

    // Create user
    let new_user = user::ActiveModel {
        username: Set(data.username),
        email: Set(data.email),
        hashed_password: Set(hashed),
        is_active: Set(true),
        is_approved: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let created_user = new_user.insert(&state.db).await?;

    // Assign roles
    for role_id in &data.role_ids {
        let user_role_model = user_role::ActiveModel {
            user_id: Set(created_user.id),
            role_id: Set(*role_id),
        };
        user_role_model.insert(&state.db).await?;
    }

    let response = get_user_with_roles(&state, created_user.id).await?;
    Ok(Json(response))
}

/// Get user by ID (requires users.view permission)
async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<UserResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.view").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.view required".to_string(),
        ));
    }
    let response = get_user_with_roles(&state, user_id).await?;
    Ok(Json(response))
}

/// Update user (requires users.manage permission)
async fn update_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.manage required".to_string(),
        ));
    }
    // Check user exists
    let existing_user = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let now = Utc::now();
    let mut user_model: user::ActiveModel = existing_user.into();

    // Update fields if provided
    if let Some(email) = data.email {
        user_model.email = Set(email);
    }
    if let Some(is_active) = data.is_active {
        user_model.is_active = Set(is_active);
    }
    if let Some(is_approved) = data.is_approved {
        user_model.is_approved = Set(is_approved);
    }
    user_model.updated_at = Set(now);

    user_model.update(&state.db).await?;

    // Update roles if provided
    if let Some(role_ids) = &data.role_ids {
        // Delete existing roles
        UserRole::delete_many()
            .filter(user_role::Column::UserId.eq(user_id))
            .exec(&state.db)
            .await?;

        // Add new roles
        for role_id in role_ids {
            let user_role_model = user_role::ActiveModel {
                user_id: Set(user_id),
                role_id: Set(*role_id),
            };
            user_role_model.insert(&state.db).await?;
        }
    }

    let response = get_user_with_roles(&state, user_id).await?;
    Ok(Json(response))
}

/// Approve a user registration (requires users.manage permission)
async fn approve_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<UserResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.manage required".to_string(),
        ));
    }
    let existing_user = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let now = Utc::now();
    let mut user_model: user::ActiveModel = existing_user.into();
    user_model.is_approved = Set(true);
    user_model.is_active = Set(true);
    user_model.updated_at = Set(now);

    user_model.update(&state.db).await?;

    let response = get_user_with_roles(&state, user_id).await?;
    Ok(Json(response))
}

/// Reject a user registration (requires users.manage permission)
async fn reject_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<serde_json::Value>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.manage required".to_string(),
        ));
    }
    let existing_user = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    existing_user.delete(&state.db).await?;

    Ok(Json(
        serde_json::json!({"message": "User rejected and deleted"}),
    ))
}

/// Delete a user (requires users.manage permission)
async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<serde_json::Value>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.manage required".to_string(),
        ));
    }
    if user_id == auth_user.0.id {
        return Err(AppError::BadRequest("Cannot delete yourself".to_string()));
    }

    let existing_user = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    existing_user.delete(&state.db).await?;

    Ok(Json(serde_json::json!({"message": "User deleted"})))
}

/// List all invites (requires users.manage permission)
async fn list_invites(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<InviteResponse>>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.manage required".to_string(),
        ));
    }
    let invites = Invite::find()
        .order_by_desc(invite::Column::CreatedAt)
        .all(&state.db)
        .await?;

    let mut responses = Vec::new();
    for inv in invites {
        let created_by = User::find_by_id(inv.created_by_id).one(&state.db).await?;

        let used_by = if let Some(used_by_id) = inv.used_by_id {
            User::find_by_id(used_by_id).one(&state.db).await?
        } else {
            None
        };

        responses.push(InviteResponse {
            id: inv.id,
            code: inv.code,
            created_by_username: created_by
                .map(|u| u.username)
                .unwrap_or_else(|| "Unknown".to_string()),
            used_by_username: used_by.map(|u| u.username),
            is_used: inv.is_used,
            expires_at: inv.expires_at,
            created_at: inv.created_at,
            used_at: inv.used_at,
        });
    }

    Ok(Json(responses))
}

/// Create an invite (requires users.manage permission)
async fn create_invite(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<CreateInviteRequest>,
) -> Result<Json<InviteResponse>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.manage required".to_string(),
        ));
    }
    use crate::services::generate_random_string;

    let code = generate_random_string(32);
    let expires_at = if data.expires_in_days > 0 {
        Some(Utc::now() + Duration::days(data.expires_in_days as i64))
    } else {
        None
    };
    let now = Utc::now();

    let new_invite = invite::ActiveModel {
        code: Set(code.clone()),
        created_by_id: Set(auth_user.0.id),
        expires_at: Set(expires_at),
        created_at: Set(now),
        is_used: Set(false),
        ..Default::default()
    };

    let created_invite = new_invite.insert(&state.db).await?;

    Ok(Json(InviteResponse {
        id: created_invite.id,
        code: created_invite.code,
        created_by_username: auth_user.0.username.clone(),
        used_by_username: None,
        is_used: created_invite.is_used,
        expires_at: created_invite.expires_at,
        created_at: created_invite.created_at,
        used_at: created_invite.used_at,
    }))
}

/// Delete an invite (requires users.manage permission)
async fn delete_invite(
    State(state): State<AppState>,
    Path(invite_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<serde_json::Value>> {
    if !user_has_permission(&state.db, auth_user.0.id, "users.manage").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.manage required".to_string(),
        ));
    }
    let existing_invite = Invite::find_by_id(invite_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Invite not found".to_string()))?;

    existing_invite.delete(&state.db).await?;

    Ok(Json(serde_json::json!({"message": "Invite deleted"})))
}

// ============================================================================
// Password Change Endpoints
// ============================================================================

/// Change own password (requires current password)
async fn change_own_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<ChangeOwnPasswordRequest>,
) -> Result<Json<serde_json::Value>> {
    // Validate new password
    if data.new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "New password must be at least 8 characters".to_string(),
        ));
    }

    // Get fresh user data to verify current password
    let user_record = User::find_by_id(auth_user.0.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Verify current password
    if !verify_password(&data.current_password, &user_record.hashed_password) {
        return Err(AppError::BadRequest(
            "Current password is incorrect".to_string(),
        ));
    }

    // Hash and update password
    let hashed = hash_password(&data.new_password)?;
    let now = Utc::now();

    let mut user_model: user::ActiveModel = user_record.into();
    user_model.hashed_password = Set(hashed);
    user_model.updated_at = Set(now);
    user_model.update(&state.db).await?;

    Ok(Json(
        serde_json::json!({"message": "Password changed successfully"}),
    ))
}

/// Admin reset password for another user (requires users.reset_password permission)
async fn admin_reset_password(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<AdminResetPasswordRequest>,
) -> Result<Json<serde_json::Value>> {
    // Check permission
    if !user_has_permission(&state.db, auth_user.0.id, "users.reset_password").await {
        return Err(AppError::Forbidden(
            "Permission denied: users.reset_password required".to_string(),
        ));
    }

    // Prevent admin from resetting their own password via this endpoint
    if user_id == auth_user.0.id {
        return Err(AppError::BadRequest(
            "Use /api/users/me/password to change your own password".to_string(),
        ));
    }

    // Validate new password
    if data.new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "New password must be at least 8 characters".to_string(),
        ));
    }

    // Get target user
    let user_record = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Hash and update password
    let hashed = hash_password(&data.new_password)?;
    let now = Utc::now();

    let mut user_model: user::ActiveModel = user_record.into();
    user_model.hashed_password = Set(hashed);
    user_model.updated_at = Set(now);
    user_model.update(&state.db).await?;

    Ok(Json(
        serde_json::json!({"message": "Password reset successfully"}),
    ))
}

// ============================================================================
// Two-Factor Authentication Endpoints
// ============================================================================

/// Check if user's role requires 2FA
async fn user_requires_2fa(db: &sea_orm::DatabaseConnection, user_id: i64) -> bool {
    // Get user's roles and check if any require 2FA
    let roles: Vec<role::Model> = Role::find()
        .inner_join(UserRole)
        .filter(user_role::Column::UserId.eq(user_id))
        .all(db)
        .await
        .unwrap_or_default();

    roles.iter().any(|r| r.requires_2fa)
}

/// Set up 2FA - generate secret and QR code
async fn setup_2fa(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<TwoFactorSetupResponse>> {
    // Get fresh user data
    let user_record = User::find_by_id(auth_user.0.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Check if 2FA is already enabled
    if user_record.totp_enabled {
        return Err(AppError::BadRequest(
            "2FA is already enabled. Disable it first to set up a new secret.".to_string(),
        ));
    }

    // Generate new TOTP secret
    let secret = generate_totp_secret();
    let account_name = &user_record.email;

    // Get provisioning URI (frontend generates QR code)
    let provisioning_uri = get_totp_provisioning_uri(&secret, account_name)?;

    // Store the secret (but don't enable yet - user must verify first)
    let now = Utc::now();
    let mut user_model: user::ActiveModel = user_record.into();
    user_model.totp_secret = Set(Some(secret.clone()));
    user_model.updated_at = Set(now);
    user_model.update(&state.db).await?;

    Ok(Json(TwoFactorSetupResponse {
        secret,
        provisioning_uri,
    }))
}

/// Enable 2FA - verify the code and activate
async fn enable_2fa(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<Enable2FARequest>,
) -> Result<Json<serde_json::Value>> {
    // Get fresh user data
    let user_record = User::find_by_id(auth_user.0.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Check if 2FA is already enabled
    if user_record.totp_enabled {
        return Err(AppError::BadRequest("2FA is already enabled".to_string()));
    }

    // Check if secret exists (user must call setup first)
    let secret = user_record.totp_secret.as_ref().ok_or_else(|| {
        AppError::BadRequest(
            "No 2FA setup in progress. Call /api/users/me/2fa/setup first.".to_string(),
        )
    })?;

    // Verify the code
    if !verify_totp(secret, &data.code, &user_record.email)? {
        return Err(AppError::BadRequest(
            "Invalid verification code".to_string(),
        ));
    }

    // Enable 2FA
    let now = Utc::now();
    let mut user_model: user::ActiveModel = user_record.into();
    user_model.totp_enabled = Set(true);
    user_model.totp_verified_at = Set(Some(now));
    user_model.updated_at = Set(now);
    user_model.update(&state.db).await?;

    Ok(Json(serde_json::json!({
        "message": "Two-factor authentication enabled successfully"
    })))
}

/// Disable 2FA (requires password confirmation)
async fn disable_2fa(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(data): Json<Disable2FARequest>,
) -> Result<Json<serde_json::Value>> {
    // Get fresh user data
    let user_record = User::find_by_id(auth_user.0.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Check if 2FA is enabled
    if !user_record.totp_enabled {
        return Err(AppError::BadRequest("2FA is not enabled".to_string()));
    }

    // Verify password
    if !verify_password(&data.password, &user_record.hashed_password) {
        return Err(AppError::BadRequest("Incorrect password".to_string()));
    }

    // Check if user's role requires 2FA
    if user_requires_2fa(&state.db, auth_user.0.id).await {
        return Err(AppError::BadRequest(
            "Cannot disable 2FA - your role requires two-factor authentication".to_string(),
        ));
    }

    // Disable 2FA and clear secret
    let now = Utc::now();
    let mut user_model: user::ActiveModel = user_record.into();
    user_model.totp_enabled = Set(false);
    user_model.totp_secret = Set(None);
    user_model.totp_verified_at = Set(None);
    user_model.updated_at = Set(now);
    user_model.update(&state.db).await?;

    Ok(Json(serde_json::json!({
        "message": "Two-factor authentication disabled"
    })))
}

/// Get 2FA status
async fn get_2fa_status(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<TwoFactorStatusResponse>> {
    // Get fresh user data
    let user_record = User::find_by_id(auth_user.0.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Check if role requires 2FA
    let required_by_role = user_requires_2fa(&state.db, auth_user.0.id).await;

    Ok(Json(TwoFactorStatusResponse {
        enabled: user_record.totp_enabled,
        verified_at: user_record.totp_verified_at,
        required_by_role,
    }))
}
