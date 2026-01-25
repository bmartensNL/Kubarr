use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    routing::{get, put},
    Json, Router,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::api::extractors::AdminUser;
use crate::db::{DbPool, SystemSetting};
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Default settings values
static DEFAULT_SETTINGS: Lazy<HashMap<&'static str, (&'static str, &'static str)>> =
    Lazy::new(|| {
        let mut m = HashMap::new();
        m.insert(
            "registration_enabled",
            (
                "true",
                "Allow new user registration (invites still work when disabled)",
            ),
        );
        m.insert(
            "registration_require_approval",
            ("true", "Require admin approval for new registrations"),
        );
        m
    });

/// Create settings routes
pub fn settings_routes(state: AppState) -> Router {
    Router::new()
        .route("/", get(list_settings))
        .route("/:key", get(get_setting).put(update_setting))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SettingResponse {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SettingUpdate {
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub settings: HashMap<String, SettingResponse>,
}

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// List all system settings (admin only)
async fn list_settings(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
) -> Result<Json<SettingsResponse>> {
    // Get all settings from database
    let db_settings: Vec<SystemSetting> = sqlx::query_as("SELECT * FROM system_settings")
        .fetch_all(&state.pool)
        .await?;

    let db_map: HashMap<String, SystemSetting> = db_settings
        .into_iter()
        .map(|s| (s.key.clone(), s))
        .collect();

    // Merge with defaults
    let mut settings = HashMap::new();
    for (key, (default_value, description)) in DEFAULT_SETTINGS.iter() {
        let setting = if let Some(db_setting) = db_map.get(*key) {
            SettingResponse {
                key: key.to_string(),
                value: db_setting.value.clone(),
                description: db_setting
                    .description
                    .clone()
                    .or_else(|| Some(description.to_string())),
            }
        } else {
            SettingResponse {
                key: key.to_string(),
                value: default_value.to_string(),
                description: Some(description.to_string()),
            }
        };
        settings.insert(key.to_string(), setting);
    }

    Ok(Json(SettingsResponse { settings }))
}

/// Get a specific setting (admin only)
async fn get_setting(
    State(state): State<AppState>,
    Path(key): Path<String>,
    AdminUser(_): AdminUser,
) -> Result<Json<SettingResponse>> {
    let db_setting: Option<SystemSetting> =
        sqlx::query_as("SELECT * FROM system_settings WHERE key = ?")
            .bind(&key)
            .fetch_optional(&state.pool)
            .await?;

    if let Some(setting) = db_setting {
        return Ok(Json(SettingResponse {
            key: setting.key,
            value: setting.value,
            description: setting.description,
        }));
    }

    // Check defaults
    if let Some((default_value, description)) = DEFAULT_SETTINGS.get(key.as_str()) {
        return Ok(Json(SettingResponse {
            key,
            value: default_value.to_string(),
            description: Some(description.to_string()),
        }));
    }

    Err(AppError::NotFound(format!("Setting '{}' not found", key)))
}

/// Update a system setting (admin only)
async fn update_setting(
    State(state): State<AppState>,
    Path(key): Path<String>,
    AdminUser(_): AdminUser,
    Json(data): Json<SettingUpdate>,
) -> Result<Json<SettingResponse>> {
    // Validate key exists in defaults
    let (_, description) = DEFAULT_SETTINGS
        .get(key.as_str())
        .ok_or_else(|| AppError::BadRequest(format!("Unknown setting key '{}'", key)))?;

    // Upsert setting
    sqlx::query(
        r#"
        INSERT INTO system_settings (key, value, description, updated_at)
        VALUES (?, ?, ?, datetime('now'))
        ON CONFLICT(key) DO UPDATE SET value = ?, updated_at = datetime('now')
        "#,
    )
    .bind(&key)
    .bind(&data.value)
    .bind(*description)
    .bind(&data.value)
    .execute(&state.pool)
    .await?;

    let setting: SystemSetting =
        sqlx::query_as("SELECT * FROM system_settings WHERE key = ?")
            .bind(&key)
            .fetch_one(&state.pool)
            .await?;

    Ok(Json(SettingResponse {
        key: setting.key,
        value: setting.value,
        description: setting.description,
    }))
}

/// Get a setting value from the database (helper for other modules)
pub async fn get_setting_value(pool: &DbPool, key: &str) -> Result<Option<String>> {
    let setting: Option<SystemSetting> =
        sqlx::query_as("SELECT * FROM system_settings WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;

    if let Some(s) = setting {
        return Ok(Some(s.value));
    }

    // Return default if exists
    if let Some((default_value, _)) = DEFAULT_SETTINGS.get(key) {
        return Ok(Some(default_value.to_string()));
    }

    Ok(None)
}

/// Get a boolean setting value (helper for other modules)
pub async fn get_setting_bool(pool: &DbPool, key: &str) -> Result<bool> {
    let value = get_setting_value(pool, key).await?;
    Ok(value
        .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false))
}
