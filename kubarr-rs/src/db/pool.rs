use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectOptions, ConnectionTrait, Database, DatabaseBackend,
    DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, Set, Statement,
};
use std::time::Duration;

use crate::config::CONFIG;
use crate::db::entities::{role, role_app_permission, role_permission, system_setting};
use crate::error::{AppError, Result};

pub type DbConn = DatabaseConnection;

/// Create a new database connection
pub async fn create_pool() -> Result<DbConn> {
    let db_url = CONFIG.db_url();

    tracing::info!("Connecting to database: {}", CONFIG.db_path.display());

    let mut opts = ConnectOptions::new(&db_url);
    opts.max_connections(10)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .sqlx_logging(false);

    let db = Database::connect(opts)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to connect to database: {}", e)))?;

    // Run migrations
    run_migrations(&db).await?;

    // Seed default data
    seed_defaults(&db).await?;

    Ok(db)
}

/// Run database migrations
async fn run_migrations(db: &DbConn) -> Result<()> {
    tracing::info!("Running database migrations...");

    // Execute schema SQL using raw statement
    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        SCHEMA_SQL.to_string(),
    ))
    .await
    .map_err(|e| AppError::Internal(format!("Failed to run migrations: {}", e)))?;

    // Run ALTER TABLE migrations for columns added after initial schema
    // These are safe to run multiple times - they check if column exists first
    run_alter_migrations(db).await?;

    tracing::info!("Database migrations completed");
    Ok(())
}

/// Run ALTER TABLE migrations for schema changes
async fn run_alter_migrations(db: &DbConn) -> Result<()> {
    // Helper to get column names for a table
    async fn get_columns(db: &DbConn, table: &str) -> Result<Vec<String>> {
        Ok(db
            .query_all(Statement::from_string(
                DatabaseBackend::Sqlite,
                format!("PRAGMA table_info({})", table),
            ))
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get table info: {}", e)))?
            .iter()
            .filter_map(|row| row.try_get::<String>("", "name").ok())
            .collect())
    }

    // Helper to add a column if it doesn't exist
    async fn add_column_if_missing(
        db: &DbConn,
        table: &str,
        column: &str,
        definition: &str,
        columns: &[String],
    ) -> Result<()> {
        if !columns.contains(&column.to_string()) {
            tracing::info!("Adding {} column to {} table...", column, table);
            db.execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition),
            ))
            .await
            .map_err(|e| AppError::Internal(format!("Failed to add {} column: {}", column, e)))?;
        }
        Ok(())
    }

    // Migrate roles table
    let role_columns = get_columns(db, "roles").await?;
    add_column_if_missing(
        db,
        "roles",
        "requires_2fa",
        "BOOLEAN NOT NULL DEFAULT 0",
        &role_columns,
    )
    .await?;

    // Migrate users table for 2FA support
    let user_columns = get_columns(db, "users").await?;
    add_column_if_missing(db, "users", "totp_secret", "TEXT", &user_columns).await?;
    add_column_if_missing(
        db,
        "users",
        "totp_enabled",
        "BOOLEAN NOT NULL DEFAULT 0",
        &user_columns,
    )
    .await?;
    add_column_if_missing(db, "users", "totp_verified_at", "DATETIME", &user_columns).await?;

    Ok(())
}

/// Seed default roles and settings
async fn seed_defaults(db: &DbConn) -> Result<()> {
    use crate::db::entities::prelude::*;

    // Check if roles exist
    let role_count = Role::find().count(db).await?;

    if role_count == 0 {
        tracing::info!("Seeding default roles...");

        let now = chrono::Utc::now();

        // Create default roles
        let default_roles = [
            ("admin", "Full administrator access", true),
            ("viewer", "View-only access to media apps", true),
            ("downloader", "Access to download clients", true),
        ];

        for (name, description, is_system) in default_roles {
            let new_role = role::ActiveModel {
                name: Set(name.to_string()),
                description: Set(Some(description.to_string())),
                is_system: Set(is_system),
                created_at: Set(now),
                ..Default::default()
            };
            new_role.insert(db).await?;
        }

        // Get viewer role ID
        let viewer_role = Role::find()
            .filter(role::Column::Name.eq("viewer"))
            .one(db)
            .await?
            .ok_or_else(|| AppError::Internal("Viewer role not found".to_string()))?;

        // Add app permissions for viewer role
        let viewer_apps = ["jellyfin", "jellyseerr"];
        for app in viewer_apps {
            let permission = role_app_permission::ActiveModel {
                role_id: Set(viewer_role.id),
                app_name: Set(app.to_string()),
                ..Default::default()
            };
            permission.insert(db).await?;
        }

        // Get downloader role ID
        let downloader_role = Role::find()
            .filter(role::Column::Name.eq("downloader"))
            .one(db)
            .await?
            .ok_or_else(|| AppError::Internal("Downloader role not found".to_string()))?;

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
            let permission = role_app_permission::ActiveModel {
                role_id: Set(downloader_role.id),
                app_name: Set(app.to_string()),
                ..Default::default()
            };
            permission.insert(db).await?;
        }

        // Get admin role ID
        let admin_role = Role::find()
            .filter(role::Column::Name.eq("admin"))
            .one(db)
            .await?
            .ok_or_else(|| AppError::Internal("Admin role not found".to_string()))?;

        // Add all permissions for admin role
        let all_permissions = [
            "apps.view",
            "apps.install",
            "apps.delete",
            "apps.restart",
            "storage.view",
            "storage.write",
            "storage.delete",
            "storage.download",
            "logs.view",
            "monitoring.view",
            "users.view",
            "users.manage",
            "roles.view",
            "roles.manage",
            "settings.view",
            "settings.manage",
        ];
        for perm in all_permissions {
            let permission = role_permission::ActiveModel {
                role_id: Set(admin_role.id),
                permission: Set(perm.to_string()),
                ..Default::default()
            };
            permission.insert(db).await?;
        }

        // Add permissions for viewer role
        let viewer_permissions = [
            "apps.view",
            "logs.view",
            "monitoring.view",
            "storage.view",
            "storage.download",
        ];
        for perm in viewer_permissions {
            let permission = role_permission::ActiveModel {
                role_id: Set(viewer_role.id),
                permission: Set(perm.to_string()),
                ..Default::default()
            };
            permission.insert(db).await?;
        }

        // Add permissions for downloader role
        let downloader_permissions = [
            "apps.view",
            "apps.restart",
            "storage.view",
            "storage.download",
        ];
        for perm in downloader_permissions {
            let permission = role_permission::ActiveModel {
                role_id: Set(downloader_role.id),
                permission: Set(perm.to_string()),
                ..Default::default()
            };
            permission.insert(db).await?;
        }

        tracing::info!("Default roles and permissions seeded");
    }

    // Check if system settings exist
    let settings_count = SystemSetting::find().count(db).await?;

    if settings_count == 0 {
        tracing::info!("Seeding default system settings...");

        let now = chrono::Utc::now();

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
            let setting = system_setting::ActiveModel {
                key: Set(key.to_string()),
                value: Set(value.to_string()),
                description: Set(Some(description.to_string())),
                updated_at: Set(now),
            };
            setting.insert(db).await?;
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
    is_approved BOOLEAN NOT NULL DEFAULT 0,
    totp_secret TEXT,
    totp_enabled BOOLEAN NOT NULL DEFAULT 0,
    totp_verified_at DATETIME,
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
    requires_2fa BOOLEAN NOT NULL DEFAULT 0,
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

-- User preferences table
CREATE TABLE IF NOT EXISTS user_preferences (
    user_id INTEGER PRIMARY KEY,
    theme TEXT NOT NULL DEFAULT 'system',
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Role permissions table (for granular action-level permissions)
CREATE TABLE IF NOT EXISTS role_permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    role_id INTEGER NOT NULL,
    permission TEXT NOT NULL,
    UNIQUE(role_id, permission),
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_role_permissions_role ON role_permissions(role_id);

-- Pending 2FA challenges table (for login flow)
CREATE TABLE IF NOT EXISTS pending_2fa_challenges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    challenge_token TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pending_2fa_token ON pending_2fa_challenges(challenge_token);
CREATE INDEX IF NOT EXISTS idx_pending_2fa_expires ON pending_2fa_challenges(expires_at);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_db;

    #[test]
    fn test_schema_sql_not_empty() {
        assert!(!SCHEMA_SQL.is_empty());
    }

    #[test]
    fn test_schema_sql_contains_users_table() {
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS users"));
    }

    #[test]
    fn test_schema_sql_contains_roles_table() {
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS roles"));
    }

    #[test]
    fn test_schema_sql_contains_user_roles_table() {
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS user_roles"));
    }

    #[test]
    fn test_schema_sql_contains_oauth2_tables() {
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS oauth2_clients"));
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS oauth2_authorization_codes"));
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS oauth2_tokens"));
    }

    #[test]
    fn test_schema_sql_contains_invites_table() {
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS invites"));
    }

    #[test]
    fn test_schema_sql_contains_system_settings_table() {
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS system_settings"));
    }

    #[test]
    fn test_schema_sql_contains_user_preferences_table() {
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS user_preferences"));
    }

    #[test]
    fn test_schema_sql_contains_role_permissions_tables() {
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS role_app_permissions"));
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS role_permissions"));
    }

    #[test]
    fn test_schema_sql_contains_indexes() {
        assert!(SCHEMA_SQL.contains("CREATE INDEX IF NOT EXISTS idx_users_username"));
        assert!(SCHEMA_SQL.contains("CREATE INDEX IF NOT EXISTS idx_users_email"));
        assert!(SCHEMA_SQL.contains("CREATE INDEX IF NOT EXISTS idx_roles_name"));
        assert!(SCHEMA_SQL.contains("CREATE INDEX IF NOT EXISTS idx_invites_code"));
    }

    #[test]
    fn test_schema_sql_contains_foreign_keys() {
        assert!(SCHEMA_SQL.contains("FOREIGN KEY (user_id) REFERENCES users(id)"));
        assert!(SCHEMA_SQL.contains("FOREIGN KEY (role_id) REFERENCES roles(id)"));
    }

    #[tokio::test]
    async fn test_schema_executes_on_sqlite() {
        // This test verifies the schema SQL is valid by executing it
        // We use test_helpers which already does this, but let's be explicit
        let db = create_test_db().await;

        // Verify we can query the tables
        use sea_orm::{ConnectionTrait, DatabaseBackend, EntityTrait, Statement};

        // Check users table exists by attempting a query
        let result = db
            .execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM users".to_string(),
            ))
            .await;
        assert!(result.is_ok());

        // Check roles table exists
        let result = db
            .execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM roles".to_string(),
            ))
            .await;
        assert!(result.is_ok());

        // Check oauth2 tables exist
        let result = db
            .execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM oauth2_clients".to_string(),
            ))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_seed_defaults_creates_roles() {
        use crate::db::entities::prelude::*;

        let db = create_test_db().await;

        // Verify no roles exist initially
        let count_before = Role::find().count(&db).await.unwrap();
        assert_eq!(count_before, 0);

        // Run seed_defaults
        seed_defaults(&db).await.unwrap();

        // Verify roles were created
        let roles = Role::find().all(&db).await.unwrap();
        assert_eq!(roles.len(), 3);

        let role_names: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
        assert!(role_names.contains(&"admin"));
        assert!(role_names.contains(&"viewer"));
        assert!(role_names.contains(&"downloader"));
    }

    #[tokio::test]
    async fn test_seed_defaults_creates_system_settings() {
        use crate::db::entities::prelude::*;

        let db = create_test_db().await;

        // Run seed_defaults
        seed_defaults(&db).await.unwrap();

        // Verify system settings were created
        let settings = SystemSetting::find().all(&db).await.unwrap();
        assert!(settings.len() >= 2);

        let setting_keys: Vec<&str> = settings.iter().map(|s| s.key.as_str()).collect();
        assert!(setting_keys.contains(&"registration_enabled"));
        assert!(setting_keys.contains(&"registration_require_approval"));
    }

    #[tokio::test]
    async fn test_seed_defaults_creates_role_permissions() {
        use crate::db::entities::prelude::*;

        let db = create_test_db().await;

        // Run seed_defaults
        seed_defaults(&db).await.unwrap();

        // Verify admin has all permissions
        let admin_role = Role::find()
            .filter(role::Column::Name.eq("admin"))
            .one(&db)
            .await
            .unwrap()
            .unwrap();

        let admin_perms = RolePermission::find()
            .filter(role_permission::Column::RoleId.eq(admin_role.id))
            .all(&db)
            .await
            .unwrap();

        // Admin should have many permissions
        assert!(admin_perms.len() >= 10);

        // Check specific permissions
        let perm_names: Vec<&str> = admin_perms.iter().map(|p| p.permission.as_str()).collect();
        assert!(perm_names.contains(&"apps.view"));
        assert!(perm_names.contains(&"users.manage"));
        assert!(perm_names.contains(&"settings.manage"));
    }

    #[tokio::test]
    async fn test_seed_defaults_creates_app_permissions() {
        use crate::db::entities::prelude::*;

        let db = create_test_db().await;

        // Run seed_defaults
        seed_defaults(&db).await.unwrap();

        // Verify viewer has app permissions
        let viewer_role = Role::find()
            .filter(role::Column::Name.eq("viewer"))
            .one(&db)
            .await
            .unwrap()
            .unwrap();

        let viewer_apps = RoleAppPermission::find()
            .filter(role_app_permission::Column::RoleId.eq(viewer_role.id))
            .all(&db)
            .await
            .unwrap();

        let app_names: Vec<&str> = viewer_apps.iter().map(|a| a.app_name.as_str()).collect();
        assert!(app_names.contains(&"jellyfin"));
        assert!(app_names.contains(&"jellyseerr"));

        // Verify downloader has app permissions
        let downloader_role = Role::find()
            .filter(role::Column::Name.eq("downloader"))
            .one(&db)
            .await
            .unwrap()
            .unwrap();

        let downloader_apps = RoleAppPermission::find()
            .filter(role_app_permission::Column::RoleId.eq(downloader_role.id))
            .all(&db)
            .await
            .unwrap();

        let app_names: Vec<&str> = downloader_apps
            .iter()
            .map(|a| a.app_name.as_str())
            .collect();
        assert!(app_names.contains(&"qbittorrent"));
        assert!(app_names.contains(&"transmission"));
    }

    #[tokio::test]
    async fn test_seed_defaults_is_idempotent() {
        use crate::db::entities::prelude::*;

        let db = create_test_db().await;

        // Run seed_defaults twice
        seed_defaults(&db).await.unwrap();
        seed_defaults(&db).await.unwrap();

        // Should still have exactly 3 roles
        let roles = Role::find().all(&db).await.unwrap();
        assert_eq!(roles.len(), 3);

        // Should still have 2 system settings
        let settings = SystemSetting::find().all(&db).await.unwrap();
        assert_eq!(settings.len(), 2);
    }

    #[tokio::test]
    async fn test_run_migrations_is_idempotent() {
        let db = create_test_db().await;

        // Running migrations again should not fail (IF NOT EXISTS clauses)
        run_migrations(&db).await.unwrap();
        run_migrations(&db).await.unwrap();

        // Tables should still exist and be usable
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        let result = db
            .execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT 1 FROM users LIMIT 1".to_string(),
            ))
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_db_conn_type_alias() {
        // DbConn should be DatabaseConnection
        fn _takes_db_conn(_: &DbConn) {}
        fn _takes_database_connection(_: &DatabaseConnection) {}
        // If this compiles, the type alias is correct
    }
}
