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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Email,
    Telegram,
    MessageBird,
    Signal,
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::Email => "email",
            ChannelType::Telegram => "telegram",
            ChannelType::MessageBird => "messagebird",
            ChannelType::Signal => "signal",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "email" => Some(ChannelType::Email),
            "telegram" => Some(ChannelType::Telegram),
            "messagebird" => Some(ChannelType::MessageBird),
            "signal" => Some(ChannelType::Signal),
            _ => None,
        }
    }

    pub fn all() -> Vec<ChannelType> {
        vec![
            ChannelType::Email,
            ChannelType::Telegram,
            ChannelType::MessageBird,
            ChannelType::Signal,
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

    pub fn from_str(s: &str) -> Self {
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
                NotificationSeverity::from_str(&setting.severity),
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
        AuditAction::Login => "Login Successful".to_string(),
        AuditAction::LoginFailed => "Login Failed".to_string(),
        AuditAction::Logout => "Logged Out".to_string(),
        AuditAction::UserCreated => "New User Created".to_string(),
        AuditAction::UserUpdated => "User Updated".to_string(),
        AuditAction::UserDeleted => "User Deleted".to_string(),
        AuditAction::AppInstalled => "App Installed".to_string(),
        AuditAction::AppUninstalled => "App Uninstalled".to_string(),
        AuditAction::AppRestarted => "App Restarted".to_string(),
        AuditAction::AppAccessed => "App Accessed".to_string(),
        AuditAction::TwoFactorEnabled => "2FA Enabled".to_string(),
        AuditAction::TwoFactorDisabled => "2FA Disabled".to_string(),
        AuditAction::PasswordChanged => "Password Changed".to_string(),
        AuditAction::SystemSettingChanged => "System Setting Changed".to_string(),
        _ => format!("{:?}", action),
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
        AuditAction::Login => format!("User {} logged in successfully", user),
        AuditAction::LoginFailed => format!("Failed login attempt for user {}", user),
        AuditAction::Logout => format!("User {} logged out", user),
        AuditAction::UserCreated => format!("New user account created: {}", detail),
        AuditAction::AppInstalled => format!("App installed: {}", detail),
        AuditAction::AppUninstalled => format!("App uninstalled: {}", detail),
        AuditAction::AppAccessed => format!("User {} accessed {}", user, detail),
        AuditAction::TwoFactorEnabled => format!("User {} enabled two-factor authentication", user),
        AuditAction::PasswordChanged => format!("User {} changed their password", user),
        _ => detail.to_string(),
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
