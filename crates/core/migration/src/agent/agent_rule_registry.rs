//! 智能体规则注册表迁移
//! Entity: [`entity::agent::rule_registry::Model`]
//!
//! 存储平台内建规则与用户自定义规则，规则以文本形式注入 System Prompt。

use sea_orm::{
    DeriveIden, DeriveMigrationName,
    sea_query::{ColumnDef, Expr, Table},
};
use sea_orm_migration::{DbErr, MigrationTrait, SchemaManager, async_trait};

use crate::utils::{if_not_exists_create_index, if_not_exists_create_unique_index};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RuleRegistry::Table)
                    .comment("智能体规则注册表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RuleRegistry::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("规则ID"),
                    )
                    .col(
                        ColumnDef::new(RuleRegistry::UserId)
                            .big_integer()
                            .not_null()
                            .comment("所属用户ID"),
                    )
                    .col(
                        ColumnDef::new(RuleRegistry::Name)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("规则名称（同一用户下唯一，排除已删除记录）"),
                    )
                    .col(
                        ColumnDef::new(RuleRegistry::Description)
                            .text()
                            .not_null()
                            .comment("规则描述"),
                    )
                    .col(
                        ColumnDef::new(RuleRegistry::Content)
                            .text()
                            .not_null()
                            .comment("规则内容（注入 System Prompt 的文本）"),
                    )
                    .col(
                        ColumnDef::new(RuleRegistry::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(RuleRegistry::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否删除"),
                    )
                    .col(
                        ColumnDef::new(RuleRegistry::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(RuleRegistry::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, RuleRegistry::Table, vec![RuleRegistry::UserId])
            .await?;
        // name 在同一用户下唯一（排除已删除记录），通过联合唯一索引约束
        if_not_exists_create_unique_index(
            manager,
            RuleRegistry::Table,
            vec![
                RuleRegistry::UserId,
                RuleRegistry::Name,
                RuleRegistry::Deleted,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RuleRegistry::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum RuleRegistry {
    #[sea_orm(iden = "t_agent_rule_registry")]
    Table,
    Id,
    UserId,
    Name,
    Description,
    Content,
    Enabled,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
