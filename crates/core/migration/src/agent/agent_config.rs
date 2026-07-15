//! 智能体配置主表迁移
//! Entity: [`entity::agent::agent_config::Model`]
//!
//! 存储智能体的核心元数据，包括基础信息与 Prompt 配置。

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
                    .table(AgentConfig::Table)
                    .comment("智能体配置主表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AgentConfig::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("智能体ID"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::UserId)
                            .big_integer()
                            .not_null()
                            .comment("所属用户ID"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::Name)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("智能体名称"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::Description)
                            .text()
                            .not_null()
                            .comment("智能体描述"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::WelcomeMessage)
                            .text()
                            .null()
                            .comment("欢迎语（可选）"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::SystemPrompt)
                            .text()
                            .not_null()
                            .comment("系统 Prompt"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::Status)
                            .small_integer()
                            .not_null()
                            .default(0)
                            .comment("状态: 0=draft, 1=active, 2=disabled"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::TagsJson)
                            .text()
                            .not_null()
                            .default("[]")
                            .comment("标签列表（JSON 数组，默认为空数组）"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::MaxDelegationDepth)
                            .integer()
                            .not_null()
                            .default(3)
                            .comment("最大子智能体递归委派深度（默认 3）"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否已删除（软删除）"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(AgentConfig::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, AgentConfig::Table, vec![AgentConfig::UserId]).await?;
        if_not_exists_create_index(manager, AgentConfig::Table, vec![AgentConfig::Status]).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AgentConfig::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum AgentConfig {
    #[sea_orm(iden = "t_agent_config")]
    Table,
    Id,
    UserId,
    Name,
    Description,
    WelcomeMessage,
    SystemPrompt,
    Status,
    TagsJson,
    MaxDelegationDepth,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
