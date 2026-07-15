//! 基于数据库的 A2A 服务加载器
//!
//! 从 `t_agent_a2a_service_rel` JOIN `t_agent_a2a_service_registry` 装配
//! 指定智能体启用的 A2A 服务配置列表。
//! 与 `agent_runtime::a2a::client::loader::LocalA2aLoader` 形成对称实现：
//! 前者属于装配层（依赖 SeaORM），后者属于核心运行时（零外部存储依赖）。
//!
//! 提供的主要内容：
//! - [`DbA2aLoader`]：数据库 A2A 服务加载器

use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
};

use agent_runtime::types::a2a::{A2aServersWrapper, A2aServiceConfig};
use entity::agent::{a2a_service_registry, a2a_service_rel};

use crate::error::{Error, PlatformResult};

/// 数据库 A2A 服务加载器
///
/// 按 `agent_id` 查询启用的 A2A 服务绑定，并装配为 [`A2aServiceConfig`] 列表。
/// 加载过程不做任何缓存，调用方应在自己的装配流程中按需调用。
///
/// ## 与 `LocalA2aLoader` 的关系
///
/// `LocalA2aLoader` 从本地 `a2a.json` 文件读取配置；
/// `DbA2aLoader` 从 `t_agent_a2a_service_rel` JOIN `t_agent_a2a_service_registry` 读取配置，
/// 支持按 `agent_id` 精确过滤，适用于多租户场景。
pub struct DbA2aLoader {
    db: DatabaseConnection,
}

impl DbA2aLoader {
    /// 创建数据库 A2A 服务加载器
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// 加载指定智能体启用的 A2A 服务配置列表
    ///
    /// 查询逻辑：
    /// 1. JOIN `t_agent_a2a_service_registry`，取出 `rel.enabled = true` 且 `registry.enabled = true` 的记录
    /// 2. 从 `registry.config_json`（`a2aServers` 包裹格式）解析 [`A2aServiceConfig`]；
    ///    `config_json` 缺失、JSON 格式错误或服务名不存在时直接返回错误
    /// 3. 按查询顺序输出，保持数据库默认排序语义
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    ///
    /// # Returns
    ///
    /// 启用的 A2A 服务配置列表；若无任何启用绑定则返回空 `Vec`
    pub async fn load(&self, agent_id: i64) -> PlatformResult<Vec<A2aServiceConfig>> {
        let results = a2a_service_rel::Entity::find()
            .filter(a2a_service_rel::Column::AgentId.eq(agent_id))
            .filter(a2a_service_rel::Column::Enabled.eq(true))
            .join(
                JoinType::InnerJoin,
                a2a_service_rel::Relation::A2aServiceRegistry.def(),
            )
            .filter(a2a_service_registry::Column::Enabled.eq(true))
            .select_also(a2a_service_registry::Entity)
            .all(&self.db)
            .await?;

        let services = results
            .into_iter()
            .map(|(rel, registry_opt)| {
                let registry = registry_opt.ok_or_else(|| {
                    Error::Internal(format!(
                        "A2A service registry not found for rel id={}",
                        rel.id
                    ))
                })?;

                // Parse A2aServiceConfig from registry config_json (a2aServers wrapper format).
                let json = registry.config_json.as_deref().ok_or_else(|| {
                    Error::Internal(format!(
                        "A2A service registry '{}' has no config_json",
                        registry.name
                    ))
                })?;
                let mut wrapper = serde_json::from_str::<A2aServersWrapper>(json).map_err(|e| {
                    Error::Internal(format!(
                        "Failed to parse config_json for A2A service '{}': {e}",
                        registry.name
                    ))
                })?;
                wrapper.a2a_servers.remove(&registry.name).ok_or_else(|| {
                    Error::Internal(format!(
                        "A2A service '{}' not found in config_json",
                        registry.name
                    ))
                })
            })
            .collect::<PlatformResult<Vec<_>>>()?;

        Ok(services)
    }
}
