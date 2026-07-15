//! 智能体与规则绑定关系实体

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, FromQueryResult, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体与规则绑定关系
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_rule_rel")]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 智能体ID
    pub agent_id: i64,
    /// 规则ID
    pub rule_id: i64,
    /// 注入优先级覆盖
    pub priority: i32,
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

/// 规则绑定关联查询结果（JOIN rule_registry）
///
/// 用于 `rule_rel` 与 `rule_registry` 关联查询，
/// 一次 SQL 获取绑定关系及其关联规则的名称、描述和启用状态。
/// 可直接作为 API 响应体序列化输出。
#[derive(Clone, Debug, Serialize, Deserialize, FromQueryResult)]
pub struct RuleRelWithRule {
    /// 绑定关系基础字段
    #[sea_orm(nested)]
    #[serde(flatten)]
    pub rel: Model,
    /// 规则名称
    pub rule_name: String,
    /// 规则描述
    pub rule_description: String,
    /// 规则自身是否启用
    pub rule_enabled: bool,
}
