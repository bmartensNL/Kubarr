use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{role, user};

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub role_ids: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub is_active: Option<bool>,
    pub is_approved: Option<bool>,
    pub role_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub is_active: bool,
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

impl From<role::Model> for RoleInfo {
    fn from(role: role::Model) -> Self {
        Self {
            id: role.id,
            name: role.name,
            description: role.description,
        }
    }
}

impl UserResponse {
    pub fn from_user_with_roles(user: user::Model, roles: Vec<role::Model>) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            is_active: user.is_active,
            is_approved: user.is_approved,
            created_at: user.created_at,
            updated_at: user.updated_at,
            roles: roles.into_iter().map(RoleInfo::from).collect(),
        }
    }
}
