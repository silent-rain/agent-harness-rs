//! 智能体会话注入消息队列表迁移
//! Entity: [`entity::agent::session_pending_input::Model`]
//!
//! 独立的"待消费注入队列"表，与 `t_agent_session_state`（已固化对话历史）职责分离：
//! - 接口层直接写入本表
//! - `drive_react_loop` 每轮开头 drain 未消费记录，消费后标记 `consumed_at`
//! - 消费时同步将消息固化到 `t_agent_session_state`，实现"消费即固化"语义
//!
//! 设计优势：
//! - 队列与历史职责清晰，无语义混淆
//! - 进程重启后未消费的消息不会丢失
//! - 通过 `consumed_at IS NULL` 精确识别待处理消息，无 race condition
//! - 可查询队列深度用于 UI 展示

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
                    .table(SessionPendingInput::Table)
                    .comment("智能体会话注入消息队列（FIFO + 消费标记）")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SessionPendingInput::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("自增主键（队列序号）"),
                    )
                    .col(
                        ColumnDef::new(SessionPendingInput::SessionId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属会话ID"),
                    )
                    .col(
                        ColumnDef::new(SessionPendingInput::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("所属智能体ID（冗余）"),
                    )
                    .col(
                        ColumnDef::new(SessionPendingInput::UserId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属用户ID（冗余；支持多种身份系统）"),
                    )
                    .col(
                        ColumnDef::new(SessionPendingInput::ContentJson)
                            .text()
                            .not_null()
                            .comment("完整业务 JSON（ConversationMessage 序列化）"),
                    )
                    .col(
                        ColumnDef::new(SessionPendingInput::ConsumedAt)
                            .date_time()
                            .null()
                            .comment("被 ReAct 循环消费的时间（NULL 表示未消费）"),
                    )
                    .col(
                        ColumnDef::new(SessionPendingInput::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .to_owned(),
            )
            .await?;

        // 核心查询索引：按 session 查找未消费记录
        if_not_exists_create_index(
            manager,
            SessionPendingInput::Table,
            vec![
                SessionPendingInput::SessionId,
                SessionPendingInput::ConsumedAt,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SessionPendingInput::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum SessionPendingInput {
    #[sea_orm(iden = "t_agent_session_pending_input")]
    Table,
    Id,
    SessionId,
    AgentId,
    UserId,
    ContentJson,
    ConsumedAt,
    CreatedAt,
}
