//! MCP 服务注册表迁移（entity-first）
//! Entity: [`entity::agent::McpServiceRegistry`]
//!
//! 存储平台内建 MCP 服务与用户自定义 MCP 服务，支持在线市场分发。

use entity::agent::{McpServiceRegistry, mcp_service_registry};
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

use crate::utils::if_not_exists_create_unique_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引由实体属性派生
        db.get_schema_builder()
            .register(McpServiceRegistry)
            .apply(db)
            .await?;

        // name 在同一用户下唯一（排除已删除记录），通过联合唯一索引约束
        if_not_exists_create_unique_index(
            manager,
            McpServiceRegistry,
            vec![
                mcp_service_registry::Column::UserId,
                mcp_service_registry::Column::Name,
                mcp_service_registry::Column::Deleted,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(McpServiceRegistry)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
