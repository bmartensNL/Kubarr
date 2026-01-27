use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set};
use serde::{Deserialize, Serialize};

use crate::middleware::permissions::{Authenticated, Authorized, SettingsView, SettingsManage, AuditView};
use crate::models::{notification_channel, notification_event, notification_log, user_notification_pref};
use crate::error::{AppError, Result};
use crate::services::notification::ChannelType;
use crate::state::AppState;

pub fn notifications_routes(state: AppState) -> Router {
    Router::new()
        // User inbox endpoints
        .route("/inbox", get(get_inbox))
        .route("/inbox/count", get(get_unread_count))
        .route("/inbox/:id/read", post(mark_as_read))
        .route("/inbox/read-all", post(mark_all_as_read))
        .route("/inbox/:id", delete(delete_notification))
        // Admin: Channel configuration
        .route("/channels", get(list_channels))
        .route("/channels/:channel_type", get(get_channel))
        .route("/channels/:channel_type", put(update_channel))
        .route("/channels/:channel_type/test", post(test_channel))
        // Admin: Event settings
        .route("/events", get(list_events))
        .route("/events/:event_type", put(update_event))
        // User preferences
        .route("/preferences", get(get_preferences))
        .route("/preferences/:channel_type", put(update_preference))
        // Admin: Logs
        .route("/logs", get(list_logs))
        .with_state(state)
}

// ============================================================================
// User Inbox Endpoints
// ============================================================================

#[derive(Serialize)]
struct InboxResponse {
    notifications: Vec<NotificationDto>,
    total: u64,
    unread: u64,
}

#[derive(Serialize)]
struct NotificationDto {
    id: i64,
    title: String,
    message: String,
    event_type: Option<String>,
    severity: String,
    read: bool,
    created_at: String,
}

#[derive(Deserialize)]
struct InboxQuery {
    limit: Option<u64>,
    offset: Option<u64>,
}

async fn get_inbox(
    State(state): State<AppState>,
    auth: Authenticated,
    Query(query): Query<InboxQuery>,
) -> Result<Json<InboxResponse>> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let notifications = state
        .notification
        .get_user_notifications(auth.user_id(), limit, offset)
        .await?;

    let unread = state.notification.get_unread_count(auth.user_id()).await?;

    let total = crate::models::user_notification::Entity::find()
        .filter(crate::models::user_notification::Column::UserId.eq(auth.user_id()))
        .count(&state.db)
        .await?;

    let dtos: Vec<NotificationDto> = notifications
        .into_iter()
        .map(|n| NotificationDto {
            id: n.id,
            title: n.title,
            message: n.message,
            event_type: n.event_type,
            severity: n.severity,
            read: n.read,
            created_at: n.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(InboxResponse {
        notifications: dtos,
        total,
        unread,
    }))
}

#[derive(Serialize)]
struct UnreadCountResponse {
    count: u64,
}

async fn get_unread_count(
    State(state): State<AppState>,
    auth: Authenticated,
) -> Result<Json<UnreadCountResponse>> {
    let count = state.notification.get_unread_count(auth.user_id()).await?;
    Ok(Json(UnreadCountResponse { count }))
}

async fn mark_as_read(
    State(state): State<AppState>,
    auth: Authenticated,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    state.notification.mark_as_read(id, auth.user_id()).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn mark_all_as_read(
    State(state): State<AppState>,
    auth: Authenticated,
) -> Result<Json<serde_json::Value>> {
    state.notification.mark_all_as_read(auth.user_id()).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn delete_notification(
    State(state): State<AppState>,
    auth: Authenticated,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    state.notification.delete_notification(id, auth.user_id()).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Admin: Channel Configuration
// ============================================================================

#[derive(Serialize)]
struct ChannelDto {
    channel_type: String,
    enabled: bool,
    config: serde_json::Value,
    created_at: String,
    updated_at: String,
}

async fn list_channels(
    State(state): State<AppState>,
    _auth: Authorized<SettingsView>,
) -> Result<Json<Vec<ChannelDto>>> {
    let channels = notification_channel::Entity::find()
        .order_by_asc(notification_channel::Column::ChannelType)
        .all(&state.db)
        .await?;

    // Return all channel types, with defaults for unconfigured ones
    let mut result: Vec<ChannelDto> = Vec::new();

    for channel_type in ChannelType::all() {
        let existing = channels.iter().find(|c| c.channel_type == channel_type.as_str());

        if let Some(channel) = existing {
            let config: serde_json::Value = serde_json::from_str(&channel.config)
                .unwrap_or(serde_json::json!({}));
            // Mask sensitive fields
            let masked_config = mask_sensitive_config(&config);

            result.push(ChannelDto {
                channel_type: channel.channel_type.clone(),
                enabled: channel.enabled,
                config: masked_config,
                created_at: channel.created_at.to_rfc3339(),
                updated_at: channel.updated_at.to_rfc3339(),
            });
        } else {
            result.push(ChannelDto {
                channel_type: channel_type.as_str().to_string(),
                enabled: false,
                config: serde_json::json!({}),
                created_at: "".to_string(),
                updated_at: "".to_string(),
            });
        }
    }

    Ok(Json(result))
}

async fn get_channel(
    State(state): State<AppState>,
    _auth: Authorized<SettingsView>,
    Path(channel_type): Path<String>,
) -> Result<Json<ChannelDto>> {
    let channel = notification_channel::Entity::find()
        .filter(notification_channel::Column::ChannelType.eq(&channel_type))
        .one(&state.db)
        .await?;

    match channel {
        Some(ch) => {
            let config: serde_json::Value = serde_json::from_str(&ch.config)
                .unwrap_or(serde_json::json!({}));
            let masked_config = mask_sensitive_config(&config);

            Ok(Json(ChannelDto {
                channel_type: ch.channel_type,
                enabled: ch.enabled,
                config: masked_config,
                created_at: ch.created_at.to_rfc3339(),
                updated_at: ch.updated_at.to_rfc3339(),
            }))
        }
        None => Ok(Json(ChannelDto {
            channel_type,
            enabled: false,
            config: serde_json::json!({}),
            created_at: "".to_string(),
            updated_at: "".to_string(),
        })),
    }
}

#[derive(Deserialize)]
struct UpdateChannelRequest {
    enabled: Option<bool>,
    config: Option<serde_json::Value>,
}

async fn update_channel(
    State(state): State<AppState>,
    _auth: Authorized<SettingsManage>,
    Path(channel_type): Path<String>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<ChannelDto>> {
    // Validate channel type
    if ChannelType::from_str(&channel_type).is_none() {
        return Err(AppError::BadRequest(format!("Invalid channel type: {}", channel_type)));
    }

    let now = chrono::Utc::now();

    let existing = notification_channel::Entity::find()
        .filter(notification_channel::Column::ChannelType.eq(&channel_type))
        .one(&state.db)
        .await?;

    let channel = if let Some(existing) = existing {
        let mut active: notification_channel::ActiveModel = existing.into();

        if let Some(enabled) = req.enabled {
            active.enabled = Set(enabled);
        }
        if let Some(config) = req.config {
            active.config = Set(serde_json::to_string(&config).unwrap_or_default());
        }
        active.updated_at = Set(now);

        active.update(&state.db).await?
    } else {
        let config = req.config.unwrap_or(serde_json::json!({}));
        let new_channel = notification_channel::ActiveModel {
            channel_type: Set(channel_type.clone()),
            enabled: Set(req.enabled.unwrap_or(false)),
            config: Set(serde_json::to_string(&config).unwrap_or_default()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_channel.insert(&state.db).await?
    };

    // Reinitialize providers
    if let Err(e) = state.notification.init_providers().await {
        tracing::warn!("Failed to reinitialize notification providers: {}", e);
    }

    let config: serde_json::Value = serde_json::from_str(&channel.config)
        .unwrap_or(serde_json::json!({}));
    let masked_config = mask_sensitive_config(&config);

    Ok(Json(ChannelDto {
        channel_type: channel.channel_type,
        enabled: channel.enabled,
        config: masked_config,
        created_at: channel.created_at.to_rfc3339(),
        updated_at: channel.updated_at.to_rfc3339(),
    }))
}

#[derive(Deserialize)]
struct TestChannelRequest {
    destination: String,
}

#[derive(Serialize)]
struct TestChannelResponse {
    success: bool,
    error: Option<String>,
}

async fn test_channel(
    State(state): State<AppState>,
    _auth: Authorized<SettingsManage>,
    Path(channel_type): Path<String>,
    Json(req): Json<TestChannelRequest>,
) -> Result<Json<TestChannelResponse>> {
    let result = state.notification.test_channel(&channel_type, &req.destination).await;

    Ok(Json(TestChannelResponse {
        success: result.success,
        error: result.error,
    }))
}

// ============================================================================
// Admin: Event Settings
// ============================================================================

#[derive(Serialize)]
struct EventSettingDto {
    event_type: String,
    enabled: bool,
    severity: String,
}

async fn list_events(
    State(state): State<AppState>,
    _auth: Authorized<SettingsView>,
) -> Result<Json<Vec<EventSettingDto>>> {
    let events = notification_event::Entity::find()
        .order_by_asc(notification_event::Column::EventType)
        .all(&state.db)
        .await?;

    // Get all possible event types from AuditAction
    let all_event_types = get_all_event_types();

    let mut result: Vec<EventSettingDto> = Vec::new();

    for event_type in all_event_types {
        let existing = events.iter().find(|e| e.event_type == event_type);

        if let Some(event) = existing {
            result.push(EventSettingDto {
                event_type: event.event_type.clone(),
                enabled: event.enabled,
                severity: event.severity.clone(),
            });
        } else {
            result.push(EventSettingDto {
                event_type,
                enabled: false,
                severity: "info".to_string(),
            });
        }
    }

    Ok(Json(result))
}

#[derive(Deserialize)]
struct UpdateEventRequest {
    enabled: Option<bool>,
    severity: Option<String>,
}

async fn update_event(
    State(state): State<AppState>,
    _auth: Authorized<SettingsManage>,
    Path(event_type): Path<String>,
    Json(req): Json<UpdateEventRequest>,
) -> Result<Json<EventSettingDto>> {
    let existing = notification_event::Entity::find()
        .filter(notification_event::Column::EventType.eq(&event_type))
        .one(&state.db)
        .await?;

    let event = if let Some(existing) = existing {
        let mut active: notification_event::ActiveModel = existing.into();

        if let Some(enabled) = req.enabled {
            active.enabled = Set(enabled);
        }
        if let Some(severity) = req.severity {
            active.severity = Set(severity);
        }

        active.update(&state.db).await?
    } else {
        let new_event = notification_event::ActiveModel {
            event_type: Set(event_type.clone()),
            enabled: Set(req.enabled.unwrap_or(false)),
            severity: Set(req.severity.unwrap_or_else(|| "info".to_string())),
            ..Default::default()
        };
        new_event.insert(&state.db).await?
    };

    Ok(Json(EventSettingDto {
        event_type: event.event_type,
        enabled: event.enabled,
        severity: event.severity,
    }))
}

// ============================================================================
// User Preferences
// ============================================================================

#[derive(Serialize)]
struct UserPrefDto {
    channel_type: String,
    enabled: bool,
    destination: Option<String>,
    verified: bool,
}

async fn get_preferences(
    State(state): State<AppState>,
    auth: Authenticated,
) -> Result<Json<Vec<UserPrefDto>>> {
    let prefs = user_notification_pref::Entity::find()
        .filter(user_notification_pref::Column::UserId.eq(auth.user_id()))
        .all(&state.db)
        .await?;

    let mut result: Vec<UserPrefDto> = Vec::new();

    for channel_type in ChannelType::all() {
        let existing = prefs.iter().find(|p| p.channel_type == channel_type.as_str());

        if let Some(pref) = existing {
            result.push(UserPrefDto {
                channel_type: pref.channel_type.clone(),
                enabled: pref.enabled,
                destination: pref.destination.clone().map(|d| mask_destination(&d)),
                verified: pref.verified,
            });
        } else {
            result.push(UserPrefDto {
                channel_type: channel_type.as_str().to_string(),
                enabled: false,
                destination: None,
                verified: false,
            });
        }
    }

    Ok(Json(result))
}

#[derive(Deserialize)]
struct UpdatePrefRequest {
    enabled: Option<bool>,
    destination: Option<String>,
}

async fn update_preference(
    State(state): State<AppState>,
    auth: Authenticated,
    Path(channel_type): Path<String>,
    Json(req): Json<UpdatePrefRequest>,
) -> Result<Json<UserPrefDto>> {
    if ChannelType::from_str(&channel_type).is_none() {
        return Err(AppError::BadRequest(format!("Invalid channel type: {}", channel_type)));
    }

    let now = chrono::Utc::now();

    let existing = user_notification_pref::Entity::find()
        .filter(user_notification_pref::Column::UserId.eq(auth.user_id()))
        .filter(user_notification_pref::Column::ChannelType.eq(&channel_type))
        .one(&state.db)
        .await?;

    let pref = if let Some(existing) = existing {
        let mut active: user_notification_pref::ActiveModel = existing.clone().into();

        if let Some(enabled) = req.enabled {
            active.enabled = Set(enabled);
        }
        if let Some(destination) = req.destination {
            // If destination changed, reset verification
            if Some(&destination) != existing.destination.as_ref() {
                active.destination = Set(Some(destination));
                active.verified = Set(false);
            }
        }
        active.updated_at = Set(now);

        active.update(&state.db).await?
    } else {
        let new_pref = user_notification_pref::ActiveModel {
            user_id: Set(auth.user_id()),
            channel_type: Set(channel_type.clone()),
            enabled: Set(req.enabled.unwrap_or(false)),
            destination: Set(req.destination),
            verified: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_pref.insert(&state.db).await?
    };

    Ok(Json(UserPrefDto {
        channel_type: pref.channel_type,
        enabled: pref.enabled,
        destination: pref.destination.map(|d| mask_destination(&d)),
        verified: pref.verified,
    }))
}

// ============================================================================
// Admin: Logs
// ============================================================================

#[derive(Deserialize)]
struct LogsQuery {
    limit: Option<u64>,
    offset: Option<u64>,
    channel_type: Option<String>,
    status: Option<String>,
}

#[derive(Serialize)]
struct LogDto {
    id: i64,
    user_id: Option<i64>,
    channel_type: String,
    event_type: String,
    recipient: Option<String>,
    status: String,
    error_message: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
struct LogsResponse {
    logs: Vec<LogDto>,
    total: u64,
}

async fn list_logs(
    State(state): State<AppState>,
    _auth: Authorized<AuditView>,
    Query(query): Query<LogsQuery>,
) -> Result<Json<LogsResponse>> {
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let mut q = notification_log::Entity::find();

    if let Some(channel) = &query.channel_type {
        q = q.filter(notification_log::Column::ChannelType.eq(channel));
    }
    if let Some(status) = &query.status {
        q = q.filter(notification_log::Column::Status.eq(status));
    }

    let total = q.clone().count(&state.db).await?;

    let logs = q
        .order_by_desc(notification_log::Column::CreatedAt)
        .offset(offset)
        .limit(limit)
        .all(&state.db)
        .await?;

    let dtos: Vec<LogDto> = logs
        .into_iter()
        .map(|l| LogDto {
            id: l.id,
            user_id: l.user_id,
            channel_type: l.channel_type,
            event_type: l.event_type,
            recipient: l.recipient,
            status: l.status,
            error_message: l.error_message,
            created_at: l.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(LogsResponse { logs: dtos, total }))
}

// ============================================================================
// Helper Functions
// ============================================================================

fn mask_sensitive_config(config: &serde_json::Value) -> serde_json::Value {
    let mut masked = config.clone();

    if let Some(obj) = masked.as_object_mut() {
        for (key, value) in obj.iter_mut() {
            let key_lower = key.to_lowercase();
            let is_sensitive = key_lower.contains("password")
                || key_lower.contains("secret")
                || key_lower.contains("token")
                || key_lower.contains("api_key");
            if is_sensitive && value.is_string() && !value.as_str().unwrap_or("").is_empty() {
                *value = serde_json::Value::String("********".to_string());
            }
        }
    }

    masked
}

fn mask_destination(dest: &str) -> String {
    if dest.contains('@') {
        // Email
        let parts: Vec<&str> = dest.split('@').collect();
        if parts.len() == 2 && parts[0].len() > 2 {
            format!("{}***@{}", &parts[0][..2], parts[1])
        } else {
            "***@***".to_string()
        }
    } else if dest.starts_with('+') {
        // Phone
        if dest.len() > 4 {
            format!("{}***{}", &dest[..3], &dest[dest.len()-2..])
        } else {
            "+***".to_string()
        }
    } else if dest.len() > 5 {
        // Other (chat ID)
        format!("{}***{}", &dest[..3], &dest[dest.len()-2..])
    } else {
        "***".to_string()
    }
}

fn get_all_event_types() -> Vec<String> {
    use crate::models::audit_log::AuditAction;

    vec![
        AuditAction::Login.to_string(),
        AuditAction::LoginFailed.to_string(),
        AuditAction::Logout.to_string(),
        AuditAction::UserCreated.to_string(),
        AuditAction::UserUpdated.to_string(),
        AuditAction::UserDeleted.to_string(),
        AuditAction::UserApproved.to_string(),
        AuditAction::UserDeactivated.to_string(),
        AuditAction::RoleCreated.to_string(),
        AuditAction::RoleUpdated.to_string(),
        AuditAction::RoleDeleted.to_string(),
        AuditAction::RoleAssigned.to_string(),
        AuditAction::RoleUnassigned.to_string(),
        AuditAction::AppInstalled.to_string(),
        AuditAction::AppUninstalled.to_string(),
        AuditAction::AppRestarted.to_string(),
        AuditAction::AppAccessed.to_string(),
        AuditAction::TwoFactorEnabled.to_string(),
        AuditAction::TwoFactorDisabled.to_string(),
        AuditAction::PasswordChanged.to_string(),
        AuditAction::SystemSettingChanged.to_string(),
        AuditAction::InviteCreated.to_string(),
        AuditAction::InviteUsed.to_string(),
    ]
}
