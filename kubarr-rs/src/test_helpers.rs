//! Test helpers and utilities for unit and integration testing.
//!
//! This module provides common utilities for setting up test environments,
//! creating mock data, and testing database operations.

use sea_orm::{Database, DatabaseConnection, ConnectionTrait, DatabaseBackend, Statement};

/// Create an in-memory SQLite database for testing
pub async fn create_test_db() -> DatabaseConnection {
    // Use simple in-memory SQLite - each connection gets its own database
    let db_url = "sqlite::memory:";

    let db = Database::connect(db_url)
        .await
        .expect("Failed to create test database");

    // Run migrations
    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        SCHEMA_SQL.to_string(),
    ))
    .await
    .expect("Failed to run test migrations");

    db
}

/// Create a test database with seeded default data (roles, permissions)
pub async fn create_test_db_with_seed() -> DatabaseConnection {
    let db = create_test_db().await;
    seed_test_data(&db).await;
    db
}

/// Seed default test data into the database
pub async fn seed_test_data(db: &DatabaseConnection) {
    use crate::db::entities::{role, role_permission, role_app_permission};
    use sea_orm::{ActiveModelTrait, Set};

    let now = chrono::Utc::now();

    // Create default roles
    let admin_role = role::ActiveModel {
        name: Set("admin".to_string()),
        description: Set(Some("Full administrator access".to_string())),
        is_system: Set(true),
        created_at: Set(now),
        ..Default::default()
    };
    let admin = admin_role.insert(db).await.unwrap();

    let viewer_role = role::ActiveModel {
        name: Set("viewer".to_string()),
        description: Set(Some("View-only access".to_string())),
        is_system: Set(true),
        created_at: Set(now),
        ..Default::default()
    };
    let viewer = viewer_role.insert(db).await.unwrap();

    let downloader_role = role::ActiveModel {
        name: Set("downloader".to_string()),
        description: Set(Some("Download client access".to_string())),
        is_system: Set(true),
        created_at: Set(now),
        ..Default::default()
    };
    let downloader = downloader_role.insert(db).await.unwrap();

    // Add admin permissions
    let admin_permissions = [
        "apps.view", "apps.install", "apps.delete", "apps.restart",
        "storage.view", "storage.write", "storage.delete", "storage.download",
        "logs.view", "monitoring.view", "users.view", "users.manage",
        "roles.view", "roles.manage", "settings.view", "settings.manage",
    ];
    for perm in admin_permissions {
        let permission = role_permission::ActiveModel {
            role_id: Set(admin.id),
            permission: Set(perm.to_string()),
            ..Default::default()
        };
        permission.insert(db).await.unwrap();
    }

    // Add viewer permissions
    let viewer_permissions = ["apps.view", "logs.view", "monitoring.view", "storage.view", "storage.download"];
    for perm in viewer_permissions {
        let permission = role_permission::ActiveModel {
            role_id: Set(viewer.id),
            permission: Set(perm.to_string()),
            ..Default::default()
        };
        permission.insert(db).await.unwrap();
    }

    // Add viewer app permissions
    for app in ["jellyfin", "jellyseerr"] {
        let app_perm = role_app_permission::ActiveModel {
            role_id: Set(viewer.id),
            app_name: Set(app.to_string()),
            ..Default::default()
        };
        app_perm.insert(db).await.unwrap();
    }

    // Add downloader permissions
    let downloader_permissions = ["apps.view", "apps.restart", "storage.view", "storage.download"];
    for perm in downloader_permissions {
        let permission = role_permission::ActiveModel {
            role_id: Set(downloader.id),
            permission: Set(perm.to_string()),
            ..Default::default()
        };
        permission.insert(db).await.unwrap();
    }

    // Add downloader app permissions
    for app in ["qbittorrent", "transmission", "deluge"] {
        let app_perm = role_app_permission::ActiveModel {
            role_id: Set(downloader.id),
            app_name: Set(app.to_string()),
            ..Default::default()
        };
        app_perm.insert(db).await.unwrap();
    }
}

/// Create a test user and return the user model
pub async fn create_test_user(
    db: &DatabaseConnection,
    username: &str,
    email: &str,
    password: &str,
    is_approved: bool,
) -> crate::db::entities::user::Model {
    use crate::db::entities::user;
    use crate::services::security::hash_password;
    use sea_orm::{ActiveModelTrait, Set};

    let hashed = hash_password(password).unwrap();
    let now = chrono::Utc::now();

    let new_user = user::ActiveModel {
        username: Set(username.to_string()),
        email: Set(email.to_string()),
        hashed_password: Set(hashed),
        is_active: Set(true),
        is_approved: Set(is_approved),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    new_user.insert(db).await.unwrap()
}

/// Create a test user with a specific role
pub async fn create_test_user_with_role(
    db: &DatabaseConnection,
    username: &str,
    email: &str,
    password: &str,
    role_name: &str,
) -> crate::db::entities::user::Model {
    use crate::db::entities::{role, user_role};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let user = create_test_user(db, username, email, password, true).await;

    // Find the role
    let role = crate::db::entities::prelude::Role::find()
        .filter(role::Column::Name.eq(role_name))
        .one(db)
        .await
        .unwrap()
        .expect("Role not found");

    // Assign role to user
    let user_role = user_role::ActiveModel {
        user_id: Set(user.id),
        role_id: Set(role.id),
    };
    user_role.insert(db).await.unwrap();

    user
}

/// SQL schema for creating all tables (same as in db/pool.rs)
const SCHEMA_SQL: &str = r#"
-- Users table
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    hashed_password TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT 1,
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
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_test_db() {
        let db = create_test_db().await;
        assert!(db.ping().await.is_ok());
    }

    #[tokio::test]
    async fn test_create_test_db_with_seed() {
        use crate::db::entities::prelude::*;
        use sea_orm::EntityTrait;

        let db = create_test_db_with_seed().await;

        // Verify roles were created
        let roles = Role::find().all(&db).await.unwrap();
        assert_eq!(roles.len(), 3);

        let role_names: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
        assert!(role_names.contains(&"admin"));
        assert!(role_names.contains(&"viewer"));
        assert!(role_names.contains(&"downloader"));
    }

    #[tokio::test]
    async fn test_create_test_user() {
        let db = create_test_db().await;

        let user = create_test_user(&db, "testuser", "test@example.com", "password123", true).await;

        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, "test@example.com");
        assert!(user.is_active);
        assert!(user.is_approved);
    }

    #[tokio::test]
    async fn test_create_test_user_with_role() {
        use crate::db::entities::prelude::*;
        use crate::db::entities::user_role;
        use sea_orm::{EntityTrait, ColumnTrait, QueryFilter};

        let db = create_test_db_with_seed().await;

        let user = create_test_user_with_role(&db, "admin_user", "admin@example.com", "password123", "admin").await;

        // Verify user has admin role
        let user_roles = UserRole::find()
            .filter(user_role::Column::UserId.eq(user.id))
            .all(&db)
            .await
            .unwrap();

        assert_eq!(user_roles.len(), 1);
    }
}
