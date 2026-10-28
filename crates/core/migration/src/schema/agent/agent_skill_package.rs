//! 智能体技能安装包迁移（entity-first）
//! Entity: [`entity::agent::SkillPackage`]
//!
//! 以 zip 二进制形式存储 Skill 物理包，配合 `t_agent_skill_rel` 实现
//! "包存储 + 绑定关系"解耦。

use entity::agent::{SkillPackage, skill_package};
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引由实体属性派生
        db.get_schema_builder()
            .register(SkillPackage)
            .apply(db)
            .await?;

        // (user_id, skill_code) 联合索引
        if_not_exists_create_index(
            manager,
            SkillPackage,
            vec![
                skill_package::Column::UserId,
                skill_package::Column::SkillCode,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SkillPackage).if_exists().to_owned())
            .await
    }
}
