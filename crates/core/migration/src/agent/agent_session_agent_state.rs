//! 智能体（agent 私有）级上下文表迁移
//! Entity: [`entity::agent::session_agent_state::Model`]
//!
//! agent 私有级上下文流：
//! - 在 `(agent_id, user_id)` 维度按写入顺序持久化
//! - 每行存储一条完整业务 JSON（蒸馏后的 agent 经验/工作模式）
//! - 按 `id` 升序即写入顺序，无需额外的 `seq` 字段
//! - 当 `user_id = 0` 时表示 agent 真正跨用户的全局状态
//!   （计数器、配额、热缓存、全局开关等）
//! - 当 `user_id != 0` 时表示 agent 自身视角下的用户记忆
//!   （区别于 `t_agent_session_user_state` 的跨 agent 用户偏好）

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
                    .table(GlobalState::Table)
                    .comment("智能体（agent 私有）级上下文表（按序追加）")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(GlobalState::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("自增主键（同时承载写入顺序）"),
                    )
                    .col(
                        ColumnDef::new(GlobalState::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("所属智能体ID"),
                    )
                    .col(
                        ColumnDef::new(GlobalState::UserId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属用户ID（支持多种身份系统）"),
                    )
                    .col(
                        ColumnDef::new(GlobalState::ContentJson)
                            .text()
                            .null()
                            .comment(
                                "完整业务 JSON（蒸馏后的 agent 经验/工作模式，可空表示 null）",
                            ),
                    )
                    .col(
                        ColumnDef::new(GlobalState::ExpiresAt)
                            .date_time()
                            .null()
                            .comment("过期时间（用于 TTL 清理，可空表示不过期）"),
                    )
                    .col(
                        ColumnDef::new(GlobalState::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否已删除（软删除）"),
                    )
                    .col(
                        ColumnDef::new(GlobalState::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(GlobalState::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        // 按 (agent, user) 顺序读取的常用过滤
        if_not_exists_create_index(
            manager,
            GlobalState::Table,
            vec![GlobalState::AgentId, GlobalState::UserId],
        )
        .await?;
        if_not_exists_create_index(manager, GlobalState::Table, vec![GlobalState::ExpiresAt])
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(GlobalState::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum GlobalState {
    #[sea_orm(iden = "t_agent_session_agent_state")]
    Table,
    Id,
    AgentId,
    UserId,
    ContentJson,
    ExpiresAt,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
