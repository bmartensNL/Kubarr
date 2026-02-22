#![allow(dead_code)]

mod email;
mod messagebird;
mod telegram;

pub use email::EmailProvider;
pub use messagebird::MessageBirdProvider;
pub use telegram::TelegramProvider;

use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{AppError, Result};
use crate::models::{
    audit_log::AuditAction, notification_channel, notification_event, notification_log,
    user_notification, user_notification_pref,
};

/// Notification channel types
///
/// Note: Signal was previously listed here as a stub but had no implementation
/// and would always fail with "Channel signal not configured". It has been
/// removed. To add Signal support in the future, implement a `SignalProvider`
/// using the signal-cli REST API (https://github.com/bbernhard/signal-cli-rest-api)
/// and add a `KUBARR_SIGNAL_CLI_URL` config option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Email,
    Telegram,
    MessageBird,
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::Email => "email",
            ChannelType::Telegram => "telegram",
            ChannelType::MessageBird => "messagebird",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "email" => Some(ChannelType::Email),
            "telegram" => Some(ChannelType::Telegram),
            "messagebird" => Some(ChannelType::MessageBird),
            _ => None,
        }
    }

    pub fn all() -> Vec<ChannelType> {
        vec![
            ChannelType::Email,
            ChannelType::Telegram,
            ChannelType::MessageBird,
        ]
    }
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Notification message to send
#[derive(Debug, Clone)]
pub struct NotificationMessage {
    pub recipient: String,
    pub title: String,
    pub body: String,
    pub severity: NotificationSeverity,
}

/// Notification severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationSeverity {
    Info,
    Warning,
    Critical,
}

impl NotificationSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationSeverity::Info => "info",
            NotificationSeverity::Warning => "warning",
            NotificationSeverity::Critical => "critical",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "warning" => NotificationSeverity::Warning,
            "critical" => NotificationSeverity::Critical,
            _ => NotificationSeverity::Info,
        }
    }
}

impl std::fmt::Display for NotificationSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Result of sending a notification
#[derive(Debug)]
pub struct SendResult {
    pub success: bool,
    pub error: Option<String>,
}

/// Trait for notification providers
#[async_trait]
pub trait NotificationProvider: Send + Sync {
    fn channel_type(&self) -> ChannelType;
    async fn send(&self, message: &NotificationMessage) -> SendResult;
    async fn test(&self, destination: &str) -> SendResult;
}

/// Notification service that manages all notification channels
pub struct NotificationService {
    db: Arc<RwLock<Option<DatabaseConnection>>>,
    email: Arc<RwLock<Option<EmailProvider>>>,
    telegram: Arc<RwLock<Option<TelegramProvider>>>,
    messagebird: Arc<RwLock<Option<MessageBirdProvider>>>,
}

impl NotificationService {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(None)),
            email: Arc::new(RwLock::new(None)),
            telegram: Arc::new(RwLock::new(None)),
            messagebird: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn set_db(&self, db: DatabaseConnection) {
        let mut db_lock = self.db.write().await;
        *db_lock = Some(db);
    }

    /// Initialize providers from database configuration
    pub async fn init_providers(&self) -> Result<()> {
        let db_lock = self.db.read().await;
        let db = db_lock
            .as_ref()
            .ok_or_else(|| AppError::Internal("Database not initialized".to_string()))?;

        // Load channel configurations
        let channels = notification_channel::Entity::find()
            .filter(notification_channel::Column::Enabled.eq(true))
            .all(db)
            .await?;

        for channel in channels {
            let config: serde_json::Value =
                serde_json::from_str(&channel.config).unwrap_or(serde_json::json!({}));

            match channel.channel_type.as_str() {
                "email" => {
                    if let Ok(provider) = EmailProvider::from_config(&config) {
                        let mut email_lock = self.email.write().await;
                        *email_lock = Some(provider);
                        tracing::info!("Email notification provider initialized");
                    }
                }
                "telegram" => {
                    if let Ok(provider) = TelegramProvider::from_config(&config) {
                        let mut telegram_lock = self.telegram.write().await;
                        *telegram_lock = Some(provider);
                        tracing::info!("Telegram notification provider initialized");
                    }
                }
                "messagebird" => {
                    if let Ok(provider) = MessageBirdProvider::from_config(&config) {
                        let mut messagebird_lock = self.messagebird.write().await;
                        *messagebird_lock = Some(provider);
                        tracing::info!("MessageBird notification provider initialized");
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Send a notification for an audit event
    pub async fn notify_event(
        &self,
        action: &AuditAction,
        user_id: Option<i64>,
        username: Option<&str>,
        details: Option<&str>,
    ) -> Result<()> {
        let db_lock = self.db.read().await;
        let db = match db_lock.as_ref() {
            Some(db) => db,
            None => return Ok(()), // Silently skip if DB not initialized
        };

        let event_type = action.to_string();

        // Check if this event type is enabled for notifications
        let event_setting = notification_event::Entity::find()
            .filter(notification_event::Column::EventType.eq(&event_type))
            .one(db)
            .await?;

        let (enabled, severity) = match event_setting {
            Some(setting) => (
                setting.enabled,
                NotificationSeverity::parse(&setting.severity),
            ),
            None => return Ok(()), // Event not configured, skip
        };

        if !enabled {
            return Ok(());
        }

        // Create notification title and body
        let title = format_event_title(action);
        let body = format_event_body(action, username, details);

        // Create in-app notification for all users or specific user
        if let Some(uid) = user_id {
            self.create_user_notification(db, uid, &title, &body, &event_type, severity)
                .await?;
        } else {
            // For system-wide events, notify all admin users
            // (simplified: just log for now, can be extended)
            tracing::debug!("System notification: {} - {}", title, body);
        }

        // Send external notifications
        self.send_external_notifications(db, user_id, &title, &body, &event_type, severity)
            .await?;

        Ok(())
    }

    /// Create an in-app notification for a user
    async fn create_user_notification(
        &self,
        db: &DatabaseConnection,
        user_id: i64,
        title: &str,
        message: &str,
        event_type: &str,
        severity: NotificationSeverity,
    ) -> Result<()> {
        let notification = user_notification::ActiveModel {
            user_id: Set(user_id),
            title: Set(title.to_string()),
            message: Set(message.to_string()),
            event_type: Set(Some(event_type.to_string())),
            severity: Set(severity.as_str().to_string()),
            read: Set(false),
            created_at: Set(chrono::Utc::now()),
            ..Default::default()
        };
        notification.insert(db).await?;
        Ok(())
    }

    /// Send notifications through external channels
    async fn send_external_notifications(
        &self,
        db: &DatabaseConnection,
        user_id: Option<i64>,
        title: &str,
        body: &str,
        event_type: &str,
        severity: NotificationSeverity,
    ) -> Result<()> {
        // If we have a specific user, check their preferences
        if let Some(uid) = user_id {
            let prefs = user_notification_pref::Entity::find()
                .filter(user_notification_pref::Column::UserId.eq(uid))
                .filter(user_notification_pref::Column::Enabled.eq(true))
                .filter(user_notification_pref::Column::Verified.eq(true))
                .all(db)
                .await?;

            for pref in prefs {
                if let Some(destination) = &pref.destination {
                    let message = NotificationMessage {
                        recipient: destination.clone(),
                        title: title.to_string(),
                        body: body.to_string(),
                        severity,
                    };

                    let result = self.send_to_channel(&pref.channel_type, &message).await;
                    self.log_notification(
                        db,
                        Some(uid),
                        &pref.channel_type,
                        event_type,
                        destination,
                        &result,
                    )
                    .await?;
                }
            }
        }

        Ok(())
    }

    /// Send a message to a specific channel
    async fn send_to_channel(
        &self,
        channel_type: &str,
        message: &NotificationMessage,
    ) -> SendResult {
        match channel_type {
            "email" => {
                let email_lock = self.email.read().await;
                if let Some(provider) = email_lock.as_ref() {
                    return provider.send(message).await;
                }
            }
            "telegram" => {
                let telegram_lock = self.telegram.read().await;
                if let Some(provider) = telegram_lock.as_ref() {
                    return provider.send(message).await;
                }
            }
            "messagebird" => {
                let messagebird_lock = self.messagebird.read().await;
                if let Some(provider) = messagebird_lock.as_ref() {
                    return provider.send(message).await;
                }
            }
            _ => {}
        }

        SendResult {
            success: false,
            error: Some(format!("Channel {} not configured", channel_type)),
        }
    }

    /// Log a notification delivery attempt
    async fn log_notification(
        &self,
        db: &DatabaseConnection,
        user_id: Option<i64>,
        channel_type: &str,
        event_type: &str,
        recipient: &str,
        result: &SendResult,
    ) -> Result<()> {
        let log = notification_log::ActiveModel {
            user_id: Set(user_id),
            channel_type: Set(channel_type.to_string()),
            event_type: Set(event_type.to_string()),
            recipient: Set(Some(mask_recipient(recipient))),
            status: Set(if result.success {
                "sent".to_string()
            } else {
                "failed".to_string()
            }),
            error_message: Set(result.error.clone()),
            created_at: Set(chrono::Utc::now()),
            ..Default::default()
        };
        log.insert(db).await?;
        Ok(())
    }

    /// Test a notification channel
    pub async fn test_channel(&self, channel_type: &str, destination: &str) -> SendResult {
        match channel_type {
            "email" => {
                let email_lock = self.email.read().await;
                if let Some(provider) = email_lock.as_ref() {
                    return provider.test(destination).await;
                }
            }
            "telegram" => {
                let telegram_lock = self.telegram.read().await;
                if let Some(provider) = telegram_lock.as_ref() {
                    return provider.test(destination).await;
                }
            }
            "messagebird" => {
                let messagebird_lock = self.messagebird.read().await;
                if let Some(provider) = messagebird_lock.as_ref() {
                    return provider.test(destination).await;
                }
            }
            _ => {}
        }

        SendResult {
            success: false,
            error: Some(format!("Channel {} not configured", channel_type)),
        }
    }

    /// Get unread notification count for a user
    pub async fn get_unread_count(&self, user_id: i64) -> Result<u64> {
        let db_lock = self.db.read().await;
        let db = db_lock
            .as_ref()
            .ok_or_else(|| AppError::Internal("Database not initialized".to_string()))?;

        let count = user_notification::Entity::find()
            .filter(user_notification::Column::UserId.eq(user_id))
            .filter(user_notification::Column::Read.eq(false))
            .count(db)
            .await?;

        Ok(count)
    }

    /// Get notifications for a user
    pub async fn get_user_notifications(
        &self,
        user_id: i64,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<user_notification::Model>> {
        let db_lock = self.db.read().await;
        let db = db_lock
            .as_ref()
            .ok_or_else(|| AppError::Internal("Database not initialized".to_string()))?;

        let notifications = user_notification::Entity::find()
            .filter(user_notification::Column::UserId.eq(user_id))
            .order_by_desc(user_notification::Column::CreatedAt)
            .offset(offset)
            .limit(limit)
            .all(db)
            .await?;

        Ok(notifications)
    }

    /// Mark a notification as read
    pub async fn mark_as_read(&self, notification_id: i64, user_id: i64) -> Result<()> {
        let db_lock = self.db.read().await;
        let db = db_lock
            .as_ref()
            .ok_or_else(|| AppError::Internal("Database not initialized".to_string()))?;

        let notification = user_notification::Entity::find_by_id(notification_id)
            .filter(user_notification::Column::UserId.eq(user_id))
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

        let mut active: user_notification::ActiveModel = notification.into();
        active.read = Set(true);
        active.update(db).await?;

        Ok(())
    }

    /// Mark all notifications as read for a user
    pub async fn mark_all_as_read(&self, user_id: i64) -> Result<()> {
        let db_lock = self.db.read().await;
        let db = db_lock
            .as_ref()
            .ok_or_else(|| AppError::Internal("Database not initialized".to_string()))?;

        user_notification::Entity::update_many()
            .filter(user_notification::Column::UserId.eq(user_id))
            .filter(user_notification::Column::Read.eq(false))
            .col_expr(
                user_notification::Column::Read,
                sea_orm::sea_query::Expr::value(true),
            )
            .exec(db)
            .await?;

        Ok(())
    }

    /// Delete a notification
    pub async fn delete_notification(&self, notification_id: i64, user_id: i64) -> Result<()> {
        let db_lock = self.db.read().await;
        let db = db_lock
            .as_ref()
            .ok_or_else(|| AppError::Internal("Database not initialized".to_string()))?;

        let result = user_notification::Entity::delete_many()
            .filter(user_notification::Column::Id.eq(notification_id))
            .filter(user_notification::Column::UserId.eq(user_id))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return Err(AppError::NotFound("Notification not found".to_string()));
        }

        Ok(())
    }
}

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for NotificationService {
    fn clone(&self) -> Self {
        Self {
            db: Arc::clone(&self.db),
            email: Arc::clone(&self.email),
            telegram: Arc::clone(&self.telegram),
            messagebird: Arc::clone(&self.messagebird),
        }
    }
}

/// Format a human-readable title for an audit event
fn format_event_title(action: &AuditAction) -> String {
    match action {
        // Authentication
        AuditAction::Login => "Login Successful".to_string(),
        AuditAction::LoginFailed => "Login Failed".to_string(),
        AuditAction::Logout => "Logged Out".to_string(),
        AuditAction::TokenRefresh => "Session Token Refreshed".to_string(),
        AuditAction::TwoFactorEnabled => "2FA Enabled".to_string(),
        AuditAction::TwoFactorDisabled => "2FA Disabled".to_string(),
        AuditAction::TwoFactorVerified => "2FA Verification Successful".to_string(),
        AuditAction::TwoFactorFailed => "2FA Verification Failed".to_string(),
        AuditAction::PasswordChanged => "Password Changed".to_string(),
        // User management
        AuditAction::UserCreated => "New User Created".to_string(),
        AuditAction::UserUpdated => "User Updated".to_string(),
        AuditAction::UserDeleted => "User Deleted".to_string(),
        AuditAction::UserApproved => "User Approved".to_string(),
        AuditAction::UserDeactivated => "User Deactivated".to_string(),
        AuditAction::UserActivated => "User Activated".to_string(),
        // Role management
        AuditAction::RoleCreated => "Role Created".to_string(),
        AuditAction::RoleUpdated => "Role Updated".to_string(),
        AuditAction::RoleDeleted => "Role Deleted".to_string(),
        AuditAction::RoleAssigned => "Role Assigned".to_string(),
        AuditAction::RoleUnassigned => "Role Unassigned".to_string(),
        // App management
        AuditAction::AppInstalled => "App Installed".to_string(),
        AuditAction::AppUninstalled => "App Uninstalled".to_string(),
        AuditAction::AppStarted => "App Started".to_string(),
        AuditAction::AppStopped => "App Stopped".to_string(),
        AuditAction::AppRestarted => "App Restarted".to_string(),
        AuditAction::AppConfigured => "App Configured".to_string(),
        AuditAction::AppAccessed => "App Accessed".to_string(),
        // System
        AuditAction::SystemSettingChanged => "System Setting Changed".to_string(),
        AuditAction::InviteCreated => "Invite Link Created".to_string(),
        AuditAction::InviteUsed => "Invite Link Used".to_string(),
        AuditAction::InviteDeleted => "Invite Link Deleted".to_string(),
        // API
        AuditAction::ApiAccess => "API Access".to_string(),
    }
}

/// Format a human-readable body for an audit event
fn format_event_body(
    action: &AuditAction,
    username: Option<&str>,
    details: Option<&str>,
) -> String {
    let user = username.unwrap_or("Unknown");
    let detail = details.unwrap_or("");

    match action {
        // Authentication
        AuditAction::Login => format!("User {} logged in successfully", user),
        AuditAction::LoginFailed => format!("Failed login attempt for user {}", user),
        AuditAction::Logout => format!("User {} logged out", user),
        AuditAction::TokenRefresh => format!("Session token refreshed for user {}", user),
        AuditAction::TwoFactorEnabled => {
            format!("User {} enabled two-factor authentication", user)
        }
        AuditAction::TwoFactorDisabled => {
            format!("User {} disabled two-factor authentication", user)
        }
        AuditAction::TwoFactorVerified => {
            format!("User {} successfully verified 2FA code", user)
        }
        AuditAction::TwoFactorFailed => {
            format!("User {} failed 2FA verification", user)
        }
        AuditAction::PasswordChanged => format!("User {} changed their password", user),
        // User management
        AuditAction::UserCreated => {
            if detail.is_empty() {
                format!("New user account created by {}", user)
            } else {
                format!("New user account created by {}: {}", user, detail)
            }
        }
        AuditAction::UserUpdated => {
            if detail.is_empty() {
                format!("User account updated by {}", user)
            } else {
                format!("User account updated by {}: {}", user, detail)
            }
        }
        AuditAction::UserDeleted => {
            if detail.is_empty() {
                format!("User account deleted by {}", user)
            } else {
                format!("User account deleted by {}: {}", user, detail)
            }
        }
        AuditAction::UserApproved => {
            if detail.is_empty() {
                format!("User account approved by {}", user)
            } else {
                format!("User {} approved by {}", detail, user)
            }
        }
        AuditAction::UserDeactivated => {
            if detail.is_empty() {
                format!("User account deactivated by {}", user)
            } else {
                format!("User {} deactivated by {}", detail, user)
            }
        }
        AuditAction::UserActivated => {
            if detail.is_empty() {
                format!("User account activated by {}", user)
            } else {
                format!("User {} activated by {}", detail, user)
            }
        }
        // Role management
        AuditAction::RoleCreated => {
            if detail.is_empty() {
                format!("New role created by {}", user)
            } else {
                format!("New role created by {}: {}", user, detail)
            }
        }
        AuditAction::RoleUpdated => {
            if detail.is_empty() {
                format!("Role updated by {}", user)
            } else {
                format!("Role updated by {}: {}", user, detail)
            }
        }
        AuditAction::RoleDeleted => {
            if detail.is_empty() {
                format!("Role deleted by {}", user)
            } else {
                format!("Role deleted by {}: {}", user, detail)
            }
        }
        AuditAction::RoleAssigned => {
            if detail.is_empty() {
                format!("Role assigned by {}", user)
            } else {
                format!("Role assigned by {}: {}", user, detail)
            }
        }
        AuditAction::RoleUnassigned => {
            if detail.is_empty() {
                format!("Role unassigned by {}", user)
            } else {
                format!("Role unassigned by {}: {}", user, detail)
            }
        }
        // App management
        AuditAction::AppInstalled => {
            if detail.is_empty() {
                format!("App installed by {}", user)
            } else {
                format!("App installed by {}: {}", user, detail)
            }
        }
        AuditAction::AppUninstalled => {
            if detail.is_empty() {
                format!("App uninstalled by {}", user)
            } else {
                format!("App uninstalled by {}: {}", user, detail)
            }
        }
        AuditAction::AppStarted => {
            if detail.is_empty() {
                format!("App started by {}", user)
            } else {
                format!("App started by {}: {}", user, detail)
            }
        }
        AuditAction::AppStopped => {
            if detail.is_empty() {
                format!("App stopped by {}", user)
            } else {
                format!("App stopped by {}: {}", user, detail)
            }
        }
        AuditAction::AppRestarted => {
            if detail.is_empty() {
                format!("App restarted by {}", user)
            } else {
                format!("App restarted by {}: {}", user, detail)
            }
        }
        AuditAction::AppConfigured => {
            if detail.is_empty() {
                format!("App configuration changed by {}", user)
            } else {
                format!("App configuration changed by {}: {}", user, detail)
            }
        }
        AuditAction::AppAccessed => {
            if detail.is_empty() {
                format!("App accessed by {}", user)
            } else {
                format!("User {} accessed {}", user, detail)
            }
        }
        // System
        AuditAction::SystemSettingChanged => {
            if detail.is_empty() {
                format!("System setting changed by {}", user)
            } else {
                format!("System setting changed by {}: {}", user, detail)
            }
        }
        AuditAction::InviteCreated => {
            if detail.is_empty() {
                format!("Invite link created by {}", user)
            } else {
                format!("Invite link created by {}: {}", user, detail)
            }
        }
        AuditAction::InviteUsed => {
            if detail.is_empty() {
                format!("Invite link used by {}", user)
            } else {
                format!("Invite link used by {}: {}", user, detail)
            }
        }
        AuditAction::InviteDeleted => {
            if detail.is_empty() {
                format!("Invite link deleted by {}", user)
            } else {
                format!("Invite link deleted by {}: {}", user, detail)
            }
        }
        // API
        AuditAction::ApiAccess => {
            if detail.is_empty() {
                format!("API accessed by {}", user)
            } else {
                format!("API accessed by {}: {}", user, detail)
            }
        }
    }
}

/// Mask a recipient for logging (privacy)
fn mask_recipient(recipient: &str) -> String {
    if recipient.contains('@') {
        // Email: show first 2 chars and domain
        let parts: Vec<&str> = recipient.split('@').collect();
        if parts.len() == 2 && parts[0].len() > 2 {
            format!("{}***@{}", &parts[0][..2], parts[1])
        } else {
            "***@***".to_string()
        }
    } else if recipient.starts_with('+') {
        // Phone: show country code and last 2 digits
        if recipient.len() > 4 {
            format!(
                "{}***{}",
                &recipient[..3],
                &recipient[recipient.len() - 2..]
            )
        } else {
            "+***".to_string()
        }
    } else {
        // Other (e.g., Telegram chat ID): show first 3 and last 2
        if recipient.len() > 5 {
            format!(
                "{}***{}",
                &recipient[..3],
                &recipient[recipient.len() - 2..]
            )
        } else {
            "***".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::audit_log::AuditAction;

    // -------------------------------------------------------------------------
    // format_event_title tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_event_title_login() {
        assert_eq!(format_event_title(&AuditAction::Login), "Login Successful");
    }

    #[test]
    fn test_format_event_title_login_failed() {
        assert_eq!(
            format_event_title(&AuditAction::LoginFailed),
            "Login Failed"
        );
    }

    #[test]
    fn test_format_event_title_logout() {
        assert_eq!(format_event_title(&AuditAction::Logout), "Logged Out");
    }

    #[test]
    fn test_format_event_title_token_refresh() {
        assert_eq!(
            format_event_title(&AuditAction::TokenRefresh),
            "Session Token Refreshed"
        );
    }

    #[test]
    fn test_format_event_title_two_factor_enabled() {
        assert_eq!(
            format_event_title(&AuditAction::TwoFactorEnabled),
            "2FA Enabled"
        );
    }

    #[test]
    fn test_format_event_title_two_factor_disabled() {
        assert_eq!(
            format_event_title(&AuditAction::TwoFactorDisabled),
            "2FA Disabled"
        );
    }

    #[test]
    fn test_format_event_title_two_factor_verified() {
        assert_eq!(
            format_event_title(&AuditAction::TwoFactorVerified),
            "2FA Verification Successful"
        );
    }

    #[test]
    fn test_format_event_title_two_factor_failed() {
        assert_eq!(
            format_event_title(&AuditAction::TwoFactorFailed),
            "2FA Verification Failed"
        );
    }

    #[test]
    fn test_format_event_title_password_changed() {
        assert_eq!(
            format_event_title(&AuditAction::PasswordChanged),
            "Password Changed"
        );
    }

    #[test]
    fn test_format_event_title_user_created() {
        assert_eq!(
            format_event_title(&AuditAction::UserCreated),
            "New User Created"
        );
    }

    #[test]
    fn test_format_event_title_user_updated() {
        assert_eq!(
            format_event_title(&AuditAction::UserUpdated),
            "User Updated"
        );
    }

    #[test]
    fn test_format_event_title_user_deleted() {
        assert_eq!(
            format_event_title(&AuditAction::UserDeleted),
            "User Deleted"
        );
    }

    #[test]
    fn test_format_event_title_user_approved() {
        assert_eq!(
            format_event_title(&AuditAction::UserApproved),
            "User Approved"
        );
    }

    #[test]
    fn test_format_event_title_user_deactivated() {
        assert_eq!(
            format_event_title(&AuditAction::UserDeactivated),
            "User Deactivated"
        );
    }

    #[test]
    fn test_format_event_title_user_activated() {
        assert_eq!(
            format_event_title(&AuditAction::UserActivated),
            "User Activated"
        );
    }

    #[test]
    fn test_format_event_title_role_created() {
        assert_eq!(
            format_event_title(&AuditAction::RoleCreated),
            "Role Created"
        );
    }

    #[test]
    fn test_format_event_title_role_updated() {
        assert_eq!(
            format_event_title(&AuditAction::RoleUpdated),
            "Role Updated"
        );
    }

    #[test]
    fn test_format_event_title_role_deleted() {
        assert_eq!(
            format_event_title(&AuditAction::RoleDeleted),
            "Role Deleted"
        );
    }

    #[test]
    fn test_format_event_title_role_assigned() {
        assert_eq!(
            format_event_title(&AuditAction::RoleAssigned),
            "Role Assigned"
        );
    }

    #[test]
    fn test_format_event_title_role_unassigned() {
        assert_eq!(
            format_event_title(&AuditAction::RoleUnassigned),
            "Role Unassigned"
        );
    }

    #[test]
    fn test_format_event_title_app_installed() {
        assert_eq!(
            format_event_title(&AuditAction::AppInstalled),
            "App Installed"
        );
    }

    #[test]
    fn test_format_event_title_app_uninstalled() {
        assert_eq!(
            format_event_title(&AuditAction::AppUninstalled),
            "App Uninstalled"
        );
    }

    #[test]
    fn test_format_event_title_app_started() {
        assert_eq!(format_event_title(&AuditAction::AppStarted), "App Started");
    }

    #[test]
    fn test_format_event_title_app_stopped() {
        assert_eq!(format_event_title(&AuditAction::AppStopped), "App Stopped");
    }

    #[test]
    fn test_format_event_title_app_restarted() {
        assert_eq!(
            format_event_title(&AuditAction::AppRestarted),
            "App Restarted"
        );
    }

    #[test]
    fn test_format_event_title_app_configured() {
        assert_eq!(
            format_event_title(&AuditAction::AppConfigured),
            "App Configured"
        );
    }

    #[test]
    fn test_format_event_title_app_accessed() {
        assert_eq!(
            format_event_title(&AuditAction::AppAccessed),
            "App Accessed"
        );
    }

    #[test]
    fn test_format_event_title_system_setting_changed() {
        assert_eq!(
            format_event_title(&AuditAction::SystemSettingChanged),
            "System Setting Changed"
        );
    }

    #[test]
    fn test_format_event_title_invite_created() {
        assert_eq!(
            format_event_title(&AuditAction::InviteCreated),
            "Invite Link Created"
        );
    }

    #[test]
    fn test_format_event_title_invite_used() {
        assert_eq!(
            format_event_title(&AuditAction::InviteUsed),
            "Invite Link Used"
        );
    }

    #[test]
    fn test_format_event_title_invite_deleted() {
        assert_eq!(
            format_event_title(&AuditAction::InviteDeleted),
            "Invite Link Deleted"
        );
    }

    #[test]
    fn test_format_event_title_api_access() {
        assert_eq!(format_event_title(&AuditAction::ApiAccess), "API Access");
    }

    // -------------------------------------------------------------------------
    // format_event_body tests
    // -------------------------------------------------------------------------

    // Authentication variants — these do NOT branch on details; username is used.

    #[test]
    fn test_format_event_body_login_with_username() {
        let body = format_event_body(&AuditAction::Login, Some("alice"), None);
        assert_eq!(body, "User alice logged in successfully");
    }

    #[test]
    fn test_format_event_body_login_without_username() {
        let body = format_event_body(&AuditAction::Login, None, None);
        assert_eq!(body, "User Unknown logged in successfully");
    }

    #[test]
    fn test_format_event_body_login_failed_with_username() {
        let body = format_event_body(&AuditAction::LoginFailed, Some("bob"), None);
        assert_eq!(body, "Failed login attempt for user bob");
    }

    #[test]
    fn test_format_event_body_logout_with_username() {
        let body = format_event_body(&AuditAction::Logout, Some("alice"), None);
        assert_eq!(body, "User alice logged out");
    }

    #[test]
    fn test_format_event_body_token_refresh_with_username() {
        let body = format_event_body(&AuditAction::TokenRefresh, Some("alice"), None);
        assert_eq!(body, "Session token refreshed for user alice");
    }

    #[test]
    fn test_format_event_body_two_factor_enabled() {
        let body = format_event_body(&AuditAction::TwoFactorEnabled, Some("alice"), None);
        assert_eq!(body, "User alice enabled two-factor authentication");
    }

    #[test]
    fn test_format_event_body_two_factor_disabled() {
        let body = format_event_body(&AuditAction::TwoFactorDisabled, Some("alice"), None);
        assert_eq!(body, "User alice disabled two-factor authentication");
    }

    #[test]
    fn test_format_event_body_two_factor_verified() {
        let body = format_event_body(&AuditAction::TwoFactorVerified, Some("alice"), None);
        assert_eq!(body, "User alice successfully verified 2FA code");
    }

    #[test]
    fn test_format_event_body_two_factor_failed() {
        let body = format_event_body(&AuditAction::TwoFactorFailed, Some("alice"), None);
        assert_eq!(body, "User alice failed 2FA verification");
    }

    #[test]
    fn test_format_event_body_password_changed() {
        let body = format_event_body(&AuditAction::PasswordChanged, Some("alice"), None);
        assert_eq!(body, "User alice changed their password");
    }

    // Variants that branch on detail presence (with and without details).

    #[test]
    fn test_format_event_body_user_created_no_detail() {
        let body = format_event_body(&AuditAction::UserCreated, Some("admin"), None);
        assert_eq!(body, "New user account created by admin");
    }

    #[test]
    fn test_format_event_body_user_created_with_detail() {
        let body = format_event_body(&AuditAction::UserCreated, Some("admin"), Some("newuser"));
        assert_eq!(body, "New user account created by admin: newuser");
    }

    #[test]
    fn test_format_event_body_user_updated_no_detail() {
        let body = format_event_body(&AuditAction::UserUpdated, Some("admin"), None);
        assert_eq!(body, "User account updated by admin");
    }

    #[test]
    fn test_format_event_body_user_updated_with_detail() {
        let body = format_event_body(
            &AuditAction::UserUpdated,
            Some("admin"),
            Some("email changed"),
        );
        assert_eq!(body, "User account updated by admin: email changed");
    }

    #[test]
    fn test_format_event_body_user_deleted_no_detail() {
        let body = format_event_body(&AuditAction::UserDeleted, Some("admin"), None);
        assert_eq!(body, "User account deleted by admin");
    }

    #[test]
    fn test_format_event_body_user_deleted_with_detail() {
        let body = format_event_body(&AuditAction::UserDeleted, Some("admin"), Some("alice"));
        assert_eq!(body, "User account deleted by admin: alice");
    }

    #[test]
    fn test_format_event_body_user_approved_no_detail() {
        let body = format_event_body(&AuditAction::UserApproved, Some("admin"), None);
        assert_eq!(body, "User account approved by admin");
    }

    #[test]
    fn test_format_event_body_user_approved_with_detail() {
        // detail is the approved username
        let body = format_event_body(&AuditAction::UserApproved, Some("admin"), Some("alice"));
        assert_eq!(body, "User alice approved by admin");
    }

    #[test]
    fn test_format_event_body_user_deactivated_no_detail() {
        let body = format_event_body(&AuditAction::UserDeactivated, Some("admin"), None);
        assert_eq!(body, "User account deactivated by admin");
    }

    #[test]
    fn test_format_event_body_user_deactivated_with_detail() {
        let body = format_event_body(&AuditAction::UserDeactivated, Some("admin"), Some("alice"));
        assert_eq!(body, "User alice deactivated by admin");
    }

    #[test]
    fn test_format_event_body_user_activated_no_detail() {
        let body = format_event_body(&AuditAction::UserActivated, Some("admin"), None);
        assert_eq!(body, "User account activated by admin");
    }

    #[test]
    fn test_format_event_body_user_activated_with_detail() {
        let body = format_event_body(&AuditAction::UserActivated, Some("admin"), Some("alice"));
        assert_eq!(body, "User alice activated by admin");
    }

    #[test]
    fn test_format_event_body_role_created_no_detail() {
        let body = format_event_body(&AuditAction::RoleCreated, Some("admin"), None);
        assert_eq!(body, "New role created by admin");
    }

    #[test]
    fn test_format_event_body_role_created_with_detail() {
        let body = format_event_body(&AuditAction::RoleCreated, Some("admin"), Some("editor"));
        assert_eq!(body, "New role created by admin: editor");
    }

    #[test]
    fn test_format_event_body_role_updated_no_detail() {
        let body = format_event_body(&AuditAction::RoleUpdated, Some("admin"), None);
        assert_eq!(body, "Role updated by admin");
    }

    #[test]
    fn test_format_event_body_role_updated_with_detail() {
        let body = format_event_body(&AuditAction::RoleUpdated, Some("admin"), Some("editor"));
        assert_eq!(body, "Role updated by admin: editor");
    }

    #[test]
    fn test_format_event_body_role_deleted_no_detail() {
        let body = format_event_body(&AuditAction::RoleDeleted, Some("admin"), None);
        assert_eq!(body, "Role deleted by admin");
    }

    #[test]
    fn test_format_event_body_role_deleted_with_detail() {
        let body = format_event_body(&AuditAction::RoleDeleted, Some("admin"), Some("editor"));
        assert_eq!(body, "Role deleted by admin: editor");
    }

    #[test]
    fn test_format_event_body_role_assigned_no_detail() {
        let body = format_event_body(&AuditAction::RoleAssigned, Some("admin"), None);
        assert_eq!(body, "Role assigned by admin");
    }

    #[test]
    fn test_format_event_body_role_assigned_with_detail() {
        let body = format_event_body(
            &AuditAction::RoleAssigned,
            Some("admin"),
            Some("editor -> alice"),
        );
        assert_eq!(body, "Role assigned by admin: editor -> alice");
    }

    #[test]
    fn test_format_event_body_role_unassigned_no_detail() {
        let body = format_event_body(&AuditAction::RoleUnassigned, Some("admin"), None);
        assert_eq!(body, "Role unassigned by admin");
    }

    #[test]
    fn test_format_event_body_role_unassigned_with_detail() {
        let body = format_event_body(
            &AuditAction::RoleUnassigned,
            Some("admin"),
            Some("editor -> alice"),
        );
        assert_eq!(body, "Role unassigned by admin: editor -> alice");
    }

    #[test]
    fn test_format_event_body_app_installed_no_detail() {
        let body = format_event_body(&AuditAction::AppInstalled, Some("admin"), None);
        assert_eq!(body, "App installed by admin");
    }

    #[test]
    fn test_format_event_body_app_installed_with_detail() {
        let body = format_event_body(&AuditAction::AppInstalled, Some("admin"), Some("sonarr"));
        assert_eq!(body, "App installed by admin: sonarr");
    }

    #[test]
    fn test_format_event_body_app_uninstalled_no_detail() {
        let body = format_event_body(&AuditAction::AppUninstalled, Some("admin"), None);
        assert_eq!(body, "App uninstalled by admin");
    }

    #[test]
    fn test_format_event_body_app_uninstalled_with_detail() {
        let body = format_event_body(&AuditAction::AppUninstalled, Some("admin"), Some("sonarr"));
        assert_eq!(body, "App uninstalled by admin: sonarr");
    }

    #[test]
    fn test_format_event_body_app_started_no_detail() {
        let body = format_event_body(&AuditAction::AppStarted, Some("admin"), None);
        assert_eq!(body, "App started by admin");
    }

    #[test]
    fn test_format_event_body_app_started_with_detail() {
        let body = format_event_body(&AuditAction::AppStarted, Some("admin"), Some("radarr"));
        assert_eq!(body, "App started by admin: radarr");
    }

    #[test]
    fn test_format_event_body_app_stopped_no_detail() {
        let body = format_event_body(&AuditAction::AppStopped, Some("admin"), None);
        assert_eq!(body, "App stopped by admin");
    }

    #[test]
    fn test_format_event_body_app_stopped_with_detail() {
        let body = format_event_body(&AuditAction::AppStopped, Some("admin"), Some("radarr"));
        assert_eq!(body, "App stopped by admin: radarr");
    }

    #[test]
    fn test_format_event_body_app_restarted_no_detail() {
        let body = format_event_body(&AuditAction::AppRestarted, Some("admin"), None);
        assert_eq!(body, "App restarted by admin");
    }

    #[test]
    fn test_format_event_body_app_restarted_with_detail() {
        let body = format_event_body(&AuditAction::AppRestarted, Some("admin"), Some("radarr"));
        assert_eq!(body, "App restarted by admin: radarr");
    }

    #[test]
    fn test_format_event_body_app_configured_no_detail() {
        let body = format_event_body(&AuditAction::AppConfigured, Some("admin"), None);
        assert_eq!(body, "App configuration changed by admin");
    }

    #[test]
    fn test_format_event_body_app_configured_with_detail() {
        let body = format_event_body(&AuditAction::AppConfigured, Some("admin"), Some("sonarr"));
        assert_eq!(body, "App configuration changed by admin: sonarr");
    }

    #[test]
    fn test_format_event_body_app_accessed_no_detail() {
        let body = format_event_body(&AuditAction::AppAccessed, Some("alice"), None);
        assert_eq!(body, "App accessed by alice");
    }

    #[test]
    fn test_format_event_body_app_accessed_with_detail() {
        // Special format: "User {user} accessed {detail}"
        let body = format_event_body(&AuditAction::AppAccessed, Some("alice"), Some("sonarr"));
        assert_eq!(body, "User alice accessed sonarr");
    }

    #[test]
    fn test_format_event_body_system_setting_changed_no_detail() {
        let body = format_event_body(&AuditAction::SystemSettingChanged, Some("admin"), None);
        assert_eq!(body, "System setting changed by admin");
    }

    #[test]
    fn test_format_event_body_system_setting_changed_with_detail() {
        let body = format_event_body(
            &AuditAction::SystemSettingChanged,
            Some("admin"),
            Some("smtp_host"),
        );
        assert_eq!(body, "System setting changed by admin: smtp_host");
    }

    #[test]
    fn test_format_event_body_invite_created_no_detail() {
        let body = format_event_body(&AuditAction::InviteCreated, Some("admin"), None);
        assert_eq!(body, "Invite link created by admin");
    }

    #[test]
    fn test_format_event_body_invite_created_with_detail() {
        let body = format_event_body(&AuditAction::InviteCreated, Some("admin"), Some("abc123"));
        assert_eq!(body, "Invite link created by admin: abc123");
    }

    #[test]
    fn test_format_event_body_invite_used_no_detail() {
        let body = format_event_body(&AuditAction::InviteUsed, Some("alice"), None);
        assert_eq!(body, "Invite link used by alice");
    }

    #[test]
    fn test_format_event_body_invite_used_with_detail() {
        let body = format_event_body(&AuditAction::InviteUsed, Some("alice"), Some("abc123"));
        assert_eq!(body, "Invite link used by alice: abc123");
    }

    #[test]
    fn test_format_event_body_invite_deleted_no_detail() {
        let body = format_event_body(&AuditAction::InviteDeleted, Some("admin"), None);
        assert_eq!(body, "Invite link deleted by admin");
    }

    #[test]
    fn test_format_event_body_invite_deleted_with_detail() {
        let body = format_event_body(&AuditAction::InviteDeleted, Some("admin"), Some("abc123"));
        assert_eq!(body, "Invite link deleted by admin: abc123");
    }

    #[test]
    fn test_format_event_body_api_access_no_detail() {
        let body = format_event_body(&AuditAction::ApiAccess, Some("alice"), None);
        assert_eq!(body, "API accessed by alice");
    }

    #[test]
    fn test_format_event_body_api_access_with_detail() {
        let body = format_event_body(
            &AuditAction::ApiAccess,
            Some("alice"),
            Some("GET /api/apps"),
        );
        assert_eq!(body, "API accessed by alice: GET /api/apps");
    }

    #[test]
    fn test_format_event_body_no_username_no_detail() {
        // Username falls back to "Unknown", empty detail suppresses the detail clause
        let body = format_event_body(&AuditAction::AppInstalled, None, None);
        assert_eq!(body, "App installed by Unknown");
    }

    #[test]
    fn test_format_event_body_no_username_with_detail() {
        let body = format_event_body(&AuditAction::AppInstalled, None, Some("sonarr"));
        assert_eq!(body, "App installed by Unknown: sonarr");
    }

    // -------------------------------------------------------------------------
    // mask_recipient tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_mask_recipient_email_normal() {
        // Shows first 2 chars of local part + domain
        assert_eq!(mask_recipient("alice@example.com"), "al***@example.com");
    }

    #[test]
    fn test_mask_recipient_email_long_local() {
        assert_eq!(mask_recipient("jonathan@domain.org"), "jo***@domain.org");
    }

    #[test]
    fn test_mask_recipient_email_short_local_two_chars() {
        // local part length == 2 → falls through to "***@***"
        assert_eq!(mask_recipient("ab@example.com"), "***@***");
    }

    #[test]
    fn test_mask_recipient_email_short_local_one_char() {
        assert_eq!(mask_recipient("a@example.com"), "***@***");
    }

    #[test]
    fn test_mask_recipient_phone_normal() {
        // "+12345678" → "+12***78" (first 3 chars + *** + last 2)
        assert_eq!(mask_recipient("+12345678"), "+12***78");
    }

    #[test]
    fn test_mask_recipient_phone_long() {
        // "+14155551234" → "+14***34"
        assert_eq!(mask_recipient("+14155551234"), "+14***34");
    }

    #[test]
    fn test_mask_recipient_phone_short_four_chars() {
        // "+123" → len 4, not > 4 → "+***"
        assert_eq!(mask_recipient("+123"), "+***");
    }

    #[test]
    fn test_mask_recipient_phone_short_two_chars() {
        assert_eq!(mask_recipient("+1"), "+***");
    }

    #[test]
    fn test_mask_recipient_other_long() {
        // Telegram chat IDs or similar: "1234567890" (len > 5) → "123***90"
        assert_eq!(mask_recipient("1234567890"), "123***90");
    }

    #[test]
    fn test_mask_recipient_other_exactly_six_chars() {
        // len 6 > 5 → first 3 + *** + last 2
        assert_eq!(mask_recipient("abcdef"), "abc***ef");
    }

    #[test]
    fn test_mask_recipient_other_short_five_chars() {
        // len 5, not > 5 → "***"
        assert_eq!(mask_recipient("abcde"), "***");
    }

    #[test]
    fn test_mask_recipient_other_very_short() {
        assert_eq!(mask_recipient("ab"), "***");
    }

    // -------------------------------------------------------------------------
    // ChannelType tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_channel_type_as_str() {
        assert_eq!(ChannelType::Email.as_str(), "email");
        assert_eq!(ChannelType::Telegram.as_str(), "telegram");
        assert_eq!(ChannelType::MessageBird.as_str(), "messagebird");
    }

    #[test]
    fn test_channel_type_parse_valid() {
        assert_eq!(ChannelType::parse("email"), Some(ChannelType::Email));
        assert_eq!(ChannelType::parse("telegram"), Some(ChannelType::Telegram));
        assert_eq!(
            ChannelType::parse("messagebird"),
            Some(ChannelType::MessageBird)
        );
    }

    #[test]
    fn test_channel_type_parse_case_insensitive() {
        assert_eq!(ChannelType::parse("EMAIL"), Some(ChannelType::Email));
        assert_eq!(ChannelType::parse("Telegram"), Some(ChannelType::Telegram));
        assert_eq!(
            ChannelType::parse("MessageBird"),
            Some(ChannelType::MessageBird)
        );
    }

    #[test]
    fn test_channel_type_parse_unknown() {
        assert_eq!(ChannelType::parse("signal"), None);
        assert_eq!(ChannelType::parse(""), None);
        assert_eq!(ChannelType::parse("slack"), None);
    }

    #[test]
    fn test_channel_type_all_contains_all_variants() {
        let all = ChannelType::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&ChannelType::Email));
        assert!(all.contains(&ChannelType::Telegram));
        assert!(all.contains(&ChannelType::MessageBird));
    }

    #[test]
    fn test_channel_type_display() {
        assert_eq!(format!("{}", ChannelType::Email), "email");
        assert_eq!(format!("{}", ChannelType::Telegram), "telegram");
        assert_eq!(format!("{}", ChannelType::MessageBird), "messagebird");
    }

    // -------------------------------------------------------------------------
    // NotificationSeverity tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_notification_severity_as_str() {
        assert_eq!(NotificationSeverity::Info.as_str(), "info");
        assert_eq!(NotificationSeverity::Warning.as_str(), "warning");
        assert_eq!(NotificationSeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_notification_severity_parse_warning() {
        assert_eq!(
            NotificationSeverity::parse("warning"),
            NotificationSeverity::Warning
        );
    }

    #[test]
    fn test_notification_severity_parse_critical() {
        assert_eq!(
            NotificationSeverity::parse("critical"),
            NotificationSeverity::Critical
        );
    }

    #[test]
    fn test_notification_severity_parse_info() {
        assert_eq!(
            NotificationSeverity::parse("info"),
            NotificationSeverity::Info
        );
    }

    #[test]
    fn test_notification_severity_parse_case_insensitive() {
        assert_eq!(
            NotificationSeverity::parse("WARNING"),
            NotificationSeverity::Warning
        );
        assert_eq!(
            NotificationSeverity::parse("CRITICAL"),
            NotificationSeverity::Critical
        );
    }

    #[test]
    fn test_notification_severity_parse_unknown_defaults_to_info() {
        // Anything unknown falls back to Info
        assert_eq!(
            NotificationSeverity::parse("unknown"),
            NotificationSeverity::Info
        );
        assert_eq!(NotificationSeverity::parse(""), NotificationSeverity::Info);
        assert_eq!(
            NotificationSeverity::parse("debug"),
            NotificationSeverity::Info
        );
    }

    #[test]
    fn test_notification_severity_display() {
        assert_eq!(format!("{}", NotificationSeverity::Info), "info");
        assert_eq!(format!("{}", NotificationSeverity::Warning), "warning");
        assert_eq!(format!("{}", NotificationSeverity::Critical), "critical");
    }
}
