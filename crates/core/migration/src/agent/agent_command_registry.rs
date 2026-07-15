//! 智能体命令注册表迁移
//! Entity: [`entity::agent::command_registry::Model`]
//!
//! 存储平台内建命令与用户自定义命令。
//! 命令（Command）是一种面向开发场景的指令式交互能力，
//! 用于将常见、高频、结构化的开发任务封装为可复用的命令。

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
                    .table(CommandRegistry::Table)
                    .comment("智能体命令注册表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CommandRegistry::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("命令ID"),
                    )
                    .col(
                        ColumnDef::new(CommandRegistry::UserId)
                            .big_integer()
                            .not_null()
                            .comment("所属用户ID"),
                    )
                    .col(
                        ColumnDef::new(CommandRegistry::Name)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("命令名称（同一用户下唯一，排除已删除记录）"),
                    )
                    .col(
                        ColumnDef::new(CommandRegistry::Description)
                            .text()
                            .not_null()
                            .comment("命令描述"),
                    )
                    .col(
                        ColumnDef::new(CommandRegistry::Content)
                            .text()
                            .not_null()
                            .comment("命令提示词正文（完整的 Markdown 内容）"),
                    )
                    .col(
                        ColumnDef::new(CommandRegistry::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(CommandRegistry::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否删除"),
                    )
                    .col(
                        ColumnDef::new(CommandRegistry::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(CommandRegistry::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(
            manager,
            CommandRegistry::Table,
            vec![CommandRegistry::UserId],
        )
        .await?;
        // name 在同一用户下唯一（排除已删除记录），通过联合唯一索引约束
        if_not_exists_create_unique_index(
            manager,
            CommandRegistry::Table,
            vec![
                CommandRegistry::UserId,
                CommandRegistry::Name,
                CommandRegistry::Deleted,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CommandRegistry::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum CommandRegistry {
    #[sea_orm(iden = "t_agent_command_registry")]
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
