//! Migration: Create oauth2_tokens table

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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Oauth2Tokens::Table).if_exists().to_owned())
            .await
    }
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
