//! 智能体 LLM 模型关联表迁移
//! Entity: [`entity::agent::llm_model_rel::Model`]
//!
//! 绑定智能体与 LLM 模型，并配置推理参数与 Planner 协议。

use sea_orm::{
    DeriveIden, DeriveMigrationName,
    sea_query::{ColumnDef, Expr, ForeignKey, ForeignKeyAction, Table},
};
use sea_orm_migration::{DbErr, MigrationTrait, SchemaManager, async_trait};

use crate::utils::if_not_exists_create_index;

use super::{agent_config::AgentConfig, agent_llm_model::LlmModel};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ModelBinding::Table)
                    .comment("智能体 LLM 模型关联表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ModelBinding::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("绑定ID"),
                    )
                    .col(
                        ColumnDef::new(ModelBinding::AgentId)
                            .big_integer()
                            .not_null()
                            .comment("智能体ID"),
                    )
                    .col(
                        ColumnDef::new(ModelBinding::ModelId)
                            .big_integer()
                            .not_null()
                            .comment("模型ID"),
                    )
                    .col(
                        ColumnDef::new(ModelBinding::Temperature)
                            .double()
                            .null()
                            .comment("温度参数"),
                    )
                    .col(
                        ColumnDef::new(ModelBinding::MaxTokens)
                            .integer()
                            .null()
                            .comment("最大输出 token 数"),
                    )
                    .col(
                        ColumnDef::new(ModelBinding::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(ModelBinding::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(ModelBinding::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_agent_llm_model_rel_agent")
                            .from(ModelBinding::Table, ModelBinding::AgentId)
                            .to(AgentConfig::Table, AgentConfig::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_agent_llm_model_rel_model")
                            .from(ModelBinding::Table, ModelBinding::ModelId)
                            .to(LlmModel::Table, LlmModel::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, ModelBinding::Table, vec![ModelBinding::AgentId])
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ModelBinding::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum ModelBinding {
    #[sea_orm(iden = "t_agent_llm_model_rel")]
    Table,
    Id,
    AgentId,
    ModelId,
    Temperature,
    MaxTokens,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
