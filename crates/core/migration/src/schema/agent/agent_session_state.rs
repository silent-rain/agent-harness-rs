//! 智能体会话级上下文表迁移（entity-first）
//! Entity: [`entity::agent::SessionState`]
//!
//! 会话级上下文流：按写入顺序持久化对话上下文条目，
//! 与 `t_agent_user_state` / `t_agent_global_state` 共同构成三层上下文体系。

use entity::agent::{SessionState, session_state};
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引由实体属性派生
        db.get_schema_builder()
            .register(SessionState)
            .apply(db)
            .await?;

        // 按 agent + user 过滤的联合索引
        if_not_exists_create_index(
            manager,
            SessionState,
            vec![
                session_state::Column::AgentId,
                session_state::Column::UserId,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SessionState).if_exists().to_owned())
            .await
    }
}
