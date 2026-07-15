//! 智能体与 MCP 服务绑定关系实体
//!
//! 记录智能体绑定了哪些 MCP 服务注册表中的服务，
//! 以及该绑定关系下的工具白名单覆盖。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EntityTrait, EnumIter, FromQueryResult, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体与 MCP 服务绑定关系
///
/// 通过 `registry_id` 引用 `t_agent_mcp_service_registry`，
/// 支持多个智能体共享同一 MCP 服务注册项，并可按绑定关系覆盖工具白名单。
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_mcp_service_rel")]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 智能体ID
    pub agent_id: i64,
    /// MCP 服务注册表ID
    pub registry_id: i64,
    /// 允许使用的工具列表覆盖（JSON 数组，空=使用注册表默认值）
    #[sea_orm(column_type = "Text")]
    pub enabled_tools: String,
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
        belongs_to = "super::mcp_service_registry::Entity",
        from = "Column::RegistryId",
        to = "super::mcp_service_registry::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    McpServiceRegistry,
}

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

/// MCP 服务绑定关联查询结果（JOIN mcp_service_registry）
///
/// 用于 `mcp_service_rel` 与 `mcp_service_registry` 关联查询，
/// 一次 SQL 获取绑定关系及其关联服务的名称、配置等信息。
/// 可直接作为 API 响应体序列化输出。
#[derive(Clone, Debug, Serialize, Deserialize, FromQueryResult)]
pub struct McpServiceRelWithRegistry {
    /// 绑定关系基础字段
    #[sea_orm(nested)]
    #[serde(flatten)]
    pub rel: Model,
    /// 服务名称
    pub registry_name: String,
    /// 服务描述
    pub registry_description: String,
    /// 传输类型
    pub registry_transport_type: String,
    /// 服务地址
    pub registry_mcp_service_url: Option<String>,
    /// 服务配置 JSON
    pub registry_config_json: Option<String>,
    /// 注册表默认工具白名单
    pub registry_enabled_tools: String,
    /// 注册表自身是否启用
    pub registry_enabled: bool,
}
