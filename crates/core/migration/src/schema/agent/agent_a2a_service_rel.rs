//! 智能体与 A2A 服务绑定关系迁移（entity-first）
//! Entity: [`entity::agent::A2aServiceRel`]
//!
//! 记录智能体绑定了哪些 A2A 服务注册表中的服务。

use entity::agent::A2aServiceRel;
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引均由实体属性派生
        db.get_schema_builder()
            .register(A2aServiceRel)
            .apply(db)
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(A2aServiceRel).if_exists().to_owned())
            .await
    }
}
