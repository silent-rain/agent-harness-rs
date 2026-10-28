//! 智能体与 MCP 服务绑定关系迁移（entity-first）
//! Entity: [`entity::agent::McpServiceRel`]
//!
//! 记录智能体绑定了哪些 MCP 服务注册表中的服务，以及工具白名单覆盖。

use entity::agent::McpServiceRel;
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引/外键均由实体属性派生
        db.get_schema_builder()
            .register(McpServiceRel)
            .apply(db)
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(McpServiceRel).if_exists().to_owned())
            .await
    }
}
