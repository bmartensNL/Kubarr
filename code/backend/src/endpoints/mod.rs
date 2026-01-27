pub mod apps;
pub mod audit;
pub mod auth;
pub mod extractors;
pub mod logs;
pub mod monitoring;
pub mod networking;
pub mod notifications;
pub mod oauth;
pub mod proxy;
pub mod roles;
pub mod settings;
pub mod setup;
pub mod storage;
pub mod users;

use axum::{middleware as axum_middleware, Router};

use crate::config::CONFIG;
use crate::middleware::require_auth;
use crate::state::AppState;

/// Create the main API router
pub fn create_router(state: AppState) -> Router {
    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/api/health", axum::routing::get(health_check))
        .route("/api/system/health", axum::routing::get(health_check))
        .route("/api/system/version", axum::routing::get(get_version))
        .nest("/auth", auth::auth_routes(state.clone()))
        .nest("/api/setup", setup::setup_routes(state.clone()));

    // Protected routes (auth required)
    let protected_routes = Router::new()
        .nest("/api", api_routes(state.clone()))
        .nest("/proxy", proxy::proxy_routes(state.clone()))
        .layer(axum_middleware::from_fn_with_state(
            state,
            require_auth,
        ));

    // Merge public and protected routes
    public_routes.merge(protected_routes)
}

/// API routes under /api/* (protected by auth middleware)
fn api_routes(state: AppState) -> Router {
    Router::new()
        .nest("/users", users::users_routes(state.clone()))
        .nest("/roles", roles::roles_routes(state.clone()))
        .nest("/settings", settings::settings_routes(state.clone()))
        .nest("/monitoring", monitoring::monitoring_routes(state.clone()))
        .nest("/networking", networking::networking_routes(state.clone()))
        .nest("/apps", apps::apps_routes(state.clone()))
        .nest("/storage", storage::storage_routes(state.clone()))
        .nest("/logs", logs::logs_routes(state.clone()))
        .nest("/audit", audit::audit_routes(state.clone()))
        .nest("/notifications", notifications::notifications_routes(state.clone()))
        .nest("/oauth", oauth::oauth_routes(state.clone()))
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
