//! Migration: Add Cloudflare API fields to cloudflare_tunnels table

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

// New columns to add (all nullable)
const NEW_COLUMNS: &[&str] = &[
    "api_token",
    "account_id",
    "tunnel_id",
    "zone_id",
    "zone_name",
    "subdomain",
    "dns_record_id",
    "hostname",
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite requires one ALTER TABLE per column
        for col_name in NEW_COLUMNS {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("cloudflare_tunnels"))
                        .add_column(ColumnDef::new(Alias::new(*col_name)).text().null())
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for col_name in NEW_COLUMNS {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("cloudflare_tunnels"))
                        .drop_column(Alias::new(*col_name))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
