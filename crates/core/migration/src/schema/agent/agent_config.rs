//! 智能体配置主表迁移（entity-first）
//! Entity: [`entity::agent::AgentConfig`]
//!
//! 存储智能体的核心元数据，包括基础信息与 Prompt 配置。

use entity::agent::AgentConfig;
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
            .register(AgentConfig)
            .apply(db)
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AgentConfig).if_exists().to_owned())
            .await
    }
}
