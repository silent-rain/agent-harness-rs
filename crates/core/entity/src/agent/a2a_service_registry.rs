//! A2A 服务注册表实体（dense 格式）
//!
//! 存储平台内建 A2A 远程 Agent 服务与用户自定义 A2A 服务，
//! 支持在线市场分发与用户私有服务的统一管理。

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// A2A 服务注册表
///
/// 记录可供智能体绑定的 A2A 远程 Agent 服务元数据与连接配置。
/// 通过 `t_agent_a2a_service_rel` 与智能体建立多对多关联。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(
    table_name = "t_agent_a2a_service_registry",
    comment = "A2A 服务注册表"
)]
pub struct Model {
    /// 服务ID
    #[sea_orm(primary_key, auto_increment = true, comment = "服务ID")]
    pub id: i64,
    /// 所属用户ID（0 表示平台内建）
    #[sea_orm(indexed, comment = "所属用户ID（0 表示平台内建）")]
    pub user_id: i64,
    /// 服务名称（同一用户下唯一，LLM 可见，如 "weather_agent"）
    #[sea_orm(
        column_type = "String(StringLen::N(128))",
        comment = "服务名称（同一用户下唯一，LLM 可见，排除已删除记录）"
    )]
    pub name: String,
    /// 服务描述（LLM 可见，覆盖 AgentCard 中的 description）
    #[sea_orm(
        column_type = "Text",
        comment = "服务描述（LLM 可见，覆盖 AgentCard 中的 description）"
    )]
    pub description: String,
    /// 远程 A2A 服务基础 URL（如 "http://remote-agent:3000"）
    #[sea_orm(
        column_type = "String(StringLen::N(512))",
        comment = "远程 A2A 服务基础 URL（如 http://remote-agent:3000）"
    )]
    pub base_url: String,
    /// 连接超时（秒），NULL 使用默认值 60
    #[sea_orm(comment = "连接超时（秒），NULL 使用默认值 60")]
    pub timeout_secs: Option<i32>,
    /// A2A 服务配置（JSON），使用与本地 `a2a.json` 相同的 `a2aServers` 包裹格式，
    /// 便于用户直接拷贝粘贴（`base_url` / `description` / `timeout_secs` / `headers`）。
    ///
    /// 示例：
    /// ```json
    /// {
    ///   "a2aServers": {
    ///     "weather_agent": {
    ///       "base_url": "http://weather-agent:3000",
    ///       "description": "Query weather information",
    ///       "timeout_secs": 60,
    ///       "headers": {
    ///         "Authorization": "Bearer your-token",
    ///         "X-Api-Key": "your-api-key"
    ///       }
    ///     }
    ///   }
    /// }
    /// ```
    #[sea_orm(
        column_type = "Text",
        comment = "A2A 服务配置（JSON，a2aServers 包裹格式）"
    )]
    pub config_json: Option<String>,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 是否删除
    #[sea_orm(default_value = false, comment = "是否删除")]
    pub deleted: bool,
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
