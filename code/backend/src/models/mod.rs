pub mod audit_log;
pub mod invite;
pub mod notification_channel;
pub mod notification_event;
pub mod notification_log;
pub mod oauth_account;
pub mod oauth_provider;
pub mod pending_2fa_challenge;
pub mod role;
pub mod role_app_permission;
pub mod session;
pub mod role_permission;
pub mod system_setting;
pub mod user;
pub mod user_notification;
pub mod user_notification_pref;
pub mod user_preferences;
pub mod user_role;

#[allow(unused_imports)]
pub mod prelude {
    pub use super::audit_log::{self, Entity as AuditLog};
    pub use super::invite::{self, Entity as Invite};
    pub use super::notification_channel::{self, Entity as NotificationChannel};
    pub use super::notification_event::{self, Entity as NotificationEvent};
    pub use super::notification_log::{self, Entity as NotificationLog};
    pub use super::oauth_account::{self, Entity as OauthAccount};
    pub use super::oauth_provider::{self, Entity as OauthProvider};
    pub use super::pending_2fa_challenge::{self, Entity as Pending2faChallenge};
    pub use super::role::{self, Entity as Role};
    pub use super::role_app_permission::{self, Entity as RoleAppPermission};
    pub use super::role_permission::{self, Entity as RolePermission};
    pub use super::session::{self, Entity as Session};
    pub use super::system_setting::{self, Entity as SystemSetting};
    pub use super::user::{self, Entity as User};
    pub use super::user_notification::{self, Entity as UserNotification};
    pub use super::user_notification_pref::{self, Entity as UserNotificationPref};
    pub use super::user_preferences::{self, Entity as UserPreferences};
    pub use super::user_role::{self, Entity as UserRole};
}
