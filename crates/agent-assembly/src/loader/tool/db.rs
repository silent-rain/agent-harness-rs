//! 基于数据库的工具加载器
//!
//! 从 `t_agent_tool_rel` 与 `t_agent_tool_registry` 装配指定智能体启用的工具列表。
//! 与 `DbRuleLoader` / `DbCommandLoader` / `DbSkillLoader` / `DbMcpLoader`
//! 保持同构的两表（绑定 + 注册表）查询模式。
//!
//! 提供的主要内容：
//! - [`DbToolLoader`]：数据库工具加载器

use std::collections::HashMap;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use agent_runtime::types::tool::ToolConfig;
use entity::agent::{tool_registry, tool_rel};

use crate::error::PlatformResult;

/// 数据库工具加载器
///
/// 按 `agent_id` 查询启用的工具绑定，并以 `tool_id` 为键合并工具注册表内容。
/// 仅返回 `enabled = true` 的注册表条目；若绑定指向已禁用或不存在的注册条目，
/// 该条记录会被跳过。
pub struct DbToolLoader {
    db: DatabaseConnection,
}

impl DbToolLoader {
    /// 创建数据库工具加载器
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// 加载指定智能体启用的工具列表
    ///
    /// 查询逻辑：
    /// 1. 从 `t_agent_tool_rel` 取出启用绑定
    /// 2. 根据绑定中的 `tool_id` 批量查询 `t_agent_tool_registry`
    /// 3. 仅保留 `enabled = true` 的注册条目
    /// 4. 按原绑定顺序输出，保持配置稳定性
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    ///
    /// # Returns
    ///
    /// 启用的工具列表；若无任何启用工具则返回空 `Vec`
    pub async fn load(&self, agent_id: i64) -> PlatformResult<Vec<ToolConfig>> {
        let rels = tool_rel::Entity::find()
            .filter(tool_rel::Column::AgentId.eq(agent_id))
            .filter(tool_rel::Column::Enabled.eq(true))
            .all(&self.db)
            .await?;

        if rels.is_empty() {
            return Ok(Vec::new());
        }

        let tool_ids: Vec<i64> = rels.iter().map(|r| r.tool_id).collect();

        let registries = tool_registry::Entity::find()
            .filter(tool_registry::Column::Id.is_in(tool_ids.clone()))
            .filter(tool_registry::Column::Enabled.eq(true))
            .all(&self.db)
            .await?;

        let reg_map: HashMap<i64, tool_registry::Model> =
            registries.into_iter().map(|r| (r.id, r)).collect();

        let tools = tool_ids
            .into_iter()
            .filter_map(|id| reg_map.get(&id))
            .map(|reg| ToolConfig {
                name: reg.tool_code.clone(),
                description: reg.description.clone(),
                schema_json: reg.schema_json.clone(),
            })
            .collect();

        Ok(tools)
    }
}
