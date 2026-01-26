pub mod apps;
pub mod auth;
pub mod extractors;
pub mod logs;
pub mod monitoring;
pub mod networking;
pub mod roles;
pub mod settings;
pub mod setup;
pub mod storage;
pub mod users;

use axum::Router;

use crate::config::CONFIG;
use crate::state::AppState;

/// Create the main API router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .nest("/api", api_routes(state.clone()))
        .nest("/auth", auth::auth_routes(state))
}

/// API routes under /api/*
fn api_routes(state: AppState) -> Router {
    Router::new()
        .route("/health", axum::routing::get(health_check))
        .route("/system/health", axum::routing::get(health_check))
        .route("/system/version", axum::routing::get(get_version))
        .nest("/users", users::users_routes(state.clone()))
        .nest("/roles", roles::roles_routes(state.clone()))
        .nest("/settings", settings::settings_routes(state.clone()))
        .nest("/monitoring", monitoring::monitoring_routes(state.clone()))
        .nest("/networking", networking::networking_routes(state.clone()))
        .nest("/apps", apps::apps_routes(state.clone()))
        .nest("/storage", storage::storage_routes(state.clone()))
        .nest("/logs", logs::logs_routes(state.clone()))
        .nest("/setup", setup::setup_routes(state))
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// Version info endpoint
async fn get_version() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "version": CONFIG.version,
        "commit_hash": CONFIG.commit_hash,
        "build_time": CONFIG.build_time,
        "rust_version": "1.83",
        "backend": "rust"
    }))
}
