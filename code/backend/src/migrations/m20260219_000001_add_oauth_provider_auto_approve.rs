//! Migration: Add auto_approve column to oauth_providers table
//!
//! For fresh installs the column already exists (added in
//! `m20260127_000007_create_oauth_providers`). This migration is only a no-op
//! in that case; it adds the column to databases created before the original
//! migration was updated.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Skip if the column was already created by the original table migration
        if manager
            .has_column("oauth_providers", "auto_approve")
            .await?
        {
            return Ok(());
        }

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
        // Only drop if the column exists (fresh installs should not touch the
        // original table definition on rollback)
        if !manager
            .has_column("oauth_providers", "auto_approve")
            .await?
        {
            return Ok(());
        }

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
