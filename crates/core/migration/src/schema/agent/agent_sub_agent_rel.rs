//! 智能体子智能体绑定关系迁移（entity-first）
//! Entity: [`entity::agent::SubAgentRel`]
//!
//! 记录主智能体可以委派的子智能体列表，以及委派策略配置。

use entity::agent::SubAgentRel;
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引由实体属性派生
        db.get_schema_builder()
            .register(SubAgentRel)
            .apply(db)
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SubAgentRel).if_exists().to_owned())
            .await
    }
}
