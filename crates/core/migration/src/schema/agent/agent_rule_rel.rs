//! 智能体与规则绑定关系迁移（entity-first）
//! Entity: [`entity::agent::RuleRel`]
//!
//! 记录智能体启用了哪些规则，以及注入顺序覆盖。

use entity::agent::RuleRel;
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引由实体属性派生
        db.get_schema_builder().register(RuleRel).apply(db).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RuleRel).if_exists().to_owned())
            .await
    }
}
