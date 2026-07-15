//! 基于数据库的 MCP 服务加载器
//!
//! 从 `t_agent_mcp_service_rel` JOIN `t_agent_mcp_service_registry` 装配
//! 指定智能体启用的 MCP 服务配置列表。
//! 与 `agent_base::mcp::loader::LocalMcpLoader` 形成对称实现：
//! 前者属于装配层（依赖 SeaORM），后者属于核心运行时（零外部存储依赖）。
//!
//! 提供的主要内容：
//! - [`DbMcpLoader`]：数据库 MCP 服务加载器

use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
};

use agent_base::{
    types::mcp::{McpServerConfig, McpServersWrapper},
    utils::mcp::infer_transport_type,
};
use entity::agent::{mcp_service_registry, mcp_service_rel};

use crate::error::{Error, PlatformResult};

/// 数据库 MCP 服务加载器
///
/// 按 `agent_id` 查询启用的 MCP 服务绑定，并装配为 [`McpServerConfig`] 列表。
/// 加载过程不做任何缓存，调用方应在自己的装配流程中按需调用。
///
/// ## 与 `LocalMcpLoader` 的关系
///
/// `LocalMcpLoader` 从本地 `mcp.json` 文件读取配置；
/// `DbMcpLoader` 从 `t_agent_mcp_service_rel` JOIN `t_agent_mcp_service_registry` 读取配置，
/// 支持按 `agent_id` 精确过滤，适用于多租户场景。
pub struct DbMcpLoader {
    db: DatabaseConnection,
}

impl DbMcpLoader {
    /// 创建数据库 MCP 服务加载器
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// 加载指定智能体启用的 MCP 服务配置列表
    ///
    /// 查询逻辑：
    /// 1. JOIN `t_agent_mcp_service_registry`，取出 `rel.enabled = true` 且 `registry.enabled = true` 的记录
    /// 2. 优先使用 `rel.enabled_tools`（非空时）覆盖注册表默认工具白名单
    /// 3. 从 `registry.config_json`（`mcpServers` 包裹格式）解析 [`McpServiceEntry`]；
    ///    `config_json` 缺失、JSON 格式错误或服务名不存在时直接返回错误
    /// 4. 自动推断并补全 `transport_type`（规则与 `LocalMcpLoader` 一致）
    /// 5. 按查询顺序输出，保持数据库默认排序语义
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    ///
    /// # Returns
    ///
    /// 启用的 MCP 服务配置列表；若无任何启用绑定则返回空 `Vec`
    pub async fn load(&self, agent_id: i64) -> PlatformResult<Vec<McpServerConfig>> {
        let results = mcp_service_rel::Entity::find()
            .filter(mcp_service_rel::Column::AgentId.eq(agent_id))
            .filter(mcp_service_rel::Column::Enabled.eq(true))
            .join(
                JoinType::InnerJoin,
                mcp_service_rel::Relation::McpServiceRegistry.def(),
            )
            .filter(mcp_service_registry::Column::Enabled.eq(true))
            .select_also(mcp_service_registry::Entity)
            .all(&self.db)
            .await?;

        let services = results
            .into_iter()
            .map(|(rel, registry_opt)| {
                let registry = registry_opt.ok_or_else(|| {
                    Error::Internal(format!(
                        "MCP service registry not found for rel id={}",
                        rel.id
                    ))
                })?;

                // Determine allowed tools: use rel override if non-empty, else fall back to registry default.
                let rel_tools =
                    serde_json::from_str::<Vec<String>>(&rel.enabled_tools).unwrap_or_default();
                let allowed_tools = if rel_tools.is_empty() {
                    serde_json::from_str::<Vec<String>>(&registry.enabled_tools).unwrap_or_default()
                } else {
                    rel_tools
                };

                // Parse McpServiceEntry from registry config_json (mcpServers wrapper format).
                let json = registry.config_json.as_deref().ok_or_else(|| {
                    Error::Internal(format!(
                        "MCP service registry '{}' has no config_json",
                        registry.name
                    ))
                })?;
                let mut wrapper = serde_json::from_str::<McpServersWrapper>(json).map_err(|e| {
                    Error::Internal(format!(
                        "Failed to parse config_json for MCP service '{}': {e}",
                        registry.name
                    ))
                })?;
                let mut server_config =
                    wrapper.mcp_servers.remove(&registry.name).ok_or_else(|| {
                        Error::Internal(format!(
                            "MCP service '{}' not found in config_json",
                            registry.name
                        ))
                    })?;

                // Normalize and infer transport_type (mirrors LocalMcpLoader::infer_transport_type)
                server_config.transport_type = Some(infer_transport_type(&server_config));

                Ok(McpServerConfig {
                    service_name: registry.name,
                    allowed_tools,
                    server_config,
                })
            })
            .collect::<PlatformResult<Vec<_>>>()?;

        Ok(services)
    }
}
