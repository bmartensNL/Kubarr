//! Migration: Create pending_2fa_challenges table

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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Pending2faChallenges::Table).if_exists().to_owned())
            .await
    }
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
