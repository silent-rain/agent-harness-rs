//! 智能体与规则绑定关系（dense 格式）

use chrono::Local;
use sea_orm::{
    FromQueryResult, Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体与规则绑定关系
///
/// 对应数据库表 `t_agent_rule_rel`。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_rule_rel", comment = "智能体与规则绑定关系表")]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key, auto_increment = true, comment = "关系ID")]
    pub id: i64,
    /// 智能体ID
    #[sea_orm(comment = "智能体ID", indexed)]
    pub agent_id: i64,
    /// 规则ID
    #[sea_orm(comment = "规则ID", indexed)]
    pub rule_id: i64,
    /// 注入优先级覆盖（覆盖规则默认优先级）
    #[sea_orm(default_value = 0, comment = "注入优先级覆盖（覆盖规则默认优先级）")]
    pub priority: i32,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 创建时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "创建时间")]
    pub created_at: DateTime,
    /// 更新时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "更新时间")]
    pub updated_at: DateTime,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 在插入/更新前自动更新时间戳
    ///
    /// 仅 `ActiveModel::insert()` / `save()` / `update()` 触发；
    /// 批量 `Entity::insert_many()` 不经过本钩子，需先调 [`ActiveModel::ensure_timestamps`]。
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

impl ActiveModel {
    /// 显式补齐 `created_at` / `updated_at`（`insert_many` 不触发 `before_save`）。
    pub fn ensure_timestamps(&mut self) {
        let now = Local::now().naive_local();
        self.created_at = Set(now);
        self.updated_at = Set(now);
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
