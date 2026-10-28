//! 智能体规则注册表迁移（entity-first）
//! Entity: [`entity::agent::RuleRegistry`]
//!
//! 存储平台内建规则与用户自定义规则，规则以文本形式注入 System Prompt。

use entity::agent::{RuleRegistry, rule_registry};
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
            .register(RuleRegistry)
            .apply(db)
            .await?;

        // name 在同一用户下唯一（排除已删除记录），通过联合唯一索引约束
        if_not_exists_create_unique_index(
            manager,
            RuleRegistry,
            vec![
                rule_registry::Column::UserId,
                rule_registry::Column::Name,
                rule_registry::Column::Deleted,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RuleRegistry).if_exists().to_owned())
            .await
    }
}
