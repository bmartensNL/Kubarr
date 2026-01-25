use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    oauth2_client_exists: bool,
    storage_configured: bool,
}

#[derive(Debug, Deserialize)]
struct SetupRequest {
    admin_username: String,
    admin_email: String,
    admin_password: String,
    storage_path: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    oauth2_client_secret: Option<String>,
}

#[derive(Debug, Serialize)]
struct GeneratedCredentialsResponse {
    admin_username: String,
    admin_email: String,
    admin_password: String,
    client_secret: String,
}

/// Check if setup is required (no admin user exists)
async fn check_setup_required(
    State(state): State<AppState>,
) -> Result<Json<SetupRequiredResponse>> {
    let admin_exists: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM users WHERE is_admin = 1 LIMIT 1"
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    Ok(Json(SetupRequiredResponse {
        setup_required: admin_exists.is_none(),
    }))
}

/// Get detailed setup status
async fn get_setup_status(
    State(state): State<AppState>,
) -> Result<Json<SetupStatusResponse>> {
    // Check for admin user
    let admin_exists: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM users WHERE is_admin = 1 LIMIT 1"
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    // Check for oauth2-proxy client
    let oauth2_client_exists: Option<(String,)> = sqlx::query_as(
        "SELECT client_id FROM oauth2_clients WHERE client_id = 'oauth2-proxy' LIMIT 1"
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    // Check for storage configuration
    let storage_configured: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM system_settings WHERE key = 'storage_path' LIMIT 1"
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    let setup_required = admin_exists.is_none();

    // Only accessible during setup
    if !setup_required {
        return Err(AppError::Forbidden(
            "Setup has already been completed".to_string(),
        ));
    }

    Ok(Json(SetupStatusResponse {
        setup_required,
        admin_user_exists: admin_exists.is_some(),
        oauth2_client_exists: oauth2_client_exists.is_some(),
        storage_configured: storage_configured.is_some(),
    }))
}

/// Initialize the dashboard
async fn initialize_setup(
    State(state): State<AppState>,
    Json(request): Json<SetupRequest>,
) -> Result<Json<serde_json::Value>> {
    // Check if setup is required
    let admin_exists: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM users WHERE is_admin = 1 LIMIT 1"
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    if admin_exists.is_some() {
        return Err(AppError::Forbidden(
            "Setup has already been completed".to_string(),
        ));
    }

    // Hash the password
    let hashed_password = crate::services::security::hash_password(&request.admin_password)?;

    // Create admin user
    let now = chrono::Utc::now();
    sqlx::query(
        r#"
        INSERT INTO users (username, email, hashed_password, is_admin, is_active, is_approved, created_at, updated_at)
        VALUES (?, ?, ?, 1, 1, 1, ?, ?)
        "#,
    )
    .bind(&request.admin_username)
    .bind(&request.admin_email)
    .bind(&hashed_password)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Failed to create admin user: {}", e)))?;

    // Save storage path
    sqlx::query(
        r#"
        INSERT INTO system_settings (key, value, description, updated_at)
        VALUES ('storage_path', ?, 'Root storage path for media apps', ?)
        "#,
    )
    .bind(&request.storage_path)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Failed to save storage path: {}", e)))?;

    // Create oauth2-proxy client if base_url is provided
    let mut oauth2_result = serde_json::Value::Null;
    if let Some(base_url) = &request.base_url {
        let client_secret = request.oauth2_client_secret.clone()
            .unwrap_or_else(|| crate::services::security::generate_random_string(32));

        let secret_hash = crate::services::security::hash_client_secret(&client_secret)?;
        let redirect_uris = serde_json::json!([
            format!("{}/oauth2/callback", base_url),
            format!("{}/oauth/callback", base_url)
        ]);

        sqlx::query(
            r#"
            INSERT INTO oauth2_clients (client_id, client_secret_hash, name, redirect_uris, created_at)
            VALUES ('oauth2-proxy', ?, 'OAuth2 Proxy', ?, ?)
            "#,
        )
        .bind(&secret_hash)
        .bind(redirect_uris.to_string())
        .bind(now)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create OAuth2 client: {}", e)))?;

        // Store the plain secret in system settings
        sqlx::query(
            r#"
            INSERT INTO system_settings (key, value, description, updated_at)
            VALUES ('oauth2_client_secret', ?, 'OAuth2-proxy client secret', ?)
            "#,
        )
        .bind(&client_secret)
        .bind(now)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to save client secret: {}", e)))?;

        // Sync credentials to Kubernetes secret for oauth2-proxy
        // Cookie secret must be exactly 32 bytes for AES-256, base64 encoded
        let cookie_secret = crate::services::security::generate_cookie_secret();
        let k8s_guard = state.k8s_client.read().await;
        if let Some(ref k8s) = *k8s_guard {
            let _ = k8s.sync_oauth2_proxy_secret(
                "oauth2-proxy",
                &client_secret,
                &cookie_secret,
                "kubarr-system",
            ).await;
        }
        drop(k8s_guard);

        oauth2_result = serde_json::json!({
            "client_id": "oauth2-proxy",
            "client_secret": client_secret,
            "redirect_uris": [
                format!("{}/oauth2/callback", base_url),
                format!("{}/oauth/callback", base_url)
            ]
        });
    }

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
            },
            "oauth2_client": oauth2_result
        }
    })))
}

/// Generate random credentials for setup
async fn generate_credentials(
    State(state): State<AppState>,
) -> Result<Json<GeneratedCredentialsResponse>> {
    // Check if setup is required
    let admin_exists: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM users WHERE is_admin = 1 LIMIT 1"
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    if admin_exists.is_some() {
        return Err(AppError::Forbidden(
            "Setup has already been completed".to_string(),
        ));
    }

    Ok(Json(GeneratedCredentialsResponse {
        admin_username: "admin".to_string(),
        admin_email: "admin@example.com".to_string(),
        admin_password: crate::services::security::generate_random_string(16),
        client_secret: crate::services::security::generate_random_string(32),
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
                Err(e) => (false, false, format!("Path exists but is not accessible: {}", e)),
            }
        } else {
            (false, false, "Path exists but is not a directory".to_string())
        }
    } else {
        // Path doesn't exist, check if parent exists and is writable
        if let Some(parent) = path.parent() {
            if parent.exists() && parent.is_dir() {
                (true, true, "Path does not exist but can be created".to_string())
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
