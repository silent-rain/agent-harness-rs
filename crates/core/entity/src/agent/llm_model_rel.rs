//! 智能体 LLM 模型关联表

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体 LLM 模型关联表
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_llm_model_rel")]
pub struct Model {
    /// 绑定ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 智能体ID
    pub agent_id: i64,
    /// 模型ID
    pub model_id: i64,
    /// 温度参数
    pub temperature: Option<f64>,
    /// 最大输出 token 数
    pub max_tokens: Option<i32>,
    /// 是否启用
    pub enabled: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::agent_config::Entity",
        from = "Column::AgentId",
        to = "super::agent_config::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Agent,
    #[sea_orm(
        belongs_to = "super::llm_model::Entity",
        from = "Column::ModelId",
        to = "super::llm_model::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    LlmModel,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 在插入/更新前自动更新时间戳
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = Local::now().naive_local();
        if insert {
            self.created_at = Set(now);
        }
        self.updated_at = Set(now);
        Ok(self)
    }
}
