use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
    Set,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{AppError, Result};
use crate::models::prelude::*;
use crate::models::{role, system_setting, user, user_role};
use crate::services::bootstrap::{self, BootstrapService, ComponentStatus};
use crate::state::AppState;

pub fn setup_routes(state: AppState) -> Router {
    Router::new()
        .route("/required", get(check_setup_required))
        .route("/status", get(get_setup_status))
        .route("/initialize", post(initialize_setup))
        .route("/generate-credentials", get(generate_credentials))
        .route("/validate-path", post(validate_path))
        // Bootstrap endpoints
        .route("/bootstrap/start", post(start_bootstrap))
        .route("/bootstrap/status", get(get_bootstrap_status))
        .route(
            "/bootstrap/retry/:component",
            post(retry_bootstrap_component),
        )
        .route("/bootstrap/ws", get(bootstrap_ws_handler))
        // Server config endpoints
        .route("/server", get(get_server_config).post(configure_server))
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct SetupRequiredResponse {
    setup_required: bool,
}

#[derive(Debug, Serialize)]
struct SetupStatusResponse {
    setup_required: bool,
    bootstrap_complete: bool,
    server_configured: bool,
    admin_user_exists: bool,
    storage_configured: bool,
}

#[derive(Debug, Deserialize)]
struct SetupRequest {
    admin_username: String,
    admin_email: String,
    admin_password: String,
}

#[derive(Debug, Serialize)]
struct GeneratedCredentialsResponse {
    admin_username: String,
    admin_email: String,
    admin_password: String,
}

/// Check if any user with admin role exists (requires database)
async fn admin_user_exists(state: &AppState) -> Result<bool> {
    let db_guard = state.db.read().await;
    let db = match db_guard.as_ref() {
        Some(db) => db,
        None => return Ok(false), // No database = no admin
    };

    let admin_exists = UserRole::find()
        .join(JoinType::InnerJoin, user_role::Relation::Role.def())
        .filter(role::Column::Name.eq("admin"))
        .one(db)
        .await?;

    Ok(admin_exists.is_some())
}

/// Require setup to be incomplete (no admin user exists)
///
/// Returns 403 Forbidden if setup is already complete (admin user exists).
/// This is the self-disabling mechanism for setup endpoints - they should only
/// be accessible during initial setup before the first admin user is created.
async fn require_setup(state: &AppState) -> Result<()> {
    let admin_exists = admin_user_exists(state).await?;
    if admin_exists {
        return Err(AppError::Forbidden("Setup already complete".to_string()));
    }
    Ok(())
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

    // Check bootstrap status
    let bootstrap_service = BootstrapService::new(
        state.db.clone(),
        state.k8s_client.clone(),
        state.catalog.clone(),
        state.bootstrap_tx.clone(),
    );
    let bootstrap_complete = bootstrap_service.is_complete().await;

    // Check for server configuration (requires database)
    let server_configured = {
        let db_guard = state.db.read().await;
        if let Some(ref db) = *db_guard {
            bootstrap::get_server_config(db).await?.is_some()
        } else {
            false
        }
    };

    // Check for storage configuration (legacy - now in server_config)
    let storage_configured = {
        let db_guard = state.db.read().await;
        if let Some(ref db) = *db_guard {
            server_configured
                || SystemSetting::find_by_id("storage_path")
                    .one(db)
                    .await?
                    .is_some()
        } else {
            false
        }
    };

    Ok(Json(SetupStatusResponse {
        setup_required: !admin_exists,
        bootstrap_complete,
        server_configured,
        admin_user_exists: admin_exists,
        storage_configured,
    }))
}

/// Initialize the dashboard (create admin user)
async fn initialize_setup(
    State(state): State<AppState>,
    Json(request): Json<SetupRequest>,
) -> Result<Json<serde_json::Value>> {
    // Get database connection (required for this step)
    let db = state.get_db().await?;

    // Check if setup is required (user with admin role exists)
    let admin_exists = admin_user_exists(&state).await?;

    if admin_exists {
        return Err(AppError::Forbidden(
            "Setup has already been completed".to_string(),
        ));
    }

    // Get server config for storage path
    let server_config = bootstrap::get_server_config(&db).await?.ok_or_else(|| {
        AppError::BadRequest("Server must be configured before creating admin user".to_string())
    })?;

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
        .insert(&db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create admin user: {}", e)))?;

    // Check if admin role exists
    let admin_role = Role::find()
        .filter(role::Column::Name.eq("admin"))
        .one(&db)
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
                .insert(&db)
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
        .insert(&db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to assign admin role: {}", e)))?;

    // Also save storage path to system_setting for backwards compatibility
    let storage_setting = system_setting::ActiveModel {
        key: Set("storage_path".to_string()),
        value: Set(server_config.storage_path.clone()),
        description: Set(Some("Root storage path for media apps".to_string())),
        updated_at: Set(now),
    };
    // Try to insert, ignore if already exists
    let _ = storage_setting.insert(&db).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Setup completed successfully",
        "data": {
            "admin_user": {
                "username": request.admin_username,
                "email": request.admin_email
            },
            "server": {
                "name": server_config.name,
                "storage_path": server_config.storage_path
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

// ============================================================================
// Bootstrap Endpoints
// ============================================================================

#[derive(Debug, Serialize)]
struct BootstrapStatusResponse {
    components: Vec<ComponentStatus>,
    complete: bool,
    started: bool,
}

/// Get bootstrap status
///
/// Returns the current status of bootstrap components during initial setup.
/// Protected by require_setup() to prevent information disclosure after
/// admin user creation.
async fn get_bootstrap_status(
    State(state): State<AppState>,
) -> Result<Json<BootstrapStatusResponse>> {
    // Return 403 if setup is already complete
    require_setup(&state).await?;

    let bootstrap_service = BootstrapService::new(
        state.db.clone(),
        state.k8s_client.clone(),
        state.catalog.clone(),
        state.bootstrap_tx.clone(),
    );

    let components = bootstrap_service.get_status().await?;
    let complete = bootstrap_service.is_complete().await;
    let started = bootstrap_service.has_started().await;

    Ok(Json(BootstrapStatusResponse {
        components,
        complete,
        started,
    }))
}

#[derive(Debug, Serialize)]
struct BootstrapStartResponse {
    message: String,
    started: bool,
}

/// Start the bootstrap process
async fn start_bootstrap(State(state): State<AppState>) -> Result<Json<BootstrapStartResponse>> {
    let bootstrap_service = Arc::new(RwLock::new(BootstrapService::new(
        state.db.clone(),
        state.k8s_client.clone(),
        state.catalog.clone(),
        state.bootstrap_tx.clone(),
    )));

    // Check if already complete
    {
        let service = bootstrap_service.read().await;
        if service.is_complete().await {
            return Ok(Json(BootstrapStartResponse {
                message: "Bootstrap already complete".to_string(),
                started: false,
            }));
        }
    }

    // Start bootstrap in background task
    let service_clone = bootstrap_service.clone();
    tokio::spawn(async move {
        let service = service_clone.read().await;
        if let Err(e) = service.start_bootstrap().await {
            tracing::error!("Bootstrap failed: {}", e);
        }
    });

    Ok(Json(BootstrapStartResponse {
        message: "Bootstrap started".to_string(),
        started: true,
    }))
}

/// Retry a failed bootstrap component
///
/// This endpoint allows retrying individual bootstrap steps if they fail during
/// the initial setup process. Protected by require_setup() to ensure it's only
/// accessible before admin user creation.
async fn retry_bootstrap_component(
    State(state): State<AppState>,
    axum::extract::Path(component): axum::extract::Path<String>,
) -> Result<Json<BootstrapStartResponse>> {
    // Return 403 if setup is already complete
    require_setup(&state).await?;

    let bootstrap_service = Arc::new(RwLock::new(BootstrapService::new(
        state.db.clone(),
        state.k8s_client.clone(),
        state.catalog.clone(),
        state.bootstrap_tx.clone(),
    )));

    // Retry in background task
    let component_clone = component.clone();
    tokio::spawn(async move {
        let service = bootstrap_service.read().await;
        if let Err(e) = service.retry_component(&component_clone).await {
            tracing::error!("Retry failed for {}: {}", component_clone, e);
        }
    });

    Ok(Json(BootstrapStartResponse {
        message: format!("Retrying {}", component),
        started: true,
    }))
}

/// WebSocket handler for bootstrap progress updates
async fn bootstrap_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    tracing::info!("Bootstrap WebSocket upgrade request received");
    ws.on_upgrade(move |socket| handle_bootstrap_socket(socket, state))
}

/// Handle bootstrap WebSocket connection
async fn handle_bootstrap_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to the shared bootstrap broadcast channel
    let mut rx = state.bootstrap_tx.subscribe();

    // Create a bootstrap service to get the initial status
    let bootstrap_service = BootstrapService::new(
        state.db.clone(),
        state.k8s_client.clone(),
        state.catalog.clone(),
        state.bootstrap_tx.clone(),
    );

    tracing::info!(
        "New WebSocket client connected for bootstrap updates, subscribers: {}",
        state.bootstrap_tx.receiver_count()
    );

    // Send initial status
    if let Ok(components) = bootstrap_service.get_status().await {
        let initial_status = serde_json::json!({
            "type": "initial_status",
            "components": components,
            "complete": bootstrap_service.is_complete().await,
        });
        if let Ok(json) = serde_json::to_string(&initial_status) {
            let _ = sender.send(Message::Text(json)).await;
        }
    }

    // Spawn task to forward broadcast messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages (ping/pong, close)
    let recv_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Ping(_)) => {
                    tracing::debug!("Received ping from WebSocket client");
                }
                Ok(Message::Close(_)) => {
                    tracing::debug!("WebSocket client requested close");
                    break;
                }
                Err(e) => {
                    tracing::debug!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    tracing::info!("Bootstrap WebSocket client disconnected");
}

// ============================================================================
// Server Config Endpoints
// ============================================================================

#[derive(Debug, Deserialize)]
struct ServerConfigRequest {
    name: String,
    storage_path: String,
}

#[derive(Debug, Serialize)]
struct ServerConfigResponse {
    name: String,
    storage_path: String,
}

/// Get server configuration
async fn get_server_config(
    State(state): State<AppState>,
) -> Result<Json<Option<ServerConfigResponse>>> {
    let db_guard = state.db.read().await;
    let config = if let Some(ref db) = *db_guard {
        bootstrap::get_server_config(db).await?
    } else {
        None
    };

    Ok(Json(config.map(|c| ServerConfigResponse {
        name: c.name,
        storage_path: c.storage_path,
    })))
}

/// Configure server (name and storage path)
async fn configure_server(
    State(state): State<AppState>,
    Json(request): Json<ServerConfigRequest>,
) -> Result<Json<ServerConfigResponse>> {
    // Get database connection (required for this step)
    let db = state.get_db().await?;

    // Check if setup is required
    let admin_exists = admin_user_exists(&state).await?;
    if admin_exists {
        return Err(AppError::Forbidden(
            "Setup has already been completed".to_string(),
        ));
    }

    // Validate storage path
    let path = Path::new(&request.storage_path);
    if !path.exists() {
        // Check if parent exists
        if let Some(parent) = path.parent() {
            if !parent.exists() || !parent.is_dir() {
                return Err(AppError::BadRequest(
                    "Invalid storage path: parent directory does not exist".to_string(),
                ));
            }
        } else {
            return Err(AppError::BadRequest("Invalid storage path".to_string()));
        }
    } else if !path.is_dir() {
        return Err(AppError::BadRequest(
            "Storage path exists but is not a directory".to_string(),
        ));
    }

    // Save server config
    let config = bootstrap::save_server_config(&db, &request.name, &request.storage_path).await?;

    Ok(Json(ServerConfigResponse {
        name: config.name,
        storage_path: config.storage_path,
    }))
}
