//! 智能体与技能绑定关系迁移（entity-first）
//! Entity: [`entity::agent::SkillRel`]
//!
//! 记录智能体启用了哪些技能。
//! 通过 `skill_package_id` 外键关联 `t_agent_skill_package`。

use entity::agent::{SkillRel, skill_rel};
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引/外键均由实体属性派生
        db.get_schema_builder().register(SkillRel).apply(db).await?;

        // 实体属性无法表达的联合索引（agent_id + skill_package_id），防止重复绑定
        if_not_exists_create_index(
            manager,
            SkillRel,
            vec![
                skill_rel::Column::AgentId,
                skill_rel::Column::SkillPackageId,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SkillRel).if_exists().to_owned())
            .await
    }
}
