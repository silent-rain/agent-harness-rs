//! 基于数据库的规则加载器
//!
//! 从 `t_agent_rule_rel` 与 `t_agent_rule_registry` 装配指定智能体启用的规则列表。
//! 与 `agent_runtime::rule::loader::LocalRuleLoader` 形成对称实现：
//! 前者属于装配层（依赖 SeaORM），后者属于核心运行时（零外部存储依赖）。
//!
//! 提供的主要内容：
//! - [`DbRuleLoader`]：数据库规则加载器

use std::collections::HashMap;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use agent_runtime::types::rule::{RuleConfig, RuleSource};
use entity::agent::{rule_registry, rule_rel};

use crate::error::PlatformResult;

/// 数据库规则加载器
///
/// 按 `agent_id` 查询启用的规则绑定，并按 `priority` 降序合并规则注册表内容。
/// 加载过程不做任何缓存，调用方应在自己的装配流程中按需调用。
///
/// ## 与 `LocalRuleLoader` 的关系
///
/// `LocalRuleLoader` 输出 `RuleSource::Local` 的规则；
/// `DbRuleLoader` 输出 `RuleSource::Injected` 的规则，并以 `db:rule:{id}` 作为 `source_hint`，
/// 便于运行期追踪规则来源。
pub struct DbRuleLoader {
    db: DatabaseConnection,
}

impl DbRuleLoader {
    /// 创建数据库规则加载器
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// 加载指定智能体启用的规则列表
    ///
    /// 查询逻辑：
    /// 1. 从 `t_agent_rule_rel` 取出启用绑定，按 `priority` 降序排序
    /// 2. 根据绑定中的 `rule_id` 批量查询 `t_agent_rule_registry`
    /// 3. 仅保留 `enabled = true` 且 `deleted = false` 的规则
    /// 4. 按原绑定顺序输出，保持优先级语义
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    ///
    /// # Returns
    ///
    /// 启用的规则列表，按 `priority` 降序排列；若无任何启用规则则返回空 `Vec`
    pub async fn load(&self, agent_id: i64) -> PlatformResult<Vec<RuleConfig>> {
        let rels = rule_rel::Entity::find()
            .filter(rule_rel::Column::AgentId.eq(agent_id))
            .filter(rule_rel::Column::Enabled.eq(true))
            .all(&self.db)
            .await?;

        if rels.is_empty() {
            return Ok(Vec::new());
        }

        // 按 priority 降序（数值越大越靠前）
        let mut sorted_rels = rels;
        sorted_rels.sort_by_key(|r| std::cmp::Reverse(r.priority));
        let rule_ids: Vec<i64> = sorted_rels.iter().map(|r| r.rule_id).collect();

        let registries = rule_registry::Entity::find()
            .filter(rule_registry::Column::Id.is_in(rule_ids.clone()))
            .filter(rule_registry::Column::Enabled.eq(true))
            .filter(rule_registry::Column::Deleted.eq(false))
            .all(&self.db)
            .await?;

        let reg_map: HashMap<i64, rule_registry::Model> =
            registries.into_iter().map(|r| (r.id, r)).collect();

        let rules = rule_ids
            .into_iter()
            .filter_map(|id| reg_map.get(&id))
            .map(|reg| RuleConfig {
                name: reg.name.clone(),
                description: reg.description.clone(),
                content: reg.content.clone(),
                source: RuleSource::Injected,
                source_hint: Some(format!("db:rule:{}", reg.id)),
            })
            .collect();

        Ok(rules)
    }
}
