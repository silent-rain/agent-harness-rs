//! 智能体与规则绑定关系迁移
//! Entity: [`entity::agent::rule_rel::Model`]
//!
//! 记录智能体启用了哪些规则，以及注入顺序覆盖。

use sea_orm::{
    DeriveIden, DeriveMigrationName,
    sea_query::{ColumnDef, Expr, Table},
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
                    .table(RuleRel::Table)
                    .comment("智能体与规则绑定关系表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RuleRel::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("关系ID"),
                    )
                    .col(
                        ColumnDef::new(RuleRel::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("智能体ID"),
                    )
                    .col(
                        ColumnDef::new(RuleRel::RuleId)
                            .big_integer()
                            .not_null()
                            .comment("规则ID"),
                    )
                    .col(
                        ColumnDef::new(RuleRel::Priority)
                            .integer()
                            .not_null()
                            .default(0)
                            .comment("注入优先级覆盖（覆盖规则默认优先级）"),
                    )
                    .col(
                        ColumnDef::new(RuleRel::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(RuleRel::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(RuleRel::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, RuleRel::Table, vec![RuleRel::AgentId]).await?;
        if_not_exists_create_index(manager, RuleRel::Table, vec![RuleRel::RuleId]).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RuleRel::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum RuleRel {
    #[sea_orm(iden = "t_agent_rule_rel")]
    Table,
    Id,
    AgentId,
    RuleId,
    Priority,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
