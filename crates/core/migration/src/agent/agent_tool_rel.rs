//! 智能体工具绑定表迁移
//! Entity: [`entity::agent::tool_rel::Model`]
//!
//! 绑定智能体与可用工具，支持自定义配置和并发限制。

use sea_orm::{
    DeriveIden, DeriveMigrationName,
    sea_query::{ColumnDef, Expr, ForeignKey, ForeignKeyAction, Table},
};
use sea_orm_migration::{DbErr, MigrationTrait, SchemaManager, async_trait};

use crate::utils::if_not_exists_create_index;

use super::{agent_config::AgentConfig, agent_tool_registry::ToolRegistry};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ToolRel::Table)
                    .comment("智能体工具绑定表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ToolRel::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("绑定ID"),
                    )
                    .col(
                        ColumnDef::new(ToolRel::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("智能体ID"),
                    )
                    .col(
                        ColumnDef::new(ToolRel::ToolId)
                            .big_integer()
                            .not_null()
                            .comment("工具ID"),
                    )
                    .col(
                        ColumnDef::new(ToolRel::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(ToolRel::Priority)
                            .integer()
                            .not_null()
                            .default(0)
                            .comment("优先级（数值越小越优先）"),
                    )
                    .col(
                        ColumnDef::new(ToolRel::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(ToolRel::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_agent_tool_rel_agent")
                            .from(ToolRel::Table, ToolRel::AgentId)
                            .to(AgentConfig::Table, AgentConfig::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_agent_tool_rel_tool")
                            .from(ToolRel::Table, ToolRel::ToolId)
                            .to(ToolRegistry::Table, ToolRegistry::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, ToolRel::Table, vec![ToolRel::AgentId]).await?;
        if_not_exists_create_index(
            manager,
            ToolRel::Table,
            vec![ToolRel::AgentId, ToolRel::ToolId],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ToolRel::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum ToolRel {
    #[sea_orm(iden = "t_agent_tool_rel")]
    Table,
    Id,
    AgentId,
    ToolId,
    Enabled,
    Priority,
    CreatedAt,
    UpdatedAt,
}
