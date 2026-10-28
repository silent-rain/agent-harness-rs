//! 智能体 LLM 模型关联表迁移（entity-first）
//! Entity: [`entity::agent::LlmModelRel`]
//!
//! 绑定智能体与 LLM 模型，并配置推理参数。

use entity::agent::LlmModelRel;
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
            .register(LlmModelRel)
            .apply(db)
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LlmModelRel).if_exists().to_owned())
            .await
    }
}
