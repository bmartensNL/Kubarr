//! Migration: Create invites table

use sea_orm_migration::prelude::*;

use super::m20260127_000001_create_users::Users;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Invites::Table).if_exists().to_owned())
            .await
    }
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
