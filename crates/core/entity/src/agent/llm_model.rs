//! LLM 模型定义表

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// LLM 模型定义表
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_llm_model")]
pub struct Model {
    /// 模型ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID
    pub user_id: i64,
    /// 供应商，如 openai / qwen / glm
    pub provider: String,
    /// 模型编码，如 gpt-4o / qwen-plus
    pub model_code: String,
    /// 模型显示名称
    pub model_name: String,
    /// API 基础地址
    pub base_url: String,
    /// API Key
    pub api_key: String,
    /// 是否启用
    pub enabled: bool,
    /// 是否已删除
    pub deleted: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

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
