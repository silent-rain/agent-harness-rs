//! 智能体会话级上下文表迁移
//! Entity: [`entity::agent::session_state::Model`]
//!
//! 会话级上下文流：
//! - 在单个 `session_id` 范围内按写入顺序持久化对话上下文条目
//! - 每行存储一条完整业务 JSON（含 `role` / `content` / `tool` 等字段）
//! - 按 `id` 升序即写入顺序，无需额外的 `seq` 字段
//! - 与 `t_agent_session_user_state` / `t_agent_session_agent_state`
//!   共同构成三层上下文体系（优先级：session > user > agent）
//! - `expires_at` / `deleted` 支持 TTL 与软删除

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
                    .table(SessionState::Table)
                    .comment("智能体会话级上下文表（按序追加）")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SessionState::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("自增主键（同时承载写入顺序）"),
                    )
                    .col(
                        ColumnDef::new(SessionState::SessionId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属会话ID"),
                    )
                    .col(
                        ColumnDef::new(SessionState::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("所属智能体ID（冗余）"),
                    )
                    .col(
                        ColumnDef::new(SessionState::UserId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属用户ID（冗余；支持多种身份系统）"),
                    )
                    .col(
                        ColumnDef::new(SessionState::ContentJson)
                            .text()
                            .null()
                            .comment(
                                "完整业务 JSON（含 role / content / tool 等字段，可空表示 null）",
                            ),
                    )
                    .col(
                        ColumnDef::new(SessionState::ExpiresAt)
                            .date_time()
                            .null()
                            .comment("过期时间（用于 TTL 清理，可空表示不过期）"),
                    )
                    .col(
                        ColumnDef::new(SessionState::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否已删除（软删除）"),
                    )
                    .col(
                        ColumnDef::new(SessionState::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(SessionState::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        // 按 session 顺序读取的常用过滤
        if_not_exists_create_index(manager, SessionState::Table, vec![SessionState::SessionId])
            .await?;
        if_not_exists_create_index(
            manager,
            SessionState::Table,
            vec![SessionState::AgentId, SessionState::UserId],
        )
        .await?;
        if_not_exists_create_index(manager, SessionState::Table, vec![SessionState::ExpiresAt])
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SessionState::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum SessionState {
    #[sea_orm(iden = "t_agent_session_state")]
    Table,
    Id,
    SessionId,
    AgentId,
    UserId,
    ContentJson,
    ExpiresAt,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
