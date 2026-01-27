use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::role;

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

#[derive(Debug, Clone, Serialize)]
pub struct RoleResponse {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub created_at: DateTime<Utc>,
    pub app_permissions: Vec<String>,
}

impl RoleResponse {
    pub fn from_role_with_permissions(role: role::Model, app_permissions: Vec<String>) -> Self {
        Self {
            id: role.id,
            name: role.name,
            description: role.description,
            is_system: role.is_system,
            created_at: role.created_at,
            app_permissions,
        }
    }
}
