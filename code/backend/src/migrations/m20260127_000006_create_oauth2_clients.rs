//! Migration: Create oauth2_clients table

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Oauth2Clients::Table).if_exists().to_owned())
            .await
    }
}

#[derive(Iden)]
#[iden = "oauth2_clients"]
pub enum Oauth2Clients {
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
