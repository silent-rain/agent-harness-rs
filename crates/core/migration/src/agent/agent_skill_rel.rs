//! 智能体与技能绑定关系迁移
//! Entity: [`entity::agent::skill_rel::Model`]
//!
//! 记录智能体启用了哪些技能。
//! 通过 `skill_package_id` 外键关联 `t_agent_skill_package`，
//! 与 `t_agent_command_rel` 的设计模式保持一致。

use sea_orm::{
    DeriveIden, DeriveMigrationName,
    sea_query::{ColumnDef, Expr, ForeignKey, ForeignKeyAction, Table},
};
use sea_orm_migration::{DbErr, MigrationTrait, SchemaManager, async_trait};

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SkillRel::Table)
                    .comment("智能体与技能绑定关系表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SkillRel::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("关系ID"),
                    )
                    .col(
                        ColumnDef::new(SkillRel::UserId)
                            .big_integer()
                            .not_null()
                            .comment("所属用户ID"),
                    )
                    .col(
                        ColumnDef::new(SkillRel::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("智能体ID"),
                    )
                    .col(
                        ColumnDef::new(SkillRel::SkillPackageId)
                            .big_integer()
                            .not_null()
                            .comment("关联的技能安装包ID（外键 → t_agent_skill_package.id）"),
                    )
                    .col(
                        ColumnDef::new(SkillRel::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(SkillRel::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(SkillRel::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_skill_rel_package_id")
                            .from(SkillRel::Table, SkillRel::SkillPackageId)
                            .to(SkillPackage::Table, SkillPackage::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, SkillRel::Table, vec![SkillRel::UserId]).await?;
        if_not_exists_create_index(manager, SkillRel::Table, vec![SkillRel::AgentId]).await?;
        if_not_exists_create_index(manager, SkillRel::Table, vec![SkillRel::SkillPackageId])
            .await?;
        // (agent_id, skill_package_id) 联合唯一索引，防止重复绑定
        if_not_exists_create_index(
            manager,
            SkillRel::Table,
            vec![SkillRel::AgentId, SkillRel::SkillPackageId],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SkillRel::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum SkillRel {
    #[sea_orm(iden = "t_agent_skill_rel")]
    Table,
    Id,
    UserId,
    AgentId,
    SkillPackageId,
    Enabled,
    CreatedAt,
    UpdatedAt,
}

/// 引用 t_agent_skill_package 表（用于外键声明）
#[derive(DeriveIden)]
pub enum SkillPackage {
    #[sea_orm(iden = "t_agent_skill_package")]
    Table,
    Id,
}
