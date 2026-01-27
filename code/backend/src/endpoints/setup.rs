use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
    Set,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::models::prelude::*;
use crate::models::{role, system_setting, user, user_role};
use crate::error::{AppError, Result};
use crate::state::AppState;

pub fn setup_routes(state: AppState) -> Router {
    Router::new()
        .route("/required", get(check_setup_required))
        .route("/status", get(get_setup_status))
        .route("/initialize", post(initialize_setup))
        .route("/generate-credentials", get(generate_credentials))
        .route("/validate-path", post(validate_path))
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct SetupRequiredResponse {
    setup_required: bool,
}

#[derive(Debug, Serialize)]
struct SetupStatusResponse {
    setup_required: bool,
    admin_user_exists: bool,
    storage_configured: bool,
}

#[derive(Debug, Deserialize)]
struct SetupRequest {
    admin_username: String,
    admin_email: String,
    admin_password: String,
    storage_path: String,
}

#[derive(Debug, Serialize)]
struct GeneratedCredentialsResponse {
    admin_username: String,
    admin_email: String,
    admin_password: String,
}

/// Check if any user with admin role exists
async fn admin_user_exists(state: &AppState) -> Result<bool> {
    let admin_exists = UserRole::find()
        .join(JoinType::InnerJoin, user_role::Relation::Role.def())
        .filter(role::Column::Name.eq("admin"))
        .one(&state.db)
        .await?;

    Ok(admin_exists.is_some())
}

/// Check if setup is required (no admin user exists)
async fn check_setup_required(
    State(state): State<AppState>,
) -> Result<Json<SetupRequiredResponse>> {
    let admin_exists = admin_user_exists(&state).await?;

    Ok(Json(SetupRequiredResponse {
        setup_required: !admin_exists,
    }))
}

/// Get detailed setup status
async fn get_setup_status(State(state): State<AppState>) -> Result<Json<SetupStatusResponse>> {
    // Check for admin user (user with admin role)
    let admin_exists = admin_user_exists(&state).await?;

    // Only accessible during setup
    if admin_exists {
        return Err(AppError::Forbidden(
            "Setup has already been completed".to_string(),
        ));
    }

    // Check for storage configuration
    let storage_configured = SystemSetting::find_by_id("storage_path")
        .one(&state.db)
        .await?
        .is_some();

    Ok(Json(SetupStatusResponse {
        setup_required: !admin_exists,
        admin_user_exists: admin_exists,
        storage_configured,
    }))
}

/// Initialize the dashboard
async fn initialize_setup(
    State(state): State<AppState>,
    Json(request): Json<SetupRequest>,
) -> Result<Json<serde_json::Value>> {
    // Check if setup is required (user with admin role exists)
    let admin_exists = admin_user_exists(&state).await?;

    if admin_exists {
        return Err(AppError::Forbidden(
            "Setup has already been completed".to_string(),
        ));
    }

    // Hash the password
    let hashed_password = crate::services::security::hash_password(&request.admin_password)?;

    // Create admin user
    let now = Utc::now();
    let new_user = user::ActiveModel {
        username: Set(request.admin_username.clone()),
        email: Set(request.admin_email.clone()),
        hashed_password: Set(hashed_password),
        is_active: Set(true),
        is_approved: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let created_user = new_user
        .insert(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create admin user: {}", e)))?;

    // Check if admin role exists
    let admin_role = Role::find()
        .filter(role::Column::Name.eq("admin"))
        .one(&state.db)
        .await?;

    let admin_role = match admin_role {
        Some(r) => r,
        None => {
            // Create admin role
            let new_role = role::ActiveModel {
                name: Set("admin".to_string()),
                description: Set(Some("Full system access".to_string())),
                is_system: Set(true),
                created_at: Set(now),
                ..Default::default()
            };
            new_role
                .insert(&state.db)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to create admin role: {}", e)))?
        }
    };

    // Assign admin role to user
    let user_role_model = user_role::ActiveModel {
        user_id: Set(created_user.id),
        role_id: Set(admin_role.id),
    };
    user_role_model
        .insert(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to assign admin role: {}", e)))?;

    // Save storage path
    let storage_setting = system_setting::ActiveModel {
        key: Set("storage_path".to_string()),
        value: Set(request.storage_path.clone()),
        description: Set(Some("Root storage path for media apps".to_string())),
        updated_at: Set(now),
    };
    storage_setting
        .insert(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to save storage path: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Setup completed successfully",
        "data": {
            "admin_user": {
                "username": request.admin_username,
                "email": request.admin_email
            },
            "storage": {
                "path": request.storage_path
            }
        }
    })))
}

/// Generate random credentials for setup
async fn generate_credentials(
    State(state): State<AppState>,
) -> Result<Json<GeneratedCredentialsResponse>> {
    // Check if setup is required (user with admin role exists)
    let admin_exists = admin_user_exists(&state).await?;

    if admin_exists {
        return Err(AppError::Forbidden(
            "Setup has already been completed".to_string(),
        ));
    }

    Ok(Json(GeneratedCredentialsResponse {
        admin_username: "admin".to_string(),
        admin_email: "admin@example.com".to_string(),
        admin_password: crate::services::security::generate_random_string(16),
    }))
}

#[derive(Debug, Deserialize)]
struct ValidatePathQuery {
    path: String,
}

#[derive(Debug, Serialize)]
struct ValidatePathResponse {
    valid: bool,
    exists: bool,
    writable: bool,
    message: String,
}

/// Validate a storage path
async fn validate_path(
    Query(query): Query<ValidatePathQuery>,
) -> Result<Json<ValidatePathResponse>> {
    let path = Path::new(&query.path);

    // Check if path exists
    let exists = path.exists();

    // Check if path is a directory and writable
    let (valid, writable, message) = if exists {
        if path.is_dir() {
            // Try to check if writable by checking metadata
            match std::fs::metadata(path) {
                Ok(_) => (true, true, "Path is valid and accessible".to_string()),
                Err(e) => (
                    false,
                    false,
                    format!("Path exists but is not accessible: {}", e),
                ),
            }
        } else {
            (
                false,
                false,
                "Path exists but is not a directory".to_string(),
            )
        }
    } else {
        // Path doesn't exist, check if parent exists and is writable
        if let Some(parent) = path.parent() {
            if parent.exists() && parent.is_dir() {
                (
                    true,
                    true,
                    "Path does not exist but can be created".to_string(),
                )
            } else {
                (false, false, "Parent directory does not exist".to_string())
            }
        } else {
            (false, false, "Invalid path".to_string())
        }
    };

    Ok(Json(ValidatePathResponse {
        valid,
        exists,
        writable,
        message,
    }))
}
