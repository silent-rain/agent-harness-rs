//! 用户自定义 LLM 模型配置表迁移（entity-first）
//! Entity: [`entity::agent::LlmModel`]
//!
//! 保存用户配置的 LLM 模型元数据，包括供应商、模型编码、接入参数等。

use entity::agent::LlmModel;
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引/外键均由实体属性派生
        db.get_schema_builder().register(LlmModel).apply(db).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LlmModel).if_exists().to_owned())
            .await
    }
}
