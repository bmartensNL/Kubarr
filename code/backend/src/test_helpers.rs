//! Test helpers and utilities for unit and integration testing.
//!
//! This module provides common utilities for setting up test environments,
//! creating mock data, and testing database operations.

use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

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
    use crate::models::{role, role_app_permission, role_permission};
    use sea_orm::{ActiveModelTrait, Set};

    let now = chrono::Utc::now();

    // Create default roles
    let admin_role = role::ActiveModel {
        name: Set("admin".to_string()),
        description: Set(Some("Full administrator access".to_string())),
        is_system: Set(true),
        requires_2fa: Set(false),
        created_at: Set(now),
        ..Default::default()
    };
    let admin = admin_role.insert(db).await.unwrap();

    let viewer_role = role::ActiveModel {
        name: Set("viewer".to_string()),
        description: Set(Some("View-only access".to_string())),
        is_system: Set(true),
        requires_2fa: Set(false),
        created_at: Set(now),
        ..Default::default()
    };
    let viewer = viewer_role.insert(db).await.unwrap();

    let downloader_role = role::ActiveModel {
        name: Set("downloader".to_string()),
        description: Set(Some("Download client access".to_string())),
        is_system: Set(true),
        requires_2fa: Set(false),
        created_at: Set(now),
        ..Default::default()
    };
    let downloader = downloader_role.insert(db).await.unwrap();

    // Add admin permissions
    let admin_permissions = [
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
    for perm in admin_permissions {
        let permission = role_permission::ActiveModel {
            role_id: Set(admin.id),
            permission: Set(perm.to_string()),
            ..Default::default()
        };
        permission.insert(db).await.unwrap();
    }

    // Add viewer permissions
    let viewer_permissions = [
        "apps.view",
        "logs.view",
        "monitoring.view",
        "storage.view",
        "storage.download",
    ];
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
    let downloader_permissions = [
        "apps.view",
        "apps.restart",
        "storage.view",
        "storage.download",
    ];
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
) -> crate::models::user::Model {
    use crate::models::user;
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
        totp_secret: Set(None),
        totp_enabled: Set(false),
        totp_verified_at: Set(None),
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
) -> crate::models::user::Model {
    use crate::models::{role, user_role};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let user = create_test_user(db, username, email, password, true).await;

    // Find the role
    let role = crate::models::prelude::Role::find()
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

/// SQL schema for creating all tables (synced with db/pool.rs)
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

-- Audit logs table
CREATE TABLE IF NOT EXISTS audit_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    user_id INTEGER,
    username TEXT,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details TEXT,
    ip_address TEXT,
    user_agent TEXT,
    success BOOLEAN NOT NULL DEFAULT 1,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);
CREATE INDEX IF NOT EXISTS idx_audit_logs_resource_type ON audit_logs(resource_type);

-- Notification channel configuration (admin-managed)
CREATE TABLE IF NOT EXISTS notification_channels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_type TEXT NOT NULL UNIQUE,
    enabled BOOLEAN NOT NULL DEFAULT 0,
    config TEXT NOT NULL DEFAULT '{}',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_notification_channels_type ON notification_channels(channel_type);

-- Notification event settings (which events trigger notifications)
CREATE TABLE IF NOT EXISTS notification_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL UNIQUE,
    enabled BOOLEAN NOT NULL DEFAULT 0,
    severity TEXT NOT NULL DEFAULT 'info'
);

CREATE INDEX IF NOT EXISTS idx_notification_events_type ON notification_events(event_type);

-- User notification preferences
CREATE TABLE IF NOT EXISTS user_notification_prefs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    channel_type TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    destination TEXT,
    verified BOOLEAN NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, channel_type),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_notification_prefs_user ON user_notification_prefs(user_id);

-- Notification delivery log
CREATE TABLE IF NOT EXISTS notification_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER,
    channel_type TEXT NOT NULL,
    event_type TEXT NOT NULL,
    recipient TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_notification_logs_user ON notification_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_notification_logs_status ON notification_logs(status);
CREATE INDEX IF NOT EXISTS idx_notification_logs_created ON notification_logs(created_at);

-- User notifications inbox (displayed in UI)
CREATE TABLE IF NOT EXISTS user_notifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    event_type TEXT,
    severity TEXT NOT NULL DEFAULT 'info',
    read BOOLEAN NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_notifications_user ON user_notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_user_notifications_read ON user_notifications(user_id, read);
CREATE INDEX IF NOT EXISTS idx_user_notifications_created ON user_notifications(created_at);

-- OAuth provider accounts (for Google/Microsoft login linking)
CREATE TABLE IF NOT EXISTS oauth_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    provider TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    email TEXT,
    display_name TEXT,
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(provider, provider_user_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_oauth_accounts_user ON oauth_accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_oauth_accounts_provider ON oauth_accounts(provider, provider_user_id);

-- OAuth provider configuration (admin settings for Google/Microsoft)
CREATE TABLE IF NOT EXISTS oauth_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 0,
    client_id TEXT,
    client_secret TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#;
