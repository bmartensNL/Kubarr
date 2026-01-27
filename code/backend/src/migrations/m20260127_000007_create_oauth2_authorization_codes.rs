//! Migration: Create oauth2_authorization_codes table

use sea_orm_migration::prelude::*;

use super::m20260127_000001_create_users::Users;
use super::m20260127_000006_create_oauth2_clients::Oauth2Clients;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Oauth2AuthorizationCodes::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
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
