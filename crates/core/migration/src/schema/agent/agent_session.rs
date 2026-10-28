//! 智能体会话主表迁移（entity-first）
//! Entity: [`entity::agent::Session`]
//!
//! 会话（Session）是 Agent 运行时的一等公民：
//! - 唯一标识一次端到端的会话上下文
//! - 关联 agent_id / user_id 用于权限隔离与统计
//! - 通过 `last_event_at` 反映最近活跃时间，便于淘汰
//! - `expires_at` / `deleted` 用于 TTL 与软删除

use entity::agent::{Session, session};
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/唯一约束/索引由实体属性派生
        db.get_schema_builder().register(Session).apply(db).await?;

        // 常用查询：按 agent + user 列出会话
        if_not_exists_create_index(
            manager,
            Session,
            vec![session::Column::AgentId, session::Column::UserId],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Session).if_exists().to_owned())
            .await
    }
}
