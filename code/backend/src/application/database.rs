use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use std::time::Duration;

use crate::config::CONFIG;
use crate::error::{AppError, Result};
use crate::migrations::Migrator;

pub type DbConn = DatabaseConnection;

/// Create a new database connection and run migrations
pub async fn connect() -> Result<DbConn> {
    tracing::info!("Connecting to database...");

    let mut opts = ConnectOptions::new(&CONFIG.database_url);
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
