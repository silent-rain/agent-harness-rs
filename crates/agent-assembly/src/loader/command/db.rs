//! 基于数据库的命令加载器
//!
//! 从 `t_agent_command_rel` 与 `t_agent_command_registry` 装配指定智能体启用的命令列表。
//! 与 `agent_base::command::loader::LocalCommandLoader` 形成对称实现：
//! 前者属于装配层（依赖 SeaORM），后者属于核心运行时（零外部存储依赖）。
//!
//! 提供的主要内容：
//! - [`DbCommandLoader`]：数据库命令加载器

use std::collections::HashMap;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use agent_base::types::command::{CommandConfig, CommandSource};
use entity::agent::{command_registry, command_rel};

use crate::error::PlatformResult;

/// 数据库命令加载器
///
/// 按 `agent_id` 查询启用的命令绑定，并按 `priority` 升序合并命令注册表内容。
/// 加载过程不做任何缓存，调用方应在自己的装配流程中按需调用。
///
/// ## 与 `LocalCommandLoader` 的关系
///
/// `LocalCommandLoader` 输出 `CommandSource::Local` 的命令；
/// `DbCommandLoader` 输出 `CommandSource::Injected` 的命令，并以 `db:command:{id}` 作为
/// `source_hint`，便于运行期追踪命令来源。
pub struct DbCommandLoader {
    db: DatabaseConnection,
}

impl DbCommandLoader {
    /// 创建数据库命令加载器
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// 加载指定智能体启用的命令列表
    ///
    /// 查询逻辑：
    /// 1. 从 `t_agent_command_rel` 取出启用绑定，按 `priority` 升序排序（数值越小越靠前）
    /// 2. 根据绑定中的 `command_id` 批量查询 `t_agent_command_registry`
    /// 3. 仅保留 `enabled = true` 且 `deleted = false` 的命令
    /// 4. 按原绑定顺序输出，保持优先级语义
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    ///
    /// # Returns
    ///
    /// 启用的命令列表，按 `priority` 升序排列；若无任何启用命令则返回空 `Vec`
    pub async fn load(&self, agent_id: i64) -> PlatformResult<Vec<CommandConfig>> {
        let rels = command_rel::Entity::find()
            .filter(command_rel::Column::AgentId.eq(agent_id))
            .filter(command_rel::Column::Enabled.eq(true))
            .all(&self.db)
            .await?;

        if rels.is_empty() {
            return Ok(Vec::new());
        }

        // 按 priority 升序（数值越小越靠前）
        let mut sorted_rels = rels;
        sorted_rels.sort_by_key(|r| r.priority);
        let command_ids: Vec<i64> = sorted_rels.iter().map(|r| r.command_id).collect();

        let registries = command_registry::Entity::find()
            .filter(command_registry::Column::Id.is_in(command_ids.clone()))
            .filter(command_registry::Column::Enabled.eq(true))
            .filter(command_registry::Column::Deleted.eq(false))
            .all(&self.db)
            .await?;

        let reg_map: HashMap<i64, command_registry::Model> =
            registries.into_iter().map(|r| (r.id, r)).collect();

        let commands = command_ids
            .into_iter()
            .filter_map(|id| reg_map.get(&id))
            .map(|reg| CommandConfig {
                name: reg.name.clone(),
                description: reg.description.clone(),
                content: reg.content.clone(),
                source: CommandSource::Injected,
                source_hint: Some(format!("db:command:{}", reg.id)),
            })
            .collect();

        Ok(commands)
    }
}
