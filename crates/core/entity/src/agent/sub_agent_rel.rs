//! 智能体子智能体绑定关系（dense 格式）
//!
//! 记录主智能体可以委派的子智能体列表，以及委派策略配置。

use chrono::Local;
use sea_orm::{
    FromQueryResult, Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体子智能体绑定关系
///
/// 对应数据库表 `t_agent_sub_agent_rel`。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(
    table_name = "t_agent_sub_agent_rel",
    comment = "智能体子智能体绑定关系表"
)]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key, auto_increment = true, comment = "关系ID")]
    pub id: i64,
    /// 主智能体ID
    #[sea_orm(comment = "主智能体ID", indexed)]
    pub agent_id: i64,
    /// 子智能体ID
    #[sea_orm(comment = "子智能体ID", indexed)]
    pub sub_agent_id: i64,
    /// 子智能体别名（在主智能体中的引用名）
    #[sea_orm(
        column_type = "String(StringLen::N(64))",
        comment = "子智能体别名（在主智能体中的引用名）"
    )]
    pub alias: Option<String>,
    /// 是否共享上下文记忆（同一轮主对话中保持子智能体的上下文）
    #[sea_orm(
        default_value = false,
        comment = "是否共享上下文记忆（同一轮主对话中保持子智能体的上下文）"
    )]
    pub share_context: bool,
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
