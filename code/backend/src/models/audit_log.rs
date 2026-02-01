use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize, utoipa::ToSchema)]
#[sea_orm(table_name = "audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[schema(value_type = String)]
    pub timestamp: DateTimeUtc,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: Option<String>, // JSON string for flexible data
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

// Audit action types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAction {
    // Authentication
    Login,
    LoginFailed,
    Logout,
    TokenRefresh,
    TwoFactorEnabled,
    TwoFactorDisabled,
    TwoFactorVerified,
    TwoFactorFailed,
    PasswordChanged,

    // User management
    UserCreated,
    UserUpdated,
    UserDeleted,
    UserApproved,
    UserDeactivated,
    UserActivated,

    // Role management
    RoleCreated,
    RoleUpdated,
    RoleDeleted,
    RoleAssigned,
    RoleUnassigned,

    // App management
    AppInstalled,
    AppUninstalled,
    AppStarted,
    AppStopped,
    AppRestarted,
    AppConfigured,
    AppAccessed,

    // System
    SystemSettingChanged,
    InviteCreated,
    InviteUsed,
    InviteDeleted,

    // API access
    ApiAccess,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditAction::Login => write!(f, "login"),
            AuditAction::LoginFailed => write!(f, "login_failed"),
            AuditAction::Logout => write!(f, "logout"),
            AuditAction::TokenRefresh => write!(f, "token_refresh"),
            AuditAction::TwoFactorEnabled => write!(f, "2fa_enabled"),
            AuditAction::TwoFactorDisabled => write!(f, "2fa_disabled"),
            AuditAction::TwoFactorVerified => write!(f, "2fa_verified"),
            AuditAction::TwoFactorFailed => write!(f, "2fa_failed"),
            AuditAction::PasswordChanged => write!(f, "password_changed"),
            AuditAction::UserCreated => write!(f, "user_created"),
            AuditAction::UserUpdated => write!(f, "user_updated"),
            AuditAction::UserDeleted => write!(f, "user_deleted"),
            AuditAction::UserApproved => write!(f, "user_approved"),
            AuditAction::UserDeactivated => write!(f, "user_deactivated"),
            AuditAction::UserActivated => write!(f, "user_activated"),
            AuditAction::RoleCreated => write!(f, "role_created"),
            AuditAction::RoleUpdated => write!(f, "role_updated"),
            AuditAction::RoleDeleted => write!(f, "role_deleted"),
            AuditAction::RoleAssigned => write!(f, "role_assigned"),
            AuditAction::RoleUnassigned => write!(f, "role_unassigned"),
            AuditAction::AppInstalled => write!(f, "app_installed"),
            AuditAction::AppUninstalled => write!(f, "app_uninstalled"),
            AuditAction::AppStarted => write!(f, "app_started"),
            AuditAction::AppStopped => write!(f, "app_stopped"),
            AuditAction::AppRestarted => write!(f, "app_restarted"),
            AuditAction::AppConfigured => write!(f, "app_configured"),
            AuditAction::AppAccessed => write!(f, "app_accessed"),
            AuditAction::SystemSettingChanged => write!(f, "system_setting_changed"),
            AuditAction::InviteCreated => write!(f, "invite_created"),
            AuditAction::InviteUsed => write!(f, "invite_used"),
            AuditAction::InviteDeleted => write!(f, "invite_deleted"),
            AuditAction::ApiAccess => write!(f, "api_access"),
        }
    }
}

// Resource types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    User,
    Role,
    App,
    System,
    Invite,
    Session,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::User => write!(f, "user"),
            ResourceType::Role => write!(f, "role"),
            ResourceType::App => write!(f, "app"),
            ResourceType::System => write!(f, "system"),
            ResourceType::Invite => write!(f, "invite"),
            ResourceType::Session => write!(f, "session"),
        }
    }
}
