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
    let conn = init_database().await?;
    let k8s_client = init_kubernetes().await;
    let catalog = init_catalog();
    let audit = init_audit(&conn).await;
    let notification = init_notifications(&conn).await;

    // Initialize JWT keys from database
    init_jwt_keys(&conn).await?;
    tracing::info!("JWT signing keys initialized");

    // Start periodic task scheduler
    scheduler::start_scheduler(Arc::new(conn.clone()));

    Ok(AppState::new(
        conn,
        k8s_client,
        catalog,
        audit,
        notification,
    ))
}

/// Initialize the database connection (runs migrations automatically)
async fn init_database() -> anyhow::Result<sea_orm::DatabaseConnection> {
    let conn = db::connect().await?;
    tracing::info!("Database connection established");
    Ok(conn)
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

/// Initialize the audit service
async fn init_audit(pool: &sea_orm::DatabaseConnection) -> AuditService {
    let audit = AuditService::new();
    audit.set_db(pool.clone()).await;
    tracing::info!("Audit service initialized");
    audit
}

/// Initialize the notification service
async fn init_notifications(pool: &sea_orm::DatabaseConnection) -> NotificationService {
    let notification = NotificationService::new();
    notification.set_db(pool.clone()).await;
    if let Err(e) = notification.init_providers().await {
        tracing::warn!("Failed to initialize notification providers: {}", e);
    }
    tracing::info!("Notification service initialized");
    notification
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
