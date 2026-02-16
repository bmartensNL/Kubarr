use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use std::time::Duration;

use crate::config::CONFIG;
use crate::error::{AppError, Result};
use crate::migrations::Migrator;

pub type DbConn = DatabaseConnection;

/// Create a new database connection and run migrations using config
pub async fn connect() -> Result<DbConn> {
    connect_with_url(&CONFIG.database.database_url).await
}

/// Create a new database connection with a specific URL and run migrations
pub async fn connect_with_url(database_url: &str) -> Result<DbConn> {
    tracing::info!("Connecting to database...");

    let mut opts = ConnectOptions::new(database_url);
    opts.max_connections(10)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .sqlx_logging(false);

    let db = Database::connect(opts)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to connect to database: {}", e)))?;

    tracing::info!("Running database migrations...");
    Migrator::up(&db, None)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to run migrations: {}", e)))?;
    tracing::info!("Database migrations completed");

    Ok(db)
}

/// Try to connect to database, returns None if connection fails
/// Uses a short timeout for the initial probe
pub async fn try_connect() -> Option<DbConn> {
    // First, check if the database URL looks like it could work
    // If it's the default localhost URL, skip trying since PostgreSQL isn't deployed yet
    if CONFIG.database.database_url.contains("localhost")
        && std::env::var("KUBARR_DATABASE_URL").is_err()
    {
        tracing::info!("No DATABASE_URL configured, skipping initial database connection");
        return None;
    }

    // Try to connect with a short timeout
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        connect_with_url(&CONFIG.database.database_url),
    )
    .await;

    match result {
        Ok(Ok(db)) => Some(db),
        Ok(Err(e)) => {
            tracing::info!("Database not available yet: {}", e);
            None
        }
        Err(_) => {
            tracing::info!("Database connection timed out");
            None
        }
    }
}
