//! Migration: Create user_notification_prefs table

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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserNotificationPrefs::Table).if_exists().to_owned())
            .await
    }
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
