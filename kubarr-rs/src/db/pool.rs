use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::time::Duration;

use crate::config::CONFIG;
use crate::error::{AppError, Result};

pub type DbPool = Pool<Sqlite>;

/// Create a new database connection pool
pub async fn create_pool() -> Result<DbPool> {
    let db_url = CONFIG.db_url();

    tracing::info!("Connecting to database: {}", CONFIG.db_path.display());

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .connect(&db_url)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to connect to database: {}", e)))?;

    // Run migrations
    run_migrations(&pool).await?;

    // Seed default data
    seed_defaults(&pool).await?;

    Ok(pool)
}

/// Run database migrations
async fn run_migrations(pool: &DbPool) -> Result<()> {
    tracing::info!("Running database migrations...");

    // Create tables if they don't exist
    sqlx::query(SCHEMA_SQL)
        .execute(pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to run migrations: {}", e)))?;

    tracing::info!("Database migrations completed");
    Ok(())
}

/// Seed default roles and settings
async fn seed_defaults(pool: &DbPool) -> Result<()> {
    // Check if roles exist
    let role_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM roles")
        .fetch_one(pool)
        .await?;

    if role_count.0 == 0 {
        tracing::info!("Seeding default roles...");

        // Create default roles
        let default_roles = [
            ("admin", "Full administrator access", true),
            ("viewer", "View-only access to media apps", true),
            ("downloader", "Access to download clients", true),
        ];

        for (name, description, is_system) in default_roles {
            sqlx::query(
                "INSERT INTO roles (name, description, is_system, created_at) VALUES (?, ?, ?, datetime('now'))",
            )
            .bind(name)
            .bind(description)
            .bind(is_system)
            .execute(pool)
            .await?;
        }

        // Add app permissions for viewer role
        let viewer_apps = ["jellyfin", "jellyseerr"];
        for app in viewer_apps {
            sqlx::query(
                "INSERT INTO role_app_permissions (role_id, app_name)
                 SELECT id, ? FROM roles WHERE name = 'viewer'",
            )
            .bind(app)
            .execute(pool)
            .await?;
        }

        // Add app permissions for downloader role
        let downloader_apps = [
            "qbittorrent",
            "transmission",
            "deluge",
            "rutorrent",
            "sabnzbd",
            "jackett",
            "prowlarr",
        ];
        for app in downloader_apps {
            sqlx::query(
                "INSERT INTO role_app_permissions (role_id, app_name)
                 SELECT id, ? FROM roles WHERE name = 'downloader'",
            )
            .bind(app)
            .execute(pool)
            .await?;
        }

        tracing::info!("Default roles seeded");
    }

    // Check if system settings exist
    let settings_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM system_settings")
        .fetch_one(pool)
        .await?;

    if settings_count.0 == 0 {
        tracing::info!("Seeding default system settings...");

        let default_settings = [
            (
                "registration_enabled",
                "true",
                "Allow new user registration",
            ),
            (
                "registration_require_approval",
                "true",
                "Require admin approval for new registrations",
            ),
        ];

        for (key, value, description) in default_settings {
            sqlx::query(
                "INSERT INTO system_settings (key, value, description, updated_at) VALUES (?, ?, ?, datetime('now'))",
            )
            .bind(key)
            .bind(value)
            .bind(description)
            .execute(pool)
            .await?;
        }

        tracing::info!("Default system settings seeded");
    }

    Ok(())
}

/// SQL schema for creating all tables
const SCHEMA_SQL: &str = r#"
-- Users table
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    hashed_password TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT 1,
    is_admin BOOLEAN NOT NULL DEFAULT 0,
    is_approved BOOLEAN NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- Roles table
CREATE TABLE IF NOT EXISTS roles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    is_system BOOLEAN NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_roles_name ON roles(name);

-- User-Role junction table
CREATE TABLE IF NOT EXISTS user_roles (
    user_id INTEGER NOT NULL,
    role_id INTEGER NOT NULL,
    PRIMARY KEY (user_id, role_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE
);

-- Role app permissions table
CREATE TABLE IF NOT EXISTS role_app_permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    role_id INTEGER NOT NULL,
    app_name TEXT NOT NULL,
    UNIQUE(role_id, app_name),
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_role_app_permissions_role ON role_app_permissions(role_id);

-- OAuth2 clients table
CREATE TABLE IF NOT EXISTS oauth2_clients (
    client_id TEXT PRIMARY KEY,
    client_secret_hash TEXT NOT NULL,
    name TEXT NOT NULL,
    redirect_uris TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- OAuth2 authorization codes table
CREATE TABLE IF NOT EXISTS oauth2_authorization_codes (
    code TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    user_id INTEGER NOT NULL,
    redirect_uri TEXT NOT NULL,
    scope TEXT,
    code_challenge TEXT,
    code_challenge_method TEXT,
    expires_at DATETIME NOT NULL,
    used BOOLEAN NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (client_id) REFERENCES oauth2_clients(client_id),
    FOREIGN KEY (user_id) REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_oauth2_auth_codes_expires ON oauth2_authorization_codes(expires_at);

-- OAuth2 tokens table
CREATE TABLE IF NOT EXISTS oauth2_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    access_token TEXT NOT NULL UNIQUE,
    refresh_token TEXT UNIQUE,
    client_id TEXT NOT NULL,
    user_id INTEGER NOT NULL,
    scope TEXT,
    expires_at DATETIME NOT NULL,
    refresh_expires_at DATETIME,
    revoked BOOLEAN NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (client_id) REFERENCES oauth2_clients(client_id),
    FOREIGN KEY (user_id) REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_oauth2_tokens_access ON oauth2_tokens(access_token);
CREATE INDEX IF NOT EXISTS idx_oauth2_tokens_refresh ON oauth2_tokens(refresh_token);
CREATE INDEX IF NOT EXISTS idx_oauth2_tokens_expires ON oauth2_tokens(expires_at);

-- Invites table
CREATE TABLE IF NOT EXISTS invites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT NOT NULL UNIQUE,
    created_by_id INTEGER NOT NULL,
    used_by_id INTEGER,
    is_used BOOLEAN NOT NULL DEFAULT 0,
    expires_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    used_at DATETIME,
    FOREIGN KEY (created_by_id) REFERENCES users(id),
    FOREIGN KEY (used_by_id) REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_invites_code ON invites(code);

-- System settings table
CREATE TABLE IF NOT EXISTS system_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    description TEXT,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#;
