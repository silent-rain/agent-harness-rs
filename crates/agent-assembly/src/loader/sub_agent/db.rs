//! 基于数据库的子智能体加载器
//!
//! 从 `t_agent_sub_agent_rel` 装配指定智能体启用的子智能体装配包列表。
//! 每个子智能体会进一步加载其自身的能力绑定（规则 / 命令 / 技能 / MCP / 工具）
//! 和模型绑定，组装为可直接被运行时消费的自包含 [`AgentBundle`]。
//!
//! 嵌套子智能体不会在此处递归加载，运行时由
//! `max_delegation_depth` 控制后续装配深度，避免无限循环。
//!
//! 提供的主要内容：
//! - [`DbSubAgentLoader`]：数据库子智能体加载器
//! - [`AgentBundle`]：子智能体装配产出

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use tracing::debug;

use agent_runtime::types::runtime::{AgentBundle, AgentDefinition, ModelConfig};
use entity::agent::{agent_config, llm_model, llm_model_rel, sub_agent_rel};

use crate::{
    error::{Error, PlatformResult},
    loader::{
        a2a::DbA2aLoader, command::DbCommandLoader, mcp::DbMcpLoader, rule::DbRuleLoader,
        skill::DbSkillLoader, tool::DbToolLoader,
    },
};

/// 数据库子智能体加载器
///
/// 按 `agent_id` 查询启用的子智能体绑定，并对每个子智能体执行：
///
/// 1. 加载自身 `t_agent_config` 基础信息
/// 2. 通过 [`DbToolLoader`] / [`DbRuleLoader`] / [`DbSkillLoader`]
///    / [`DbMcpLoader`] / [`DbCommandLoader`] 装配各类能力
/// 3. 加载自身模型绑定（缺失时退回 `ModelBindingConfig::default()`）
/// 4. `alias` 优先作为子智能体名称，缺省回退到 `agent_config.name`
///
/// 加载过程不做任何缓存，单条子智能体加载失败会被跳过并记录 debug 日志，
/// 不影响其它子智能体的装配。
pub struct DbSubAgentLoader {
    db: DatabaseConnection,
}

impl DbSubAgentLoader {
    /// 创建数据库子智能体加载器
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// 加载指定智能体启用的子智能体装配包列表
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 父智能体 ID
    ///
    /// # Returns
    ///
    /// 启用的子智能体装配包列表；若无任何启用子智能体则返回空 `Vec`。
    /// 单个子智能体加载失败不会让整体失败，仅会被跳过。
    pub async fn load(&self, agent_id: i64) -> PlatformResult<Vec<AgentBundle>> {
        let rels = sub_agent_rel::Entity::find()
            .filter(sub_agent_rel::Column::AgentId.eq(agent_id))
            .filter(sub_agent_rel::Column::Enabled.eq(true))
            .all(&self.db)
            .await?;

        let mut sub_agents = Vec::with_capacity(rels.len());
        for rel in rels {
            match self.load_single(&rel).await {
                Ok(config) => sub_agents.push(config),
                Err(e) => {
                    debug!(
                        agent_id,
                        sub_agent_id = rel.sub_agent_id,
                        error = %e,
                        "Failed to load sub-agent full config, skipped"
                    );
                }
            }
        }

        Ok(sub_agents)
    }

    /// 装配单个子智能体的完整装配包
    ///
    /// 加载子智能体自身的 `t_agent_config` 与全部能力绑定（工具、规则、技能、
    /// MCP、命令），组装为自包含的 [`AgentBundle`]，可直接交给 `AgentBuilder`
    /// 或 `DelegateSubAgentTool` 使用。
    ///
    /// 嵌套子智能体不会在此处递归加载，避免出现死循环；下一层是否继续装配
    /// 由运行时根据 `max_delegation_depth` 决定。
    ///
    /// # Parameters
    ///
    /// * `rel` - 子智能体绑定关系记录
    async fn load_single(&self, rel: &sub_agent_rel::Model) -> PlatformResult<AgentBundle> {
        let sub_agent_id = rel.sub_agent_id;

        // 1. 加载子智能体自身的 agent_config 基础信息
        let sub_config = agent_config::Entity::find_by_id(sub_agent_id)
            .filter(agent_config::Column::Deleted.eq(false))
            .one(&self.db)
            .await?
            .ok_or(Error::AgentNotFound(sub_agent_id))?;

        // 2. 并行加载子智能体自身的各类能力绑定
        //    单项失败不影响整体（保持与原实现一致的"宽松装配"语义）
        let tool_configs = DbToolLoader::new(self.db.clone())
            .load(sub_agent_id)
            .await
            .unwrap_or_default();
        let rule_configs = DbRuleLoader::new(self.db.clone())
            .load(sub_agent_id)
            .await
            .unwrap_or_default();
        let skill_configs = DbSkillLoader::new(self.db.clone())
            .load(sub_agent_id)
            .await
            .unwrap_or_default();
        let mcp_configs = DbMcpLoader::new(self.db.clone())
            .load(sub_agent_id)
            .await
            .unwrap_or_default();
        let command_configs = DbCommandLoader::new(self.db.clone())
            .load(sub_agent_id)
            .await
            .unwrap_or_default();
        let a2a_configs = DbA2aLoader::new(self.db.clone())
            .load(sub_agent_id)
            .await
            .unwrap_or_default();

        // 3. 加载子智能体自身的模型绑定（缺失时回落到默认值，避免阻塞装配）
        let model_config = self
            .load_model_binding(sub_agent_id)
            .await
            .unwrap_or_default();

        // 4. 名称优先取 alias，缺省回退到 agent_config.name
        let name = rel.alias.clone().unwrap_or_else(|| sub_config.name.clone());

        Ok(AgentBundle {
            agent_definition: AgentDefinition {
                id: sub_config.id,
                user_id: sub_config.user_id.to_string(),
                name,
                description: sub_config.description.clone(),
                welcome_message: sub_config.welcome_message.clone(),
                system_prompt: sub_config.system_prompt.clone(),
                max_delegation_depth: sub_config.max_delegation_depth,
            },
            model_config,
            tool_configs,
            rule_configs,
            skill_configs,
            mcp_configs,
            command_configs,
            sub_agent_bundles: Vec::new(),
            a2a_configs,
        })
    }

    /// 查询子智能体的模型绑定
    ///
    /// 关联 `t_agent_llm_model_rel` 与 `t_agent_llm_model` 装配
    /// [`ModelBindingConfig`]。任一环节查询失败均会向上返回错误，由调用方
    /// 决定是否回落到默认值。
    async fn load_model_binding(&self, sub_agent_id: i64) -> PlatformResult<ModelConfig> {
        let binding = llm_model_rel::Entity::find()
            .filter(llm_model_rel::Column::AgentId.eq(sub_agent_id))
            .one(&self.db)
            .await?
            .ok_or(Error::ModelBindingNotFound(sub_agent_id))?;

        let llm = llm_model::Entity::find_by_id(binding.model_id)
            .filter(llm_model::Column::Enabled.eq(true))
            .one(&self.db)
            .await?
            .ok_or(Error::LlmModelNotFound(binding.model_id))?;

        Ok(ModelConfig {
            provider: llm.provider,
            model: llm.model_code,
            base_url: llm.base_url,
            api_key: llm.api_key,
            temperature: binding.temperature,
            max_tokens: binding.max_tokens.map(|v| v.max(0) as u64),
        })
    }
}
