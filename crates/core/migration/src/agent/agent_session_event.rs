//! 智能体会话事件流表迁移
//! Entity: [`entity::agent::session_event::Model`]
//!
//! 会话事件流（Append-only Event Log），合并并取代原有：
//! - `t_agent_memory_stream`（流水记忆）
//! - `t_agent_tool_audit_log`（工具调用审计）
//! - `t_agent_run_record` 中的运行级事件
//!
//! 设计要点：
//! - 仅追加写入（append-only），按 session 维度顺序读取
//! - 顺序由自增主键 `id` 保证（已天然单调），不再单独维护 `seq`
//! - `payload_json` 承载完整事件内容（结构对应 `AgentStreamEvent`）
//! - 通过 `event_type` 二级筛选（user_message / assistant_message /
//!   tool_call / tool_result / planning / error / system）
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
                    .table(SessionEvent::Table)
                    .comment("智能体会话事件流表（append-only）")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SessionEvent::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("自增主键，同时承担会话内事件顺序"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::SessionId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属会话ID"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("所属智能体ID（冗余，便于免 join 过滤）"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::UserId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属用户ID（冗余，支持多种身份系统）"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::RunId)
                            .string()
                            .string_len(64)
                            .null()
                            .comment("关联运行ID（可空，系统级事件可不绑定）"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::EventType)
                            .string()
                            .string_len(48)
                            .not_null()
                            .comment("事件类型: user_message/assistant_message/tool_call/tool_result/planning/error/system"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::PayloadJson)
                            .text()
                            .not_null()
                            .comment("事件完整载荷（JSON，对应 AgentStreamEvent 序列化结果）"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::ExpiresAt)
                            .date_time()
                            .null()
                            .comment("过期时间（用于 TTL 清理，可空表示不过期）"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否已删除（软删除）"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间（事件发生时间）"),
                    )
                    .col(
                        ColumnDef::new(SessionEvent::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        // 常用查询：按 session 顺序回放（依靠自增 id 单调性）
        if_not_exists_create_index(
            manager,
            SessionEvent::Table,
            vec![SessionEvent::SessionId, SessionEvent::Id],
        )
        .await?;
        // 按 session + 时间扫描
        if_not_exists_create_index(
            manager,
            SessionEvent::Table,
            vec![SessionEvent::SessionId, SessionEvent::CreatedAt],
        )
        .await?;
        // 按 run_id 回溯
        if_not_exists_create_index(manager, SessionEvent::Table, vec![SessionEvent::RunId]).await?;
        // 按事件类型过滤（如只看工具调用）
        if_not_exists_create_index(manager, SessionEvent::Table, vec![SessionEvent::EventType])
            .await?;
        // TTL 清理扫描
        if_not_exists_create_index(manager, SessionEvent::Table, vec![SessionEvent::ExpiresAt])
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SessionEvent::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum SessionEvent {
    #[sea_orm(iden = "t_agent_session_event")]
    Table,
    Id,
    SessionId,
    AgentId,
    UserId,
    RunId,
    EventType,
    PayloadJson,
    ExpiresAt,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
