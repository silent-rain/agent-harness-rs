//! 智能体用户级上下文表迁移（entity-first）
//! Entity: [`entity::agent::UserState`]
//!
//! 用户级上下文流（跨 agent），按写入顺序持久化用户偏好 / 画像 / 事实。

use entity::agent::UserState;
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
            .register(UserState)
            .apply(db)
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserState).if_exists().to_owned())
            .await
    }
}
