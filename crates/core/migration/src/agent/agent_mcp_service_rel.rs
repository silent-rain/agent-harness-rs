//! 智能体与 MCP 服务绑定关系迁移
//! Entity: [`entity::agent::mcp_service_rel::Model`]
//!
//! 记录智能体绑定了哪些 MCP 服务注册表中的服务，以及工具白名单覆盖。

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
                    .table(McpServiceRel::Table)
                    .comment("智能体与 MCP 服务绑定关系表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(McpServiceRel::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("关系ID"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRel::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("智能体ID"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRel::RegistryId)
                            .big_integer()
                            .not_null()
                            .comment("MCP 服务注册表ID"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRel::EnabledTools)
                            .text()
                            .not_null()
                            .default("[]")
                            .comment("允许使用的工具列表覆盖（JSON 数组，空=使用注册表默认值）"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRel::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRel::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRel::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, McpServiceRel::Table, vec![McpServiceRel::AgentId])
            .await?;
        if_not_exists_create_index(
            manager,
            McpServiceRel::Table,
            vec![McpServiceRel::RegistryId],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(McpServiceRel::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum McpServiceRel {
    #[sea_orm(iden = "t_agent_mcp_service_rel")]
    Table,
    Id,
    AgentId,
    RegistryId,
    EnabledTools,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
