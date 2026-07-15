//! 智能体与命令绑定关系迁移
//! Entity: [`entity::agent::command_rel::Model`]
//!
//! 记录智能体启用了哪些命令，以及排序优先级覆盖。

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
                    .table(CommandRel::Table)
                    .comment("智能体与命令绑定关系表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CommandRel::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("关系ID"),
                    )
                    .col(
                        ColumnDef::new(CommandRel::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("智能体ID"),
                    )
                    .col(
                        ColumnDef::new(CommandRel::CommandId)
                            .big_integer()
                            .not_null()
                            .comment("命令ID"),
                    )
                    .col(
                        ColumnDef::new(CommandRel::Priority)
                            .integer()
                            .not_null()
                            .default(0)
                            .comment("排序优先级（数值越小越靠前）"),
                    )
                    .col(
                        ColumnDef::new(CommandRel::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(CommandRel::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(CommandRel::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, CommandRel::Table, vec![CommandRel::AgentId]).await?;
        if_not_exists_create_index(manager, CommandRel::Table, vec![CommandRel::CommandId]).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CommandRel::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum CommandRel {
    #[sea_orm(iden = "t_agent_command_rel")]
    Table,
    Id,
    AgentId,
    CommandId,
    Priority,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
