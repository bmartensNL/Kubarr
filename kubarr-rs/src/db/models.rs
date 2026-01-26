use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db::entities::{role, user};

// ============================================================================
// User Request/Response Models (DTOs)
// ============================================================================

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

// ============================================================================
// Role Request/Response Models (DTOs)
// ============================================================================

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
// Invite Request Models (DTOs)
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvite {
    #[serde(default = "default_invite_days")]
    pub expires_in_days: i32,
}

fn default_invite_days() -> i32 {
    7
}

// ============================================================================
// System Settings Models (DTOs)
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSetting {
    pub value: String,
}

// ============================================================================
// User Preferences Models (DTOs)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct UserPreferencesResponse {
    pub theme: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUserPreferences {
    pub theme: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
pub struct InviteResponse {
    pub id: i64,
    pub code: String,
    pub created_by_id: i64,
    pub created_by_username: String,
    pub used_by_id: Option<i64>,
    pub used_by_username: Option<String>,
    pub is_used: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // ==========================================================================
    // CreateUser Tests
    // ==========================================================================

    #[test]
    fn test_create_user_deserialize() {
        let json = r#"{
            "username": "testuser",
            "email": "test@example.com",
            "password": "password123"
        }"#;

        let user: CreateUser = serde_json::from_str(json).unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.password, "password123");
        assert!(user.role_ids.is_empty()); // Default empty vec
    }

    #[test]
    fn test_create_user_with_roles() {
        let json = r#"{
            "username": "testuser",
            "email": "test@example.com",
            "password": "password123",
            "role_ids": [1, 2, 3]
        }"#;

        let user: CreateUser = serde_json::from_str(json).unwrap();
        assert_eq!(user.role_ids, vec![1, 2, 3]);
    }

    // ==========================================================================
    // UpdateUser Tests
    // ==========================================================================

    #[test]
    fn test_update_user_partial() {
        let json = r#"{
            "email": "new@example.com"
        }"#;

        let update: UpdateUser = serde_json::from_str(json).unwrap();
        assert_eq!(update.email, Some("new@example.com".to_string()));
        assert!(update.is_active.is_none());
        assert!(update.is_approved.is_none());
        assert!(update.role_ids.is_none());
    }

    #[test]
    fn test_update_user_full() {
        let json = r#"{
            "email": "new@example.com",
            "is_active": false,
            "is_approved": true,
            "role_ids": [1, 2]
        }"#;

        let update: UpdateUser = serde_json::from_str(json).unwrap();
        assert_eq!(update.email, Some("new@example.com".to_string()));
        assert_eq!(update.is_active, Some(false));
        assert_eq!(update.is_approved, Some(true));
        assert_eq!(update.role_ids, Some(vec![1, 2]));
    }

    // ==========================================================================
    // CreateRole Tests
    // ==========================================================================

    #[test]
    fn test_create_role_minimal() {
        let json = r#"{
            "name": "custom_role"
        }"#;

        let role: CreateRole = serde_json::from_str(json).unwrap();
        assert_eq!(role.name, "custom_role");
        assert!(role.description.is_none());
        assert!(role.app_names.is_empty());
    }

    #[test]
    fn test_create_role_full() {
        let json = r#"{
            "name": "custom_role",
            "description": "A custom role description",
            "app_names": ["sonarr", "radarr"]
        }"#;

        let role: CreateRole = serde_json::from_str(json).unwrap();
        assert_eq!(role.name, "custom_role");
        assert_eq!(
            role.description,
            Some("A custom role description".to_string())
        );
        assert_eq!(role.app_names, vec!["sonarr", "radarr"]);
    }

    // ==========================================================================
    // UpdateRole Tests
    // ==========================================================================

    #[test]
    fn test_update_role_empty() {
        let json = r#"{}"#;

        let update: UpdateRole = serde_json::from_str(json).unwrap();
        assert!(update.name.is_none());
        assert!(update.description.is_none());
    }

    #[test]
    fn test_update_role_name_only() {
        let json = r#"{"name": "new_name"}"#;

        let update: UpdateRole = serde_json::from_str(json).unwrap();
        assert_eq!(update.name, Some("new_name".to_string()));
        assert!(update.description.is_none());
    }

    // ==========================================================================
    // SetRoleApps Tests
    // ==========================================================================

    #[test]
    fn test_set_role_apps() {
        let json = r#"{"app_names": ["jellyfin", "jellyseerr", "sonarr"]}"#;

        let apps: SetRoleApps = serde_json::from_str(json).unwrap();
        assert_eq!(apps.app_names, vec!["jellyfin", "jellyseerr", "sonarr"]);
    }

    #[test]
    fn test_set_role_apps_empty() {
        let json = r#"{"app_names": []}"#;

        let apps: SetRoleApps = serde_json::from_str(json).unwrap();
        assert!(apps.app_names.is_empty());
    }

    // ==========================================================================
    // CreateInvite Tests
    // ==========================================================================

    #[test]
    fn test_create_invite_default_expiry() {
        let json = r#"{}"#;

        let invite: CreateInvite = serde_json::from_str(json).unwrap();
        assert_eq!(invite.expires_in_days, 7); // Default value
    }

    #[test]
    fn test_create_invite_custom_expiry() {
        let json = r#"{"expires_in_days": 30}"#;

        let invite: CreateInvite = serde_json::from_str(json).unwrap();
        assert_eq!(invite.expires_in_days, 30);
    }

    // ==========================================================================
    // UpdateSetting Tests
    // ==========================================================================

    #[test]
    fn test_update_setting() {
        let json = r#"{"value": "true"}"#;

        let setting: UpdateSetting = serde_json::from_str(json).unwrap();
        assert_eq!(setting.value, "true");
    }

    // ==========================================================================
    // UserPreferences Tests
    // ==========================================================================

    #[test]
    fn test_user_preferences_response_serialize() {
        let prefs = UserPreferencesResponse {
            theme: "dark".to_string(),
        };

        let json = serde_json::to_string(&prefs).unwrap();
        assert!(json.contains("\"theme\":\"dark\""));
    }

    #[test]
    fn test_update_user_preferences() {
        let json = r#"{"theme": "light"}"#;

        let update: UpdateUserPreferences = serde_json::from_str(json).unwrap();
        assert_eq!(update.theme, Some("light".to_string()));
    }

    #[test]
    fn test_update_user_preferences_empty() {
        let json = r#"{}"#;

        let update: UpdateUserPreferences = serde_json::from_str(json).unwrap();
        assert!(update.theme.is_none());
    }

    // ==========================================================================
    // UserResponse Tests
    // ==========================================================================

    #[test]
    fn test_user_response_serialize() {
        let now = Utc::now();
        let response = UserResponse {
            id: 1,
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            is_active: true,
            is_approved: true,
            created_at: now,
            updated_at: now,
            roles: vec![RoleInfo {
                id: 1,
                name: "admin".to_string(),
                description: Some("Administrator".to_string()),
            }],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"username\":\"testuser\""));
        assert!(json.contains("\"email\":\"test@example.com\""));
        assert!(json.contains("\"is_active\":true"));
        assert!(json.contains("\"admin\""));
    }

    // ==========================================================================
    // RoleInfo Tests
    // ==========================================================================

    #[test]
    fn test_role_info_serialize() {
        let info = RoleInfo {
            id: 1,
            name: "admin".to_string(),
            description: Some("Full access".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"name\":\"admin\""));
        assert!(json.contains("\"description\":\"Full access\""));
    }

    #[test]
    fn test_role_info_without_description() {
        let info = RoleInfo {
            id: 2,
            name: "viewer".to_string(),
            description: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"name\":\"viewer\""));
        assert!(json.contains("\"description\":null"));
    }

    // ==========================================================================
    // RoleResponse Tests
    // ==========================================================================

    #[test]
    fn test_role_response_serialize() {
        let now = Utc::now();
        let response = RoleResponse {
            id: 1,
            name: "admin".to_string(),
            description: Some("Administrator".to_string()),
            is_system: true,
            created_at: now,
            app_permissions: vec!["sonarr".to_string(), "radarr".to_string()],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"name\":\"admin\""));
        assert!(json.contains("\"is_system\":true"));
        assert!(json.contains("\"sonarr\""));
        assert!(json.contains("\"radarr\""));
    }

    // ==========================================================================
    // InviteResponse Tests
    // ==========================================================================

    #[test]
    fn test_invite_response_serialize() {
        let now = Utc::now();
        let response = InviteResponse {
            id: 1,
            code: "abc123".to_string(),
            created_by_id: 1,
            created_by_username: "admin".to_string(),
            used_by_id: None,
            used_by_username: None,
            is_used: false,
            expires_at: Some(now),
            created_at: now,
            used_at: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"code\":\"abc123\""));
        assert!(json.contains("\"is_used\":false"));
        assert!(json.contains("\"used_by_id\":null"));
    }

    #[test]
    fn test_invite_response_used() {
        let now = Utc::now();
        let response = InviteResponse {
            id: 1,
            code: "abc123".to_string(),
            created_by_id: 1,
            created_by_username: "admin".to_string(),
            used_by_id: Some(2),
            used_by_username: Some("newuser".to_string()),
            is_used: true,
            expires_at: Some(now),
            created_at: now,
            used_at: Some(now),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"is_used\":true"));
        assert!(json.contains("\"used_by_id\":2"));
        assert!(json.contains("\"used_by_username\":\"newuser\""));
    }

    // ==========================================================================
    // Clone Tests
    // ==========================================================================

    #[test]
    fn test_dto_clone() {
        let user = CreateUser {
            username: "test".to_string(),
            email: "test@example.com".to_string(),
            password: "pass".to_string(),
            role_ids: vec![1, 2],
        };
        let cloned = user.clone();
        assert_eq!(user.username, cloned.username);
        assert_eq!(user.role_ids, cloned.role_ids);

        let role = CreateRole {
            name: "test".to_string(),
            description: Some("desc".to_string()),
            app_names: vec!["app1".to_string()],
        };
        let cloned = role.clone();
        assert_eq!(role.name, cloned.name);
    }
}
