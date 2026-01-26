mod api;
mod config;
mod db;
mod error;
mod services;
mod state;

#[cfg(test)]
mod test_helpers;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tokio::sync::RwLock;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::CONFIG;
use crate::db::create_pool;
use crate::services::{AppCatalog, K8sClient};
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kubarr=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Kubarr backend v{}", env!("CARGO_PKG_VERSION"));

    // Create database pool
    let pool = create_pool().await?;
    tracing::info!("Database connection established");

    // Create Kubernetes client
    let k8s_client = match K8sClient::new().await {
        Ok(client) => {
            tracing::info!("Kubernetes client initialized");
            Arc::new(RwLock::new(Some(client)))
        }
        Err(e) => {
            tracing::warn!(
                "Failed to initialize Kubernetes client: {}. Some features will be unavailable.",
                e
            );
            Arc::new(RwLock::new(None))
        }
    };

    // Create app catalog
    let catalog = Arc::new(RwLock::new(AppCatalog::new()));
    tracing::info!("App catalog loaded");

    // Create app state
    let state = AppState::new(pool, k8s_client, catalog);

    // Build the application
    let app = create_app(state);

    // Determine bind address
    let addr = SocketAddr::from(([0, 0, 0, 0], CONFIG.port));
    tracing::info!("Listening on {}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Create the main application router
fn create_app(state: AppState) -> Router {
    // CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // API routes
    let api_router = api::create_router(state.clone());

    // Static file serving (frontend)
    let static_service = ServeDir::new(&CONFIG.static_files_dir).not_found_service(
        ServeDir::new(&CONFIG.static_files_dir).fallback(tower_http::services::ServeFile::new(
            CONFIG.static_files_dir.join("index.html"),
        )),
    );

    Router::new()
        .merge(api_router)
        .fallback_service(static_service)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}
