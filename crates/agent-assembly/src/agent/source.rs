//! 数据库驱动的智能体配置来源
//!
//! 提供 [`DbAgentSource`]：实现 [`agent_runtime::runtime::AgentSource`]，从数据库
//! 一次性加载完整的 [`AgentBundle`] 并喂给 [`AgentBuilder`]。
//!
//! 配套提供：
//! - [`AgentSummary`] —— 装配产物的能力摘要
//!
//! [`AgentBuilder`]: agent_runtime::runtime::AgentBuilder
//! [`AgentBundle`]: agent_runtime::types::runtime::AgentBundle
//! [`ModelAdapter`]: agent_runtime::model::ModelAdapter
//! [`RuntimeServices`]: agent_runtime::runtime::RuntimeServices
//! [`DbSessionService`]: crate::session::DbSessionService

use async_trait::async_trait;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use tracing::debug;

use agent_runtime::{
    error::AgentResult,
    runtime::AgentSource,
    types::{
        a2a::A2aServiceConfig,
        command::CommandConfig,
        mcp::McpServerConfig,
        rule::RuleConfig,
        runtime::{AgentBundle, AgentDefinition, ModelConfig},
        skill::SkillConfig,
        tool::ToolConfig,
    },
};
use entity::agent::{agent_config, llm_model, llm_model_rel};

use crate::{
    error::{Error, PlatformResult},
    loader::{
        a2a::DbA2aLoader, command::DbCommandLoader, mcp::DbMcpLoader, rule::DbRuleLoader,
        skill::DbSkillLoader, sub_agent::DbSubAgentLoader, tool::DbToolLoader,
    },
};

/// 装配产物的能力摘要
///
/// 可由 [`AgentSummary::from_bundle`] 直接从 [`AgentBundle`] 计算得到，
/// 便于上层快速展示「已加载多少规则 / 命令 / 技能 / MCP / 子智能体」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentSummary {
    /// agent_id
    pub agent_id: i64,
    /// agent 名称
    pub name: String,
    /// 模型名（如 `gpt-4o-mini`）
    pub model: String,
    /// 模型供应商（如 `openai`）
    pub provider: String,
    /// 已加载规则数量
    pub rules: usize,
    /// 已加载命令数量
    pub commands: usize,
    /// 已加载技能数量
    pub skills: usize,
    /// 已加载 MCP 服务数量
    pub mcp: usize,
    /// 已加载 A2A 远程 Agent 服务数量
    pub a2a: usize,
    /// 已加载子智能体数量
    pub sub_agents: usize,
}

impl AgentSummary {
    /// 从已加载的 [`AgentBundle`] 计算摘要
    pub fn from_bundle(bundle: &AgentBundle) -> Self {
        Self {
            agent_id: bundle.agent_definition.id,
            name: bundle.agent_definition.name.clone(),
            model: bundle.model_config.model.clone(),
            provider: bundle.model_config.provider.clone(),
            rules: bundle.rule_configs.len(),
            commands: bundle.command_configs.len(),
            skills: bundle.skill_configs.len(),
            mcp: bundle.mcp_configs.len(),
            a2a: bundle.a2a_configs.len(),
            sub_agents: bundle.sub_agent_bundles.len(),
        }
    }
}

/// 数据库驱动的配置来源
///
/// 实现 [`AgentSource`]，在 `load()` 时从数据库一次性装配完整的
/// [`AgentBundle`]（agent 定义 + 模型绑定 + rules / skills / mcp / commands /
/// tools / sub_agents）。
///
/// 涉及的数据库表：
/// - `t_agent_config`: 智能体基础信息
/// - `t_agent_llm_model_rel`: 模型绑定配置
/// - `t_agent_llm_model`: LLM 模型定义
/// - 各能力绑定表由对应 [`crate::loader`] 处理
///
/// 子智能体由 [`DbSubAgentLoader`] 递归装配其自身的能力绑定。
///
/// ## 使用方式
///
/// ```rust,no_run
/// # use std::sync::Arc;
/// # use sea_orm::DatabaseConnection;
/// # async fn example(db: DatabaseConnection) -> agent_assembly::PlatformResult<()> {
/// use agent_runtime::{
///     model::create_model,
///     runtime::{AgentBuilder, AgentSource},
///     RuntimeServices,
/// };
/// use agent_assembly::{
///     AgentSummary, DbAgentSource, DbSessionService,
/// };
///
/// let agent_id = 1001;
/// let user_id = 42;
///
/// // 1. 显式加载 bundle，用于构造 model + summary
/// let source = DbAgentSource::new(db.clone(), agent_id, user_id);
/// let bundle = source.load().await?;
/// let summary = AgentSummary::from_bundle(&bundle);
/// let model = create_model(&bundle.model_config)?;
/// let services = RuntimeServices {
///     session: Arc::new(DbSessionService::new(Arc::new(db.clone()))),
///     ..RuntimeServices::noop()
/// };
///
/// // 2. 装配 Agent —— DbAgentSource::load 会再次访问数据库加载 bundle
/// let source = DbAgentSource::new(db, agent_id, user_id);
/// let agent = AgentBuilder::new(bundle.to_agent_config())
///     .unwrap()
///     .add_source(source)
///     .with_services(services)
///     .build()
///     .await?;
/// # let _ = (summary, agent);
/// # Ok(())
/// # }
/// ```
pub struct DbAgentSource {
    db: DatabaseConnection,
    agent_id: i64,
    user_id: i64,
}

impl DbAgentSource {
    /// 创建数据库源
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    /// * `agent_id` - 智能体 ID（`t_agent_config.id`）
    /// * `user_id` - 用户 ID（用于权限隔离）
    pub fn new(db: DatabaseConnection, agent_id: i64, user_id: i64) -> Self {
        Self {
            db,
            agent_id,
            user_id,
        }
    }

    /// 从数据库加载完整的 [`AgentBundle`]
    ///
    /// 等价于 [`AgentSource::load`]，但返回 [`PlatformResult`] 以便平台层
    /// 直接消费数据库错误的具体类型。
    pub async fn load_bundle(&self) -> PlatformResult<AgentBundle> {
        debug!(
            agent_id = self.agent_id,
            user_id = self.user_id,
            "从数据库加载智能体完整配置"
        );

        // 1. 加载基础配置（不存在直接返回错误）
        let agent_config = self.load_agent_config().await?;

        // 2. 加载模型绑定 + 各类能力（并行执行，复用同一 db 连接）
        let (
            model_config,
            rule_configs,
            command_configs,
            skill_configs,
            mcp_configs,
            tool_configs,
            sub_agent_bundles,
            a2a_configs,
        ) = tokio::try_join!(
            self.load_model_binding(),
            self.load_rules(),
            self.load_commands(),
            self.load_skills(),
            self.load_mcp_services(),
            self.load_tools(),
            self.load_sub_agents(),
            self.load_a2a_services(),
        )?;

        debug!(agent_id = self.agent_id, "成功加载智能体完整配置");

        Ok(AgentBundle {
            agent_definition: AgentDefinition {
                id: agent_config.id,
                user_id: agent_config.user_id.to_string(),
                name: agent_config.name,
                description: agent_config.description.clone(),
                welcome_message: agent_config.welcome_message.clone(),
                system_prompt: agent_config.system_prompt.clone(),
                max_delegation_depth: agent_config.max_delegation_depth,
            },
            model_config,
            tool_configs,
            rule_configs,
            skill_configs,
            mcp_configs,
            command_configs,
            sub_agent_bundles,
            a2a_configs,
        })
    }

    /// 查询智能体基础配置
    async fn load_agent_config(&self) -> PlatformResult<agent_config::Model> {
        let model = agent_config::Entity::find_by_id(self.agent_id)
            .filter(agent_config::Column::UserId.eq(self.user_id))
            .filter(agent_config::Column::Deleted.eq(false))
            .one(&self.db)
            .await?
            .ok_or(Error::AgentNotFound(self.agent_id))?;
        Ok(model)
    }

    /// 查询模型绑定配置（含 LLM 模型定义）
    async fn load_model_binding(&self) -> PlatformResult<ModelConfig> {
        let binding = llm_model_rel::Entity::find()
            .filter(llm_model_rel::Column::AgentId.eq(self.agent_id))
            .one(&self.db)
            .await?
            .ok_or(Error::ModelBindingNotFound(self.agent_id))?;

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

    /// 查询已启用的规则列表
    async fn load_rules(&self) -> PlatformResult<Vec<RuleConfig>> {
        DbRuleLoader::new(self.db.clone()).load(self.agent_id).await
    }

    /// 查询已启用的技能列表
    async fn load_skills(&self) -> PlatformResult<Vec<SkillConfig>> {
        DbSkillLoader::new(self.db.clone())
            .load(self.agent_id)
            .await
    }

    /// 查询已启用的命令列表
    async fn load_commands(&self) -> PlatformResult<Vec<CommandConfig>> {
        DbCommandLoader::new(self.db.clone())
            .load(self.agent_id)
            .await
    }

    /// 查询已启用的 MCP 服务绑定列表
    async fn load_mcp_services(&self) -> PlatformResult<Vec<McpServerConfig>> {
        DbMcpLoader::new(self.db.clone()).load(self.agent_id).await
    }

    /// 查询已启用的 A2A 远程 Agent 服务绑定列表
    async fn load_a2a_services(&self) -> PlatformResult<Vec<A2aServiceConfig>> {
        DbA2aLoader::new(self.db.clone()).load(self.agent_id).await
    }

    /// 查询已启用的工具列表
    async fn load_tools(&self) -> PlatformResult<Vec<ToolConfig>> {
        DbToolLoader::new(self.db.clone()).load(self.agent_id).await
    }

    /// 查询已启用的子智能体绑定列表
    async fn load_sub_agents(&self) -> PlatformResult<Vec<AgentBundle>> {
        DbSubAgentLoader::new(self.db.clone())
            .load(self.agent_id)
            .await
    }
}

#[async_trait]
impl AgentSource for DbAgentSource {
    async fn load(&self) -> AgentResult<AgentBundle> {
        self.load_bundle()
            .await
            .map_err(|e| agent_runtime::Error::Internal(e.to_string()))
    }
}
