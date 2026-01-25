use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ============================================================================
// User Models
// ============================================================================

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub hashed_password: String,
    pub is_active: bool,
    pub is_admin: bool,
    pub is_approved: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserWithRoles {
    #[serde(flatten)]
    pub user: User,
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub role_ids: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub is_active: Option<bool>,
    pub is_admin: Option<bool>,
    pub is_approved: Option<bool>,
    pub role_ids: Option<Vec<i64>>,
}

// ============================================================================
// Role Models
// ============================================================================

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Role {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleWithPermissions {
    #[serde(flatten)]
    pub role: Role,
    pub app_permissions: Vec<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RoleAppPermission {
    pub id: i64,
    pub role_id: i64,
    pub app_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRole {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub app_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRole {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetRoleApps {
    pub app_names: Vec<String>,
}

// ============================================================================
// OAuth2 Models
// ============================================================================

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct OAuth2Client {
    pub client_id: String,
    pub client_secret_hash: String,
    pub name: String,
    pub redirect_uris: String, // JSON array as text
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct OAuth2AuthorizationCode {
    pub code: String,
    pub client_id: String,
    pub user_id: i64,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub used: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct OAuth2Token {
    pub id: i64,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub client_id: String,
    pub user_id: i64,
    pub scope: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub refresh_expires_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Invite Models
// ============================================================================

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Invite {
    pub id: i64,
    pub code: String,
    pub created_by_id: i64,
    pub used_by_id: Option<i64>,
    pub is_used: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteWithUsers {
    #[serde(flatten)]
    pub invite: Invite,
    pub created_by_username: String,
    pub used_by_username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvite {
    #[serde(default = "default_invite_days")]
    pub expires_in_days: i32,
}

fn default_invite_days() -> i32 {
    7
}

// ============================================================================
// System Settings Models
// ============================================================================

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SystemSetting {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSetting {
    pub value: String,
}

// ============================================================================
// User Role Association (junction table)
// ============================================================================

#[derive(Debug, Clone, FromRow)]
pub struct UserRole {
    pub user_id: i64,
    pub role_id: i64,
}

// ============================================================================
// API Response Models
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub is_active: bool,
    pub is_admin: bool,
    pub is_approved: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub roles: Vec<RoleInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleInfo {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
}

impl From<Role> for RoleInfo {
    fn from(role: Role) -> Self {
        Self {
            id: role.id,
            name: role.name,
            description: role.description,
        }
    }
}

impl UserResponse {
    pub fn from_user_with_roles(user: User, roles: Vec<Role>) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            is_active: user.is_active,
            is_admin: user.is_admin,
            is_approved: user.is_approved,
            created_at: user.created_at,
            updated_at: user.updated_at,
            roles: roles.into_iter().map(RoleInfo::from).collect(),
        }
    }
}
