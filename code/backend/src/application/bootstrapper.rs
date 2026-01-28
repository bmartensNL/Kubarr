//! Application bootstrapper
//!
//! Handles all initialization and setup for the Kubarr backend.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tokio::sync::RwLock;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::CONFIG;
use crate::db;
use crate::endpoints;
use crate::services::{
    init_jwt_keys, scheduler, start_network_broadcaster, AppCatalog, AuditService, K8sClient,
    NotificationService,
};
use crate::state::AppState;

/// Bootstrap and run the application
pub async fn run() -> anyhow::Result<()> {
    init_tracing();

    tracing::info!("Starting Kubarr backend v{}", env!("CARGO_PKG_VERSION"));

    let state = init_services().await?;

    // Start background network metrics broadcaster
    start_network_broadcaster(state.clone());
    tracing::info!("Network metrics broadcaster started");

    let app = create_app(state);

    serve(app).await
}

/// Initialize tracing/logging
fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("kubarr={}", CONFIG.log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();
}

/// Initialize all application services
async fn init_services() -> anyhow::Result<AppState> {
    let k8s_client = init_kubernetes().await;
    let catalog = init_catalog();

    // Try to connect to database (may not be available during initial setup)
    let conn = init_database().await;

    let audit = AuditService::new();
    let notification = NotificationService::new();

    // If database is available, initialize services that need it
    if let Some(ref db) = conn {
        audit.set_db(db.clone()).await;
        notification.set_db(db.clone()).await;
        if let Err(e) = notification.init_providers().await {
            tracing::warn!("Failed to initialize notification providers: {}", e);
        }

        // Initialize JWT keys from database
        if let Err(e) = init_jwt_keys(db).await {
            tracing::warn!("Failed to initialize JWT keys: {}", e);
        } else {
            tracing::info!("JWT signing keys initialized");
        }

        // Start periodic task scheduler
        scheduler::start_scheduler(Arc::new(db.clone()));
    } else {
        tracing::info!("Database not available - running in setup mode");
    }

    Ok(AppState::new(
        conn,
        k8s_client,
        catalog,
        audit,
        notification,
    ))
}

/// Initialize the database connection (runs migrations automatically)
/// Returns None if database is not available (e.g., PostgreSQL not yet installed)
async fn init_database() -> Option<sea_orm::DatabaseConnection> {
    match db::try_connect().await {
        Some(conn) => {
            tracing::info!("Database connection established");
            Some(conn)
        }
        None => {
            tracing::info!("Database not available - will connect after PostgreSQL is installed");
            None
        }
    }
}

/// Initialize the Kubernetes client
async fn init_kubernetes() -> Arc<RwLock<Option<K8sClient>>> {
    match K8sClient::new().await {
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
    }
}

/// Initialize the app catalog
fn init_catalog() -> Arc<RwLock<AppCatalog>> {
    let catalog = Arc::new(RwLock::new(AppCatalog::new()));
    tracing::info!("App catalog loaded");
    catalog
}

/// Create the main application router
fn create_app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    endpoints::create_router(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

/// Start the HTTP server
async fn serve(app: Router) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], CONFIG.port));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
