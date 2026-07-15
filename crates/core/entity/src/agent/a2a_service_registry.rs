//! A2A 服务注册表实体
//!
//! 存储平台内建 A2A 远程 Agent 服务与用户自定义 A2A 服务，
//! 支持在线市场分发与用户私有服务的统一管理。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// A2A 服务注册表
///
/// 记录可供智能体绑定的 A2A 远程 Agent 服务元数据与连接配置。
/// 通过 `t_agent_a2a_service_rel` 与智能体建立多对多关联。
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_a2a_service_registry")]
pub struct Model {
    /// 服务ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID（0 表示平台内建）
    pub user_id: i64,
    /// 服务名称（同一用户下唯一，LLM 可见，如 "weather_agent"）
    pub name: String,
    /// 服务描述（LLM 可见，覆盖 AgentCard 中的 description）
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// 远程 A2A 服务基础 URL（如 "http://remote-agent:3000"）
    pub base_url: String,
    /// 连接超时（秒），NULL 使用默认值 60
    pub timeout_secs: Option<i32>,
    /// A2A 服务配置（JSON），使用与本地 `a2a.json` 相同的 `a2aServers` 包裹格式，
    /// 便于用户直接拷贝粘贴。示例：
    ///
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
    #[sea_orm(column_type = "Text", nullable)]
    pub config_json: Option<String>,
    /// 是否启用
    pub enabled: bool,
    /// 是否删除
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
