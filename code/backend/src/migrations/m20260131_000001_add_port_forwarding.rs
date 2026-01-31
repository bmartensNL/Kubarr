//! Migration: Add port_forwarding column to app_vpn_configs table

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AppVpnConfigs::Table)
                    .add_column(
                        ColumnDef::new(AppVpnConfigs::PortForwarding)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AppVpnConfigs::Table)
                    .drop_column(AppVpnConfigs::PortForwarding)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
#[iden = "app_vpn_configs"]
enum AppVpnConfigs {
    Table,
    #[iden = "port_forwarding"]
    PortForwarding,
}
