//! 智能体（agent 私有）级上下文表迁移（entity-first）
//! Entity: [`entity::agent::GlobalState`]
//!
//! agent 私有上下文流，按写入顺序持久化 agent 经验 / 工作模式。

use entity::agent::{GlobalState, session_agent_state};
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
            .register(GlobalState)
            .apply(db)
            .await?;

        // 按 (agent, user) 顺序读取的联合索引
        if_not_exists_create_index(
            manager,
            GlobalState,
            vec![
                session_agent_state::Column::AgentId,
                session_agent_state::Column::UserId,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(GlobalState).if_exists().to_owned())
            .await
    }
}
