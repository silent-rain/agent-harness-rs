//! 用户自定义 LLM 模型配置表迁移
//! Entity: [`entity::agent::llm_model::Model`]
//!
//! 保存用户配置的 LLM 模型元数据，包括供应商、模型编码、接入参数等。

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
                    .table(LlmModel::Table)
                    .comment("LLM 模型配置表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LlmModel::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("模型ID"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::UserId)
                            .big_integer()
                            .not_null()
                            .comment("所属用户ID"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::Provider)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("供应商，如 openai / qwen / glm"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::ModelCode)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("模型编码，如 gpt-4o / qwen-plus"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::ModelName)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("模型显示名称"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::BaseUrl)
                            .string()
                            .string_len(512)
                            .not_null()
                            .comment("API 基础地址"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::ApiKey)
                            .string()
                            .string_len(256)
                            .not_null()
                            .comment("API Key"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否删除"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(LlmModel::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, LlmModel::Table, vec![LlmModel::UserId]).await?;
        if_not_exists_create_index(manager, LlmModel::Table, vec![LlmModel::Provider]).await?;
        if_not_exists_create_index(manager, LlmModel::Table, vec![LlmModel::ModelCode]).await?;
        if_not_exists_create_index(manager, LlmModel::Table, vec![LlmModel::Enabled]).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LlmModel::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum LlmModel {
    #[sea_orm(iden = "t_agent_llm_model")]
    Table,
    Id,
    UserId,
    Provider,
    ModelCode,
    ModelName,
    BaseUrl,
    ApiKey,
    Enabled,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
