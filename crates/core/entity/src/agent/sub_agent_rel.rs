//! 智能体子智能体绑定关系实体

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, FromQueryResult, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体子智能体绑定关系
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_sub_agent_rel")]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 主智能体ID
    pub agent_id: i64,
    /// 子智能体ID
    pub sub_agent_id: i64,
    /// 子智能体别名（在主智能体中的引用名）
    pub alias: Option<String>,
    /// 是否共享上下文记忆（同一轮主对话中保持子智能体的上下文）
    pub share_context: bool,
    /// 是否启用
    pub enabled: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
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

/// 子智能体绑定关联查询结果（JOIN agent_config）
///
/// 用于 `sub_agent_rel` 与 `agent_config` 关联查询，
/// 一次 SQL 获取绑定关系及其关联子智能体的名称、描述和状态。
/// 可直接作为 API 响应体序列化输出。
#[derive(Clone, Debug, Serialize, Deserialize, FromQueryResult)]
pub struct SubAgentRelWithAgent {
    /// 绑定关系基础字段
    #[sea_orm(nested)]
    #[serde(flatten)]
    pub rel: Model,
    /// 子智能体名称
    pub agent_name: String,
    /// 子智能体描述
    pub agent_description: String,
    /// 子智能体状态
    pub agent_status: i16,
}
