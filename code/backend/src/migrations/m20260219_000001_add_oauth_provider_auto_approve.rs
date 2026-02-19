//! Migration: Add auto_approve column to oauth_providers table

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(OauthProviders::Table)
                    .add_column(
                        ColumnDef::new(OauthProviders::AutoApprove)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(OauthProviders::Table)
                    .drop_column(OauthProviders::AutoApprove)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
#[iden = "oauth_providers"]
enum OauthProviders {
    Table,
    #[iden = "auto_approve"]
    AutoApprove,
}
