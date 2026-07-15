//! 智能体子智能体绑定关系迁移
//! Entity: [`entity::agent::sub_agent_rel::Model`]
//!
//! 记录主智能体可以委派的子智能体列表，以及委派策略配置。

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
                    .table(SubAgentRel::Table)
                    .comment("智能体子智能体绑定关系表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SubAgentRel::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("关系ID"),
                    )
                    .col(
                        ColumnDef::new(SubAgentRel::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("主智能体ID"),
                    )
                    .col(
                        ColumnDef::new(SubAgentRel::SubAgentId)
                            .big_integer()
                            .not_null()
                            .comment("子智能体ID"),
                    )
                    .col(
                        ColumnDef::new(SubAgentRel::Alias)
                            .string()
                            .string_len(64)
                            .null()
                            .comment("子智能体别名（在主智能体中的引用名）"),
                    )
                    .col(
                        ColumnDef::new(SubAgentRel::ShareContext)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否共享上下文记忆（同一轮主对话中保持子智能体的上下文）"),
                    )
                    .col(
                        ColumnDef::new(SubAgentRel::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(SubAgentRel::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(SubAgentRel::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, SubAgentRel::Table, vec![SubAgentRel::AgentId]).await?;
        if_not_exists_create_index(manager, SubAgentRel::Table, vec![SubAgentRel::SubAgentId])
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SubAgentRel::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum SubAgentRel {
    #[sea_orm(iden = "t_agent_sub_agent_rel")]
    Table,
    Id,
    AgentId,
    SubAgentId,
    Alias,
    ShareContext,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
