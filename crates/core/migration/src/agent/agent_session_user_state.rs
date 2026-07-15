//! 智能体用户级上下文表迁移
//! Entity: [`entity::agent::session_user_state::Model`]
//!
//! 用户级上下文流（**跨 agent**）：
//! - 在同一个 `user_id` 范围内跨 agent、跨 session 按写入顺序持久化
//! - 每行存储一条完整业务 JSON（蒸馏后的用户偏好/画像/事实）
//! - 按 `id` 升序即写入顺序，无需额外的 `seq` 字段
//! - 适用于：用户偏好、用户画像增量、长期记忆指针等
//!
//! 不携带 `agent_id`：
//! - 一个用户在多个 agent 之间共享的偏好/画像应当跨 agent 可见
//! - 如需 agent 私有的用户级记忆，请使用 `t_agent_session_agent_state`

use sea_orm::{
    DeriveIden, DeriveMigrationName,
    sea_query::{ColumnDef, Expr, Table},
};
use sea_orm_migration::{DbErr, MigrationTrait, SchemaManager, async_trait};

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserState::Table)
                    .comment("智能体用户级上下文表（按序追加，跨 agent）")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserState::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("自增主键（同时承载写入顺序）"),
                    )
                    .col(
                        ColumnDef::new(UserState::UserId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属用户ID（支持多种身份系统）"),
                    )
                    .col(
                        ColumnDef::new(UserState::ContentJson)
                            .text()
                            .null()
                            .comment("完整业务 JSON（蒸馏后的用户偏好/画像/事实，可空表示 null）"),
                    )
                    .col(
                        ColumnDef::new(UserState::ExpiresAt)
                            .date_time()
                            .null()
                            .comment("过期时间（用于 TTL 清理，可空表示不过期）"),
                    )
                    .col(
                        ColumnDef::new(UserState::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否已删除（软删除）"),
                    )
                    .col(
                        ColumnDef::new(UserState::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(UserState::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        // 按 user 顺序读取的常用过滤
        if_not_exists_create_index(manager, UserState::Table, vec![UserState::UserId]).await?;
        if_not_exists_create_index(manager, UserState::Table, vec![UserState::ExpiresAt]).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserState::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum UserState {
    #[sea_orm(iden = "t_agent_session_user_state")]
    Table,
    Id,
    UserId,
    ContentJson,
    ExpiresAt,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
