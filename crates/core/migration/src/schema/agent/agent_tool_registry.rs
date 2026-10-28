//! 工具注册表建表（entity-first）
//!
//! Entity: [`entity::agent::ToolRegistry`]
//!
//! 平台内建工具与受控工具定义，包含函数 Schema。预置工具数据见
//! [`crate::seed::s_agent_tool_registry`]。

use entity::agent::ToolRegistry;
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引由实体属性派生
        db.get_schema_builder().register(ToolRegistry).apply(db).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ToolRegistry).if_exists().to_owned())
            .await
    }
}
