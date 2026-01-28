pub mod apps;
pub mod audit;
pub mod auth;
pub mod extractors;
pub mod frontend;
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
pub mod vpn;

use axum::{extract::State, middleware as axum_middleware, Router};
use sea_orm::{ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait};

use crate::config::CONFIG;
use crate::middleware::require_auth;
use crate::models::prelude::*;
use crate::models::{role, user_role};
use crate::state::AppState;

/// Create the main API router
pub fn create_router(state: AppState) -> Router {
    // Health/version routes that need state (separate so we can apply state properly)
    let health_routes = Router::new()
        .route("/api/health", axum::routing::get(health_check))
        .route(
            "/api/system/health",
            axum::routing::get(health_check_detailed),
        )
        .route("/api/system/version", axum::routing::get(get_version))
        .with_state(state.clone());

    // Public routes (no auth required) - these already have state applied internally
    let public_routes = Router::new()
        .nest("/auth", auth::auth_routes(state.clone()))
        .nest("/api/setup", setup::setup_routes(state.clone()));

    // Protected API routes (auth required)
    let protected_api_routes = Router::new().nest("/api", api_routes(state.clone())).layer(
        axum_middleware::from_fn_with_state(state.clone(), require_auth),
    );

    // Note: App proxy routes (e.g., /qbittorrent/) are handled by the frontend fallback
    // which checks if the path is an installed app and proxies to it if authenticated

    // Frontend fallback router (unauthenticated)
    let fallback_router = Router::new()
        .fallback(frontend::proxy_frontend)
        .with_state(state);

    // Merge all routes, with frontend proxy as fallback
    // The frontend fallback handles app proxying (e.g., /qbittorrent/) for authenticated users
    health_routes
        .merge(public_routes)
        .merge(protected_api_routes)
        .merge(fallback_router)
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
        .nest(
            "/notifications",
            notifications::notifications_routes(state.clone()),
        )
        .nest("/oauth", oauth::oauth_routes(state.clone()))
        .nest("/vpn", vpn::vpn_routes(state.clone()))
}

/// Simple health check endpoint (for k8s probes)
async fn health_check() -> &'static str {
    "OK"
}

/// Detailed health check endpoint with setup status
async fn health_check_detailed(State(state): State<AppState>) -> axum::Json<serde_json::Value> {
    // Check if any user with admin role exists (setup complete)
    let admin_exists = match state.get_db().await {
        Ok(db) => UserRole::find()
            .join(JoinType::InnerJoin, user_role::Relation::Role.def())
            .filter(role::Column::Name.eq("admin"))
            .one(&db)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false),
        Err(_) => false,
    };

    axum::Json(serde_json::json!({
        "status": "ok",
        "ready": admin_exists,
        "setup_required": !admin_exists
    }))
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
