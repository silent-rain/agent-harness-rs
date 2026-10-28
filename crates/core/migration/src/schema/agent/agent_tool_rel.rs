//! 智能体工具绑定表迁移（entity-first）
//! Entity: [`entity::agent::ToolRel`]
//!
//! 绑定智能体与可用工具，支持自定义配置和并发限制。

use entity::agent::{ToolRel, tool_rel};
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引/外键均由实体属性派生
        db.get_schema_builder().register(ToolRel).apply(db).await?;

        // 实体属性无法表达的联合索引（agent_id + tool_id）
        if_not_exists_create_index(
            manager,
            ToolRel,
            vec![tool_rel::Column::AgentId, tool_rel::Column::ToolId],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ToolRel).if_exists().to_owned())
            .await
    }
}
