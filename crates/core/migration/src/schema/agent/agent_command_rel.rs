//! 智能体与命令绑定关系迁移（entity-first）
//! Entity: [`entity::agent::CommandRel`]
//!
//! 记录智能体启用了哪些命令，以及排序优先级覆盖。

use entity::agent::CommandRel;
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
            .register(CommandRel)
            .apply(db)
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CommandRel).if_exists().to_owned())
            .await
    }
}
