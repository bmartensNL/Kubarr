//! Permission system with type-safe authorization extractors
//!
//! Usage in handlers:
//! ```ignore
//! use crate::middleware::{Authorized, permissions::*};
//!
//! async fn list_users(
//!     Authorized(user): Authorized<UsersView>,
//!     State(state): State<AppState>,
//! ) -> Result<Json<Vec<User>>> {
//!     // Permission already verified - just use user
//! }
//! ```

use std::marker::PhantomData;

use axum::{async_trait, extract::FromRequestParts, http::request::Parts};

use crate::error::AppError;
use crate::middleware::AuthenticatedUser;
use crate::models::user;

/// Trait for permission marker types
pub trait Permission: Send + Sync + 'static {
    /// The permission string (e.g., "users.view")
    const NAME: &'static str;
}

/// Macro to define permission types
///
/// Creates zero-sized marker types that implement `Permission`
macro_rules! define_permissions {
    ($($(#[$meta:meta])* $name:ident => $perm:expr),* $(,)?) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, Copy)]
            pub struct $name;

            impl Permission for $name {
                const NAME: &'static str = $perm;
            }
        )*
    };
}

// Define all application permissions
define_permissions! {
    // User management
    /// View users list and details
    UsersView => "users.view",
    /// Create, update, delete users
    UsersManage => "users.manage",
    /// Reset other users' passwords
    UsersResetPassword => "users.reset_password",

    // Role management
    /// View roles list and details
    RolesView => "roles.view",
    /// Create, update, delete roles
    RolesManage => "roles.manage",

    // App management
    /// View installed apps
    AppsView => "apps.view",
    /// Install new apps
    AppsInstall => "apps.install",
    /// Delete/uninstall apps
    AppsDelete => "apps.delete",
    /// Restart apps
    AppsRestart => "apps.restart",

    // Storage management
    /// Browse and view storage
    StorageView => "storage.view",
    /// Create directories, upload files
    StorageWrite => "storage.write",
    /// Delete files and directories
    StorageDelete => "storage.delete",
    /// Download files
    StorageDownload => "storage.download",

    // Logs
    /// View application logs
    LogsView => "logs.view",

    // Monitoring
    /// View metrics and monitoring data
    MonitoringView => "monitoring.view",

    // Settings
    /// View system settings
    SettingsView => "settings.view",
    /// Modify system settings
    SettingsManage => "settings.manage",

    // Audit
    /// View audit logs
    AuditView => "audit.view",
    /// Manage audit logs (clear old entries)
    AuditManage => "audit.manage",

    // Notifications
    /// View notifications
    NotificationsView => "notifications.view",
    /// Manage notification settings
    NotificationsManage => "notifications.manage",

    // Networking
    /// View network topology
    NetworkingView => "networking.view",
}

/// Extractor that requires a specific permission
///
/// This extractor verifies that the authenticated user has the required
/// permission before the handler is called. If the permission check fails,
/// a 403 Forbidden error is returned.
///
/// # Example
/// ```ignore
/// async fn delete_user(
///     Authorized(user): Authorized<UsersManage>,
///     Path(id): Path<i64>,
/// ) -> Result<()> {
///     // User is guaranteed to have "users.manage" permission
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Authorized<P: Permission>(pub user::Model, PhantomData<P>);

impl<P: Permission> Authorized<P> {
    /// Get the authenticated user
    pub fn user(&self) -> &user::Model {
        &self.0
    }

    /// Get the user ID
    pub fn user_id(&self) -> i64 {
        self.0.id
    }
}

#[async_trait]
impl<S, P> FromRequestParts<S> for Authorized<P>
where
    S: Send + Sync,
    P: Permission,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Get authenticated user from extensions (set by auth middleware)
        let auth_user = parts
            .extensions
            .get::<AuthenticatedUser>()
            .ok_or_else(|| AppError::Unauthorized("Authentication required".to_string()))?;

        // Check if user has the required permission
        if !auth_user.has_permission(P::NAME) {
            return Err(AppError::Forbidden(format!(
                "Permission denied: {} required",
                P::NAME
            )));
        }

        Ok(Authorized(auth_user.user.clone(), PhantomData))
    }
}

/// Extractor for any authenticated user (no specific permission required)
///
/// Use this when you just need to verify the user is authenticated
/// but don't need a specific permission.
#[derive(Debug, Clone)]
pub struct Authenticated(pub user::Model);

impl Authenticated {
    /// Get the authenticated user
    pub fn user(&self) -> &user::Model {
        &self.0
    }

    /// Get the user ID
    pub fn user_id(&self) -> i64 {
        self.0.id
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for Authenticated
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_user = parts
            .extensions
            .get::<AuthenticatedUser>()
            .ok_or_else(|| AppError::Unauthorized("Authentication required".to_string()))?;

        Ok(Authenticated(auth_user.user.clone()))
    }
}
