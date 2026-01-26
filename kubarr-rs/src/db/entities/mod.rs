pub mod invite;
pub mod oauth2_authorization_code;
pub mod oauth2_client;
pub mod oauth2_token;
pub mod pending_2fa_challenge;
pub mod role;
pub mod role_app_permission;
pub mod role_permission;
pub mod system_setting;
pub mod user;
pub mod user_preferences;
pub mod user_role;

pub mod prelude {
    pub use super::invite::{self, Entity as Invite};
    pub use super::oauth2_authorization_code::{self, Entity as OAuth2AuthorizationCode};
    pub use super::oauth2_client::{self, Entity as OAuth2Client};
    pub use super::oauth2_token::{self, Entity as OAuth2Token};
    pub use super::pending_2fa_challenge::{self, Entity as Pending2faChallenge};
    pub use super::role::{self, Entity as Role};
    pub use super::role_app_permission::{self, Entity as RoleAppPermission};
    pub use super::role_permission::{self, Entity as RolePermission};
    pub use super::system_setting::{self, Entity as SystemSetting};
    pub use super::user::{self, Entity as User};
    pub use super::user_preferences::{self, Entity as UserPreferences};
    pub use super::user_role::{self, Entity as UserRole};
}
