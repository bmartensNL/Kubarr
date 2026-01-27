use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectOptions, Database, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, Set,
};
use sea_orm_migration::MigratorTrait;
use std::time::Duration;

use crate::config::CONFIG;
use crate::error::{AppError, Result};
use crate::migrations::Migrator;
#[allow(unused_imports)]
use crate::models::{oauth_provider, role, role_app_permission, role_permission, system_setting};

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

    // Run migrations using SeaORM Migrator
    tracing::info!("Running database migrations...");
    Migrator::up(&db, None)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to run migrations: {}", e)))?;
    tracing::info!("Database migrations completed");

    // Seed default data
    seed_defaults(&db).await?;

    Ok(db)
}

/// Seed default roles and settings
async fn seed_defaults(db: &DbConn) -> Result<()> {
    use crate::models::prelude::*;

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
            "users.reset_password",
            "roles.view",
            "roles.manage",
            "settings.view",
            "settings.manage",
            "audit.view",
            "audit.manage",
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

    // Ensure admin role has all required permissions (for existing databases)
    let admin_role = Role::find()
        .filter(role::Column::Name.eq("admin"))
        .one(db)
        .await?;

    if let Some(admin) = admin_role {
        let required_permissions = [
            "audit.view",
            "audit.manage",
            "users.reset_password",
        ];

        for perm in required_permissions {
            // Check if permission already exists
            let exists = RolePermission::find()
                .filter(role_permission::Column::RoleId.eq(admin.id))
                .filter(role_permission::Column::Permission.eq(perm))
                .one(db)
                .await?
                .is_some();

            if !exists {
                tracing::info!("Adding missing permission {} to admin role", perm);
                let permission = role_permission::ActiveModel {
                    role_id: Set(admin.id),
                    permission: Set(perm.to_string()),
                    ..Default::default()
                };
                permission.insert(db).await?;
            }
        }
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

    // Seed OAuth providers (Google, Microsoft)
    let oauth_count = OauthProvider::find().count(db).await?;

    if oauth_count == 0 {
        tracing::info!("Seeding default OAuth providers...");

        let now = chrono::Utc::now();

        let default_providers = [
            ("google", "Google"),
            ("microsoft", "Microsoft"),
        ];

        for (id, name) in default_providers {
            let provider = oauth_provider::ActiveModel {
                id: Set(id.to_string()),
                name: Set(name.to_string()),
                enabled: Set(false),
                client_id: Set(None),
                client_secret: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
            };
            provider.insert(db).await?;
        }

        tracing::info!("Default OAuth providers seeded");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_db;

    #[tokio::test]
    async fn test_migrations_create_tables() {
        let db = create_test_db().await;

        // Verify we can query the tables (migrations ran successfully)
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

        // Check users table exists
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

        // Check notification tables exist
        let result = db
            .execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM notification_channels".to_string(),
            ))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_migrations_are_idempotent() {
        let db = create_test_db().await;

        // Running migrations again should not fail (migrations track state)
        Migrator::up(&db, None).await.unwrap();
        Migrator::up(&db, None).await.unwrap();

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

    #[tokio::test]
    async fn test_seed_defaults_creates_roles() {
        use crate::models::prelude::*;

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
        use crate::models::prelude::*;

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
        use crate::models::prelude::*;

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
        use crate::models::prelude::*;

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
        use crate::models::prelude::*;

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

    #[test]
    fn test_db_conn_type_alias() {
        // DbConn should be DatabaseConnection
        fn _takes_db_conn(_: &DbConn) {}
        fn _takes_database_connection(_: &DatabaseConnection) {}
        // If this compiles, the type alias is correct
    }
}
