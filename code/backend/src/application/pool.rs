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
            "app.*", // Access to all apps via proxy
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
            "networking.view",
            "networking.manage",
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
            "app.*", // Access to all apps via proxy
            "audit.view",
            "audit.manage",
            "users.reset_password",
            "networking.view",
            "networking.manage",
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
