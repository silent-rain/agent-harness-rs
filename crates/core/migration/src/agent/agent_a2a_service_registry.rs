//! A2A 服务注册表迁移
//! Entity: [`entity::agent::a2a_service_registry::Model`]
//!
//! 存储平台内建 A2A 远程 Agent 服务与用户自定义 A2A 服务，支持在线市场分发。

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
                    .table(A2aServiceRegistry::Table)
                    .comment("A2A 服务注册表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(A2aServiceRegistry::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("服务ID"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::UserId)
                            .big_integer()
                            .not_null()
                            .comment("所属用户ID（0 表示平台内建）"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::Name)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("服务名称（同一用户下唯一，LLM 可见，排除已删除记录）"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::Description)
                            .text()
                            .not_null()
                            .comment("服务描述（LLM 可见，覆盖 AgentCard 中的 description）"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::BaseUrl)
                            .string()
                            .string_len(512)
                            .not_null()
                            .comment("远程 A2A 服务基础 URL（如 http://remote-agent:3000）"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::TimeoutSecs)
                            .integer()
                            .null()
                            .comment("连接超时（秒），NULL 使用默认值 60"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::ConfigJson)
                            .text()
                            .null()
                            .comment("A2A 服务配置（JSON，a2aServers 包裹格式）"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否删除"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRegistry::UpdatedAt)
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
            A2aServiceRegistry::Table,
            vec![A2aServiceRegistry::UserId],
        )
        .await?;

        // name 在同一用户下唯一（排除已删除记录），通过联合唯一索引约束
        if_not_exists_create_unique_index(
            manager,
            A2aServiceRegistry::Table,
            vec![
                A2aServiceRegistry::UserId,
                A2aServiceRegistry::Name,
                A2aServiceRegistry::Deleted,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(A2aServiceRegistry::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum A2aServiceRegistry {
    #[sea_orm(iden = "t_agent_a2a_service_registry")]
    Table,
    Id,
    UserId,
    Name,
    Description,
    BaseUrl,
    TimeoutSecs,
    ConfigJson,
    Enabled,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
