//! 智能体会话主表迁移
//! Entity: [`entity::agent::session::Model`]
//!
//! 会话（Session）是 Agent 运行时的一等公民：
//! - 唯一标识一次端到端的会话上下文
//! - 关联 agent_id / user_id 用于权限隔离与统计
//! - 通过 `last_event_at` 反映最近活跃时间，便于淘汰
//! - `expires_at` / `deleted` 用于 TTL 与软删除

use sea_orm::{
    DeriveIden, DeriveMigrationName,
    sea_query::{ColumnDef, Expr, Table},
};
use sea_orm_migration::{DbErr, MigrationTrait, SchemaManager, async_trait};

use crate::utils::{if_not_exists_create_index, if_not_exists_create_unique_index};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Session::Table)
                    .comment("智能体会话主表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Session::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("自增主键"),
                    )
                    .col(
                        ColumnDef::new(Session::SessionId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("会话ID（UUID，业务主键，全局唯一）"),
                    )
                    .col(
                        ColumnDef::new(Session::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("所属智能体ID"),
                    )
                    .col(
                        ColumnDef::new(Session::UserId)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("所属用户ID（支持多种身份系统）"),
                    )
                    .col(
                        ColumnDef::new(Session::Title)
                            .string()
                            .string_len(255)
                            .null()
                            .comment("会话标题（用户可改）"),
                    )
                    .col(
                        ColumnDef::new(Session::Status)
                            .small_integer()
                            .not_null()
                            .default(1)
                            .comment("状态: 0=未运行(Idle), 1=运行中(Running), 2=已取消(Cancelled), 3=已完成(Completed)"),
                    )
                    .col(
                        ColumnDef::new(Session::LastEventAt)
                            .date_time()
                            .null()
                            .comment("最近事件时间（用于活跃度排序与淘汰）"),
                    )
                    .col(
                        ColumnDef::new(Session::ExpiresAt)
                            .date_time()
                            .null()
                            .comment("过期时间（用于 TTL 清理，可空表示不过期）"),
                    )
                    .col(
                        ColumnDef::new(Session::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否已删除（软删除）"),
                    )
                    .col(
                        ColumnDef::new(Session::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(Session::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        // session_id 全局唯一
        if_not_exists_create_unique_index(manager, Session::Table, vec![Session::SessionId])
            .await?;
        // 常用查询：按 agent + user 列出会话
        if_not_exists_create_index(
            manager,
            Session::Table,
            vec![Session::AgentId, Session::UserId],
        )
        .await?;
        if_not_exists_create_index(manager, Session::Table, vec![Session::UserId]).await?;
        if_not_exists_create_index(manager, Session::Table, vec![Session::Status]).await?;
        if_not_exists_create_index(manager, Session::Table, vec![Session::LastEventAt]).await?;
        if_not_exists_create_index(manager, Session::Table, vec![Session::ExpiresAt]).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Session::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Session {
    #[sea_orm(iden = "t_agent_session")]
    Table,
    Id,
    SessionId,
    AgentId,
    UserId,
    Title,
    Status,
    LastEventAt,
    ExpiresAt,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
