//! 智能体与 MCP 服务绑定关系实体（dense 格式）
//!
//! 记录智能体绑定了哪些 MCP 服务注册表中的服务，
//! 以及该绑定关系下的工具白名单覆盖。

use chrono::Local;
use sea_orm::{
    FromQueryResult, Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

use super::mcp_service_registry;

/// 智能体与 MCP 服务绑定关系
///
/// 通过 `registry_id` 引用 `t_agent_mcp_service_registry`，
/// 支持多个智能体共享同一 MCP 服务注册项，并可按绑定关系覆盖工具白名单。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(
    table_name = "t_agent_mcp_service_rel",
    comment = "智能体与 MCP 服务绑定关系表"
)]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key, auto_increment = true, comment = "关系ID")]
    pub id: i64,
    /// 智能体ID
    #[sea_orm(comment = "智能体ID", indexed)]
    pub agent_id: i64,
    /// MCP 服务注册表ID
    #[sea_orm(comment = "MCP 服务注册表ID", indexed)]
    pub registry_id: i64,
    /// 允许使用的工具列表覆盖（JSON 数组，空=使用注册表默认值）
    #[sea_orm(
        column_type = "Text",
        default_value = "[]",
        comment = "允许使用的工具列表覆盖（JSON 数组，空=使用注册表默认值）"
    )]
    pub enabled_tools: String,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 创建时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "创建时间")]
    pub created_at: DateTime,
    /// 更新时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "更新时间")]
    pub updated_at: DateTime,
    /// 关联的 MCP 服务注册项（历史表未建外键，`skip_fk` 保持 DDL 不变，仅生成关系）
    #[sea_orm(
        belongs_to,
        skip_fk,
        from = "registry_id",
        to = "id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    pub mcp_service_registry: BelongsTo<mcp_service_registry::Entity>,
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
