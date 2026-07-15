//! 智能体与 A2A 服务绑定关系迁移
//! Entity: [`entity::agent::a2a_service_rel::Model`]
//!
//! 记录智能体绑定了哪些 A2A 服务注册表中的服务。

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
                    .table(A2aServiceRel::Table)
                    .comment("智能体与 A2A 服务绑定关系表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(A2aServiceRel::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("关系ID"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRel::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("智能体ID"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRel::RegistryId)
                            .big_integer()
                            .not_null()
                            .comment("A2A 服务注册表ID"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRel::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRel::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(A2aServiceRel::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, A2aServiceRel::Table, vec![A2aServiceRel::AgentId])
            .await?;
        if_not_exists_create_index(
            manager,
            A2aServiceRel::Table,
            vec![A2aServiceRel::RegistryId],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(A2aServiceRel::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum A2aServiceRel {
    #[sea_orm(iden = "t_agent_a2a_service_rel")]
    Table,
    Id,
    AgentId,
    RegistryId,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
