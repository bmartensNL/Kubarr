//! Initial schema migration - creates all tables for Kubarr
//!
//! This migration creates the complete database schema including:
//! - Users and authentication (users, roles, user_roles, role_permissions)
//! - OAuth2 server (oauth2_clients, oauth2_authorization_codes, oauth2_tokens)
//! - OAuth providers (oauth_accounts, oauth_providers)
//! - 2FA support (pending_2fa_challenges)
//! - System configuration (system_settings, user_preferences)
//! - Invitations (invites)
//! - Audit logging (audit_logs)
//! - Notifications (notification_channels, notification_events, notification_logs, etc.)

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // =====================================================================
        // Core User Tables
        // =====================================================================

        // Users table
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Users::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Users::Username)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Users::Email)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Users::HashedPassword).string().not_null())
                    .col(
                        ColumnDef::new(Users::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Users::IsApproved)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Users::TotpSecret).string().null())
                    .col(
                        ColumnDef::new(Users::TotpEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Users::TotpVerifiedAt).date_time().null())
                    .col(ColumnDef::new(Users::CreatedAt).date_time().not_null())
                    .col(ColumnDef::new(Users::UpdatedAt).date_time().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_users_username")
                    .table(Users::Table)
                    .col(Users::Username)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_users_email")
                    .table(Users::Table)
                    .col(Users::Email)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // Roles table
        manager
            .create_table(
                Table::create()
                    .table(Roles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Roles::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Roles::Name).string().not_null().unique_key())
                    .col(ColumnDef::new(Roles::Description).string().null())
                    .col(
                        ColumnDef::new(Roles::IsSystem)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Roles::Requires2fa)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Roles::CreatedAt).date_time().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_roles_name")
                    .table(Roles::Table)
                    .col(Roles::Name)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // User-Role junction table
        manager
            .create_table(
                Table::create()
                    .table(UserRoles::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(UserRoles::UserId).integer().not_null())
                    .col(ColumnDef::new(UserRoles::RoleId).integer().not_null())
                    .primary_key(
                        Index::create()
                            .col(UserRoles::UserId)
                            .col(UserRoles::RoleId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserRoles::Table, UserRoles::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserRoles::Table, UserRoles::RoleId)
                            .to(Roles::Table, Roles::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Role app permissions table
        manager
            .create_table(
                Table::create()
                    .table(RoleAppPermissions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RoleAppPermissions::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RoleAppPermissions::RoleId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RoleAppPermissions::AppName)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(RoleAppPermissions::Table, RoleAppPermissions::RoleId)
                            .to(Roles::Table, Roles::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_role_app_permissions_role")
                    .table(RoleAppPermissions::Table)
                    .col(RoleAppPermissions::RoleId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_role_app_permissions_unique")
                    .table(RoleAppPermissions::Table)
                    .col(RoleAppPermissions::RoleId)
                    .col(RoleAppPermissions::AppName)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // Role permissions table (granular action-level permissions)
        manager
            .create_table(
                Table::create()
                    .table(RolePermissions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RolePermissions::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RolePermissions::RoleId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RolePermissions::Permission)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(RolePermissions::Table, RolePermissions::RoleId)
                            .to(Roles::Table, Roles::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_role_permissions_role")
                    .table(RolePermissions::Table)
                    .col(RolePermissions::RoleId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_role_permissions_unique")
                    .table(RolePermissions::Table)
                    .col(RolePermissions::RoleId)
                    .col(RolePermissions::Permission)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // =====================================================================
        // OAuth2 Server Tables
        // =====================================================================

        // OAuth2 clients table
        manager
            .create_table(
                Table::create()
                    .table(Oauth2Clients::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Oauth2Clients::ClientId)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Oauth2Clients::ClientSecretHash)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Oauth2Clients::Name).string().not_null())
                    .col(
                        ColumnDef::new(Oauth2Clients::RedirectUris)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Oauth2Clients::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // OAuth2 authorization codes table
        manager
            .create_table(
                Table::create()
                    .table(Oauth2AuthorizationCodes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::Code)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::ClientId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::UserId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::RedirectUri)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Oauth2AuthorizationCodes::Scope).string().null())
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::CodeChallenge)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::CodeChallengeMethod)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::ExpiresAt)
                            .date_time()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::Used)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Oauth2AuthorizationCodes::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                Oauth2AuthorizationCodes::Table,
                                Oauth2AuthorizationCodes::ClientId,
                            )
                            .to(Oauth2Clients::Table, Oauth2Clients::ClientId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                Oauth2AuthorizationCodes::Table,
                                Oauth2AuthorizationCodes::UserId,
                            )
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_oauth2_auth_codes_expires")
                    .table(Oauth2AuthorizationCodes::Table)
                    .col(Oauth2AuthorizationCodes::ExpiresAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // OAuth2 tokens table
        manager
            .create_table(
                Table::create()
                    .table(Oauth2Tokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Oauth2Tokens::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Oauth2Tokens::AccessToken)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Oauth2Tokens::RefreshToken)
                            .string()
                            .null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Oauth2Tokens::ClientId).string().not_null())
                    .col(ColumnDef::new(Oauth2Tokens::UserId).integer().not_null())
                    .col(ColumnDef::new(Oauth2Tokens::Scope).string().null())
                    .col(
                        ColumnDef::new(Oauth2Tokens::ExpiresAt)
                            .date_time()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Oauth2Tokens::RefreshExpiresAt)
                            .date_time()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Oauth2Tokens::Revoked)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Oauth2Tokens::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Oauth2Tokens::Table, Oauth2Tokens::ClientId)
                            .to(Oauth2Clients::Table, Oauth2Clients::ClientId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Oauth2Tokens::Table, Oauth2Tokens::UserId)
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_oauth2_tokens_access")
                    .table(Oauth2Tokens::Table)
                    .col(Oauth2Tokens::AccessToken)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_oauth2_tokens_refresh")
                    .table(Oauth2Tokens::Table)
                    .col(Oauth2Tokens::RefreshToken)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_oauth2_tokens_expires")
                    .table(Oauth2Tokens::Table)
                    .col(Oauth2Tokens::ExpiresAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // =====================================================================
        // OAuth Provider Tables (for Google/Microsoft login)
        // =====================================================================

        // OAuth accounts table
        manager
            .create_table(
                Table::create()
                    .table(OauthAccounts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OauthAccounts::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OauthAccounts::UserId).integer().not_null())
                    .col(ColumnDef::new(OauthAccounts::Provider).string().not_null())
                    .col(
                        ColumnDef::new(OauthAccounts::ProviderUserId)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(OauthAccounts::Email).string().null())
                    .col(ColumnDef::new(OauthAccounts::DisplayName).string().null())
                    .col(ColumnDef::new(OauthAccounts::AccessToken).string().null())
                    .col(ColumnDef::new(OauthAccounts::RefreshToken).string().null())
                    .col(
                        ColumnDef::new(OauthAccounts::TokenExpiresAt)
                            .date_time()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(OauthAccounts::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OauthAccounts::UpdatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(OauthAccounts::Table, OauthAccounts::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_oauth_accounts_user")
                    .table(OauthAccounts::Table)
                    .col(OauthAccounts::UserId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_oauth_accounts_provider")
                    .table(OauthAccounts::Table)
                    .col(OauthAccounts::Provider)
                    .col(OauthAccounts::ProviderUserId)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // OAuth providers configuration table
        manager
            .create_table(
                Table::create()
                    .table(OauthProviders::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OauthProviders::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OauthProviders::Name).string().not_null())
                    .col(
                        ColumnDef::new(OauthProviders::Enabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(OauthProviders::ClientId).string().null())
                    .col(ColumnDef::new(OauthProviders::ClientSecret).string().null())
                    .col(
                        ColumnDef::new(OauthProviders::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OauthProviders::UpdatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // =====================================================================
        // 2FA Support Tables
        // =====================================================================

        // Pending 2FA challenges table
        manager
            .create_table(
                Table::create()
                    .table(Pending2faChallenges::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Pending2faChallenges::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Pending2faChallenges::UserId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Pending2faChallenges::ChallengeToken)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Pending2faChallenges::ExpiresAt)
                            .date_time()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Pending2faChallenges::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Pending2faChallenges::Table, Pending2faChallenges::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_pending_2fa_token")
                    .table(Pending2faChallenges::Table)
                    .col(Pending2faChallenges::ChallengeToken)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_pending_2fa_expires")
                    .table(Pending2faChallenges::Table)
                    .col(Pending2faChallenges::ExpiresAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // =====================================================================
        // System Configuration Tables
        // =====================================================================

        // System settings table
        manager
            .create_table(
                Table::create()
                    .table(SystemSettings::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SystemSettings::Key)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SystemSettings::Value).string().not_null())
                    .col(ColumnDef::new(SystemSettings::Description).string().null())
                    .col(
                        ColumnDef::new(SystemSettings::UpdatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // User preferences table
        manager
            .create_table(
                Table::create()
                    .table(UserPreferences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserPreferences::UserId)
                            .integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::Theme)
                            .string()
                            .not_null()
                            .default("system"),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::UpdatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserPreferences::Table, UserPreferences::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // =====================================================================
        // Invitations Table
        // =====================================================================

        // Invites table
        manager
            .create_table(
                Table::create()
                    .table(Invites::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Invites::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Invites::Code)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Invites::CreatedById).integer().not_null())
                    .col(ColumnDef::new(Invites::UsedById).integer().null())
                    .col(
                        ColumnDef::new(Invites::IsUsed)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Invites::ExpiresAt).date_time().null())
                    .col(ColumnDef::new(Invites::CreatedAt).date_time().not_null())
                    .col(ColumnDef::new(Invites::UsedAt).date_time().null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Invites::Table, Invites::CreatedById)
                            .to(Users::Table, Users::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Invites::Table, Invites::UsedById)
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_invites_code")
                    .table(Invites::Table)
                    .col(Invites::Code)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // =====================================================================
        // Audit Logging Table
        // =====================================================================

        // Audit logs table
        manager
            .create_table(
                Table::create()
                    .table(AuditLogs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuditLogs::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AuditLogs::Timestamp).date_time().not_null())
                    .col(ColumnDef::new(AuditLogs::UserId).integer().null())
                    .col(ColumnDef::new(AuditLogs::Username).string().null())
                    .col(ColumnDef::new(AuditLogs::Action).string().not_null())
                    .col(ColumnDef::new(AuditLogs::ResourceType).string().not_null())
                    .col(ColumnDef::new(AuditLogs::ResourceId).string().null())
                    .col(ColumnDef::new(AuditLogs::Details).string().null())
                    .col(ColumnDef::new(AuditLogs::IpAddress).string().null())
                    .col(ColumnDef::new(AuditLogs::UserAgent).string().null())
                    .col(
                        ColumnDef::new(AuditLogs::Success)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(AuditLogs::ErrorMessage).string().null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_timestamp")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::Timestamp)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_user_id")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::UserId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_action")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::Action)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_resource_type")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::ResourceType)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // =====================================================================
        // Notification Tables
        // =====================================================================

        // Notification channels table
        manager
            .create_table(
                Table::create()
                    .table(NotificationChannels::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NotificationChannels::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::ChannelType)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::Enabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::Config)
                            .string()
                            .not_null()
                            .default("{}"),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::UpdatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notification_channels_type")
                    .table(NotificationChannels::Table)
                    .col(NotificationChannels::ChannelType)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // Notification events table
        manager
            .create_table(
                Table::create()
                    .table(NotificationEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NotificationEvents::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NotificationEvents::EventType)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(NotificationEvents::Enabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(NotificationEvents::Severity)
                            .string()
                            .not_null()
                            .default("info"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notification_events_type")
                    .table(NotificationEvents::Table)
                    .col(NotificationEvents::EventType)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // User notification preferences table
        manager
            .create_table(
                Table::create()
                    .table(UserNotificationPrefs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserNotificationPrefs::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserNotificationPrefs::UserId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserNotificationPrefs::ChannelType)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserNotificationPrefs::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(UserNotificationPrefs::Destination)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserNotificationPrefs::Verified)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(UserNotificationPrefs::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserNotificationPrefs::UpdatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserNotificationPrefs::Table, UserNotificationPrefs::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_notification_prefs_user")
                    .table(UserNotificationPrefs::Table)
                    .col(UserNotificationPrefs::UserId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_notification_prefs_unique")
                    .table(UserNotificationPrefs::Table)
                    .col(UserNotificationPrefs::UserId)
                    .col(UserNotificationPrefs::ChannelType)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // Notification logs table
        manager
            .create_table(
                Table::create()
                    .table(NotificationLogs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NotificationLogs::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(NotificationLogs::UserId).integer().null())
                    .col(
                        ColumnDef::new(NotificationLogs::ChannelType)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationLogs::EventType)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(NotificationLogs::Recipient).string().null())
                    .col(
                        ColumnDef::new(NotificationLogs::Status)
                            .string()
                            .not_null()
                            .default("pending"),
                    )
                    .col(
                        ColumnDef::new(NotificationLogs::ErrorMessage)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(NotificationLogs::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(NotificationLogs::Table, NotificationLogs::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notification_logs_user")
                    .table(NotificationLogs::Table)
                    .col(NotificationLogs::UserId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notification_logs_status")
                    .table(NotificationLogs::Table)
                    .col(NotificationLogs::Status)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_notification_logs_created")
                    .table(NotificationLogs::Table)
                    .col(NotificationLogs::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // User notifications inbox table
        manager
            .create_table(
                Table::create()
                    .table(UserNotifications::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserNotifications::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserNotifications::UserId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserNotifications::Title)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserNotifications::Message)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(UserNotifications::EventType).string().null())
                    .col(
                        ColumnDef::new(UserNotifications::Severity)
                            .string()
                            .not_null()
                            .default("info"),
                    )
                    .col(
                        ColumnDef::new(UserNotifications::Read)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(UserNotifications::CreatedAt)
                            .date_time()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserNotifications::Table, UserNotifications::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_notifications_user")
                    .table(UserNotifications::Table)
                    .col(UserNotifications::UserId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_notifications_read")
                    .table(UserNotifications::Table)
                    .col(UserNotifications::UserId)
                    .col(UserNotifications::Read)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_notifications_created")
                    .table(UserNotifications::Table)
                    .col(UserNotifications::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop tables in reverse order (respecting foreign key dependencies)
        manager
            .drop_table(Table::drop().table(UserNotifications::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(NotificationLogs::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(UserNotificationPrefs::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(NotificationEvents::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(NotificationChannels::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AuditLogs::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Invites::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(UserPreferences::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(SystemSettings::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Pending2faChallenges::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(OauthProviders::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(OauthAccounts::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Oauth2Tokens::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Oauth2AuthorizationCodes::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Oauth2Clients::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(RolePermissions::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(RoleAppPermissions::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(UserRoles::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Roles::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).if_exists().to_owned())
            .await?;

        Ok(())
    }
}

// ============================================================================
// Table Identifiers (Iden enums)
// ============================================================================

#[derive(Iden)]
enum Users {
    Table,
    Id,
    Username,
    Email,
    #[iden = "hashed_password"]
    HashedPassword,
    #[iden = "is_active"]
    IsActive,
    #[iden = "is_approved"]
    IsApproved,
    #[iden = "totp_secret"]
    TotpSecret,
    #[iden = "totp_enabled"]
    TotpEnabled,
    #[iden = "totp_verified_at"]
    TotpVerifiedAt,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

#[derive(Iden)]
enum Roles {
    Table,
    Id,
    Name,
    Description,
    #[iden = "is_system"]
    IsSystem,
    #[iden = "requires_2fa"]
    Requires2fa,
    #[iden = "created_at"]
    CreatedAt,
}

#[derive(Iden)]
#[iden = "user_roles"]
enum UserRoles {
    Table,
    #[iden = "user_id"]
    UserId,
    #[iden = "role_id"]
    RoleId,
}

#[derive(Iden)]
#[iden = "role_app_permissions"]
enum RoleAppPermissions {
    Table,
    Id,
    #[iden = "role_id"]
    RoleId,
    #[iden = "app_name"]
    AppName,
}

#[derive(Iden)]
#[iden = "role_permissions"]
enum RolePermissions {
    Table,
    Id,
    #[iden = "role_id"]
    RoleId,
    Permission,
}

#[derive(Iden)]
#[iden = "oauth2_clients"]
enum Oauth2Clients {
    Table,
    #[iden = "client_id"]
    ClientId,
    #[iden = "client_secret_hash"]
    ClientSecretHash,
    Name,
    #[iden = "redirect_uris"]
    RedirectUris,
    #[iden = "created_at"]
    CreatedAt,
}

#[derive(Iden)]
#[iden = "oauth2_authorization_codes"]
enum Oauth2AuthorizationCodes {
    Table,
    Code,
    #[iden = "client_id"]
    ClientId,
    #[iden = "user_id"]
    UserId,
    #[iden = "redirect_uri"]
    RedirectUri,
    Scope,
    #[iden = "code_challenge"]
    CodeChallenge,
    #[iden = "code_challenge_method"]
    CodeChallengeMethod,
    #[iden = "expires_at"]
    ExpiresAt,
    Used,
    #[iden = "created_at"]
    CreatedAt,
}

#[derive(Iden)]
#[iden = "oauth2_tokens"]
enum Oauth2Tokens {
    Table,
    Id,
    #[iden = "access_token"]
    AccessToken,
    #[iden = "refresh_token"]
    RefreshToken,
    #[iden = "client_id"]
    ClientId,
    #[iden = "user_id"]
    UserId,
    Scope,
    #[iden = "expires_at"]
    ExpiresAt,
    #[iden = "refresh_expires_at"]
    RefreshExpiresAt,
    Revoked,
    #[iden = "created_at"]
    CreatedAt,
}

#[derive(Iden)]
#[iden = "oauth_accounts"]
enum OauthAccounts {
    Table,
    Id,
    #[iden = "user_id"]
    UserId,
    Provider,
    #[iden = "provider_user_id"]
    ProviderUserId,
    Email,
    #[iden = "display_name"]
    DisplayName,
    #[iden = "access_token"]
    AccessToken,
    #[iden = "refresh_token"]
    RefreshToken,
    #[iden = "token_expires_at"]
    TokenExpiresAt,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

#[derive(Iden)]
#[iden = "oauth_providers"]
enum OauthProviders {
    Table,
    Id,
    Name,
    Enabled,
    #[iden = "client_id"]
    ClientId,
    #[iden = "client_secret"]
    ClientSecret,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

#[derive(Iden)]
#[iden = "pending_2fa_challenges"]
enum Pending2faChallenges {
    Table,
    Id,
    #[iden = "user_id"]
    UserId,
    #[iden = "challenge_token"]
    ChallengeToken,
    #[iden = "expires_at"]
    ExpiresAt,
    #[iden = "created_at"]
    CreatedAt,
}

#[derive(Iden)]
#[iden = "system_settings"]
enum SystemSettings {
    Table,
    Key,
    Value,
    Description,
    #[iden = "updated_at"]
    UpdatedAt,
}

#[derive(Iden)]
#[iden = "user_preferences"]
enum UserPreferences {
    Table,
    #[iden = "user_id"]
    UserId,
    Theme,
    #[iden = "updated_at"]
    UpdatedAt,
}

#[derive(Iden)]
enum Invites {
    Table,
    Id,
    Code,
    #[iden = "created_by_id"]
    CreatedById,
    #[iden = "used_by_id"]
    UsedById,
    #[iden = "is_used"]
    IsUsed,
    #[iden = "expires_at"]
    ExpiresAt,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "used_at"]
    UsedAt,
}

#[derive(Iden)]
#[iden = "audit_logs"]
enum AuditLogs {
    Table,
    Id,
    Timestamp,
    #[iden = "user_id"]
    UserId,
    Username,
    Action,
    #[iden = "resource_type"]
    ResourceType,
    #[iden = "resource_id"]
    ResourceId,
    Details,
    #[iden = "ip_address"]
    IpAddress,
    #[iden = "user_agent"]
    UserAgent,
    Success,
    #[iden = "error_message"]
    ErrorMessage,
}

#[derive(Iden)]
#[iden = "notification_channels"]
enum NotificationChannels {
    Table,
    Id,
    #[iden = "channel_type"]
    ChannelType,
    Enabled,
    Config,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

#[derive(Iden)]
#[iden = "notification_events"]
enum NotificationEvents {
    Table,
    Id,
    #[iden = "event_type"]
    EventType,
    Enabled,
    Severity,
}

#[derive(Iden)]
#[iden = "user_notification_prefs"]
enum UserNotificationPrefs {
    Table,
    Id,
    #[iden = "user_id"]
    UserId,
    #[iden = "channel_type"]
    ChannelType,
    Enabled,
    Destination,
    Verified,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

#[derive(Iden)]
#[iden = "notification_logs"]
enum NotificationLogs {
    Table,
    Id,
    #[iden = "user_id"]
    UserId,
    #[iden = "channel_type"]
    ChannelType,
    #[iden = "event_type"]
    EventType,
    Recipient,
    Status,
    #[iden = "error_message"]
    ErrorMessage,
    #[iden = "created_at"]
    CreatedAt,
}

#[derive(Iden)]
#[iden = "user_notifications"]
enum UserNotifications {
    Table,
    Id,
    #[iden = "user_id"]
    UserId,
    Title,
    Message,
    #[iden = "event_type"]
    EventType,
    Severity,
    Read,
    #[iden = "created_at"]
    CreatedAt,
}
