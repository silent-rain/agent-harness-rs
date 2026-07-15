//! MCP 服务注册表迁移
//! Entity: [`entity::agent::mcp_service_registry::Model`]
//!
//! 存储平台内建 MCP 服务与用户自定义 MCP 服务，支持在线市场分发。

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
                    .table(McpServiceRegistry::Table)
                    .comment("MCP 服务注册表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(McpServiceRegistry::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("服务ID"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::UserId)
                            .big_integer()
                            .not_null()
                            .comment("所属用户ID（0 表示平台内建）"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::Name)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("服务名称（同一用户下唯一，排除已删除记录）"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::Description)
                            .text()
                            .not_null()
                            .comment("服务描述"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::McpServiceUrl)
                            .text()
                            .null()
                            .comment("MCP 服务地址（stdio 类型为空）"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::TransportType)
                            .string()
                            .string_len(32)
                            .not_null()
                            .default("stdio")
                            .comment("传输类型: stdio, sse, streamable-http"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::EnabledTools)
                            .text()
                            .not_null()
                            .default("[]")
                            .comment("允许使用的工具列表（JSON 数组，空=全部可用）"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::ConfigJson)
                            .text()
                            .null()
                            .comment("MCP 服务配置（JSON，mcpServers 包裹格式）"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否删除"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(McpServiceRegistry::UpdatedAt)
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
            McpServiceRegistry::Table,
            vec![McpServiceRegistry::UserId],
        )
        .await?;

        // name 在同一用户下唯一（排除已删除记录），通过联合唯一索引约束
        if_not_exists_create_unique_index(
            manager,
            McpServiceRegistry::Table,
            vec![
                McpServiceRegistry::UserId,
                McpServiceRegistry::Name,
                McpServiceRegistry::Deleted,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(McpServiceRegistry::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum McpServiceRegistry {
    #[sea_orm(iden = "t_agent_mcp_service_registry")]
    Table,
    Id,
    UserId,
    Name,
    Description,
    McpServiceUrl,
    TransportType,
    EnabledTools,
    ConfigJson,
    Enabled,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
