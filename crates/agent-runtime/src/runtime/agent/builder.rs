//! 智能体运行时构建器
use std::sync::Arc;

#[cfg(feature = "skill")]
use std::collections::HashSet;

use rig_core::tool::{DynamicTool, Tool};
#[cfg(any(
    feature = "rule",
    feature = "skill",
    feature = "mcp",
    feature = "command",
    feature = "sub_agent"
))]
use tracing::info;

#[cfg(feature = "command")]
use crate::{
    command::{CommandListTool, CommandLoadTool},
    types::command::CommandConfig,
};

#[cfg(feature = "mcp")]
use crate::{mcp::McpRegistry, types::mcp::McpServerConfig};

#[cfg(feature = "rule")]
use crate::{
    rule::{RuleListTool, RuleLoadTool},
    types::rule::RuleConfig,
};

#[cfg(feature = "skill")]
use crate::{
    skill::{
        LocalSkillLoader, SkillExecTool, SkillListDocsTool, SkillLoadTool, SkillSelectDocsTool,
    },
    types::skill::SkillConfig,
};

#[cfg(feature = "sub_agent")]
use crate::{
    sub_agent::{DelegateSubAgentTool, FlatSubAgentTool},
    types::agent::SubAgentDelegationMode,
};

#[cfg(feature = "a2a")]
use crate::{a2a::A2aRegistry, types::a2a::A2aServiceConfig};

use crate::{
    Error, RuntimeServices,
    error::AgentResult,
    hook::{AgentHook, HookRegistry},
    model::{ModelAdapter, create_model},
    runtime::{LlmAgent, agent::AgentSource},
    tool::{KnowledgeQueryTool, ToolPool},
    types::{
        runtime::{AgentBundle, AgentConfig},
        tool::ToolSource,
    },
};

/// 智能体构建器
///
/// 在构建阶段以 [`AgentBundle`] 为统一装配蓝图：所有 source 加载结果、外部
/// 直接注入的能力配置都汇入 `bundle`，再由 builder 统一去重 / 截断 / 装配工具。
///
/// ## 核心设计：Source 驱动 + Bundle 装配
///
/// Builder 通过一组实现了 [`AgentSource`] 的 source 来装配能力配置。
/// 每个 source 加载并返回一个 [`AgentBundle`] 片段，Builder 在 `build()`
/// 阶段按注册顺序合并到内部 bundle，再统一应用去重 / 深度截断 / 工具装配。
///
/// 内置 source：
/// - [`LocalAgentSource`](super::source::LocalAgentSource) — 扫描 `.codebuddy/*` 本地目录
///
/// 需要以预构造 [`AgentBundle`] 作为种子时（如子智能体递归装配），
/// 使用 [`AgentBuilder::from_bundle`] 直接注入 bundle，无需绕道 source 抽象。
///
/// 用户也可实现自定义 [`AgentSource`]（如数据库、远程 RPC 等）。
///
/// ## 构造入口
///
/// - [`AgentBuilder::new`]：以 [`AgentConfig`](crate::types::runtime::AgentConfig)
///   为身份种子创建构建器，内部自动调用 [`create_model`] 创建模型，**不附带 source**，
///   适用于/// - [`AgentBuilder::from_sources`]：以 source 列表为起点，适用于“多源装配”场景
///
/// 两者都使用默认 [`RuntimeServices::noop()`] 初始化运行时服务，
/// 如需实际服务请调用 [`AgentBuilder::with_services`] 进行覆盖。
///
/// ## 工具初始化流程
///
/// 1. 调用方通过 `add_tool()` / `add_tools()` 装填候选池（可选）
/// 2. 调用方通过 `allow_tools()` 设置白名单（可选；未设置默认放行全部）
/// 3. 多 Agent 共享候选池场景：调用 `with_global_tool_pool()` 用外部 `Arc<ToolPool>`
/// 4. `build()` 时把候选池固化为 `Arc<ToolPool>`，再按白名单生成 `ToolSet`
///
/// ## 装配流程
///
/// 1. 依次调用所有 source 的 `load()`，把片段 `merge` 到 `self.bundle`
///    （`merge` 内部会自动触发 first-wins 去重 + 委派深度截断）
/// 2. 对手动注入场景补一次 `bundle.normalize()`
/// 3. 加载内置工具 + 用户额外注册的工具
/// 4. 各 `load_*` 方法读取已规范化的 bundle 并注入对应工具
/// 5. 候选池冻结为 `Arc<ToolPool>`
/// 6. 从 bundle 抽取 `AgentConfig` + 能力配置，构造 `LlmAgent`
///
/// # Examples
///
/// ```rust,no_run
/// # use std::sync::Arc;
/// # use agent_runtime::{Agent, RuntimeServices};
/// # use agent_runtime::AgentBuilder;
/// # use agent_runtime::runtime::LocalAgentSource;
/// # use agent_runtime::model::ModelAdapter;
/// # use agent_runtime::tool::{ToolPool, ReadFileTool};
/// # use agent_runtime::types::runtime::AgentConfig;
/// # async fn example(model: Arc<dyn ModelAdapter>, agent_config: AgentConfig)
/// #     -> agent_runtime::error::AgentResult<()> {
/// // Scenario A: identity-only seed; capabilities injected via source
/// let agent = AgentBuilder::new(agent_config.clone())?
///     .add_source(LocalAgentSource::default().with_all_enabled())
///     .allow_tools(["read_file"])
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct AgentBuilder {
    /// 配置来源列表（按注册顺序合并到 bundle）
    sources: Vec<Arc<dyn AgentSource>>,
    /// Agent 装配 bundle —— 由 sources 加载后合并而成
    bundle: AgentBundle,
    model: Arc<dyn ModelAdapter>,
    services: RuntimeServices,
    /// MCP 连接注册表（保留为连接管理器，后续会演进为连接池）
    #[cfg(feature = "mcp")]
    mcp_registry: McpRegistry,
    /// A2A 连接注册表
    #[cfg(feature = "a2a")]
    a2a_registry: A2aRegistry,
    /// 全局工具池
    global_tool_pool: Arc<ToolPool>,
    /// 候选工具池
    tool_pool: ToolPool,
    /// 工具白名单：None 表示放行全部，Some 表示仅放行列表中的 `tool_code`
    tool_whitelist: Option<Vec<String>>,
    // === 便捷 API 控制字段 ===
    /// Dynamic skill whitelist from convenience API
    #[cfg(feature = "skill")]
    skill_whitelist: Option<Vec<String>>,
    /// Sub-agent delegation mode (flat vs single-tool)
    #[cfg(feature = "sub_agent")]
    delegation_mode: SubAgentDelegationMode,
    /// 钩子注册表
    hooks: HookRegistry,
}

impl AgentBuilder {
    /// 以 [`AgentConfig`] 为身份种子创建构建器（最常用）
    ///
    /// `config` 中的 `model_config` 会被用于自动调用 [`create_model`] 创建 LLM 模型实例。
    /// `agent_definition` 与 `model_config` 会被填充到内部 [`AgentBundle`]，
    /// 作为最终 [`LlmAgent`] 的身份快照。`sources` 初始为空，
    /// 可通过 [`AgentBuilder::add_source`] / [`AgentBuilder::add_source_arc`]
    /// 后续追加。
    ///
    /// 运行时服务默认使用 [`RuntimeServices::noop()`]，如需覆盖请使用
    /// [`AgentBuilder::with_services`]。
    ///
    /// # Parameters
    ///
    /// * `config` - 智能体身份与模型绑定配置（含 `model_config`，用于自动创建模型）
    ///
    /// # Errors
    ///
    /// 当 `model_config` 中的配置无效导致模型创建失败时返回错误。
    pub fn new(config: AgentConfig) -> AgentResult<Self> {
        let model = create_model(&config.model_config)?;
        Ok(Self::new_with_model(config, model))
    }

    /// 以 [`AgentConfig`] 和外部传入的模型实例创建构建器
    ///
    /// 适用于需要自定义模型实例的场景（如测试 Mock、特殊模型适配器等）。
    /// `config` 中的 `agent_definition` 与 `model_config` 会被填充到
    /// 内部 [`AgentBundle`]，作为最终 [`LlmAgent`] 的身份快照。`sources` 初始为空，
    /// 可通过 [`AgentBuilder::add_source`] / [`AgentBuilder::add_source_arc`]
    /// 后续追加。
    ///
    /// 运行时服务默认使用 [`RuntimeServices::noop()`]，如需覆盖请使用
    /// [`AgentBuilder::with_services`]。
    ///
    /// # Parameters
    ///
    /// * `config` - 智能体身份与模型绑定配置
    /// * `model` - 外部构造的 LLM 模型实例
    pub fn new_with_model(config: AgentConfig, model: Arc<dyn ModelAdapter>) -> Self {
        Self {
            sources: Vec::new(),
            bundle: AgentBundle {
                agent_definition: config.agent_definition,
                model_config: config.model_config,
                ..Default::default()
            },
            model,
            services: RuntimeServices::noop(),
            #[cfg(feature = "mcp")]
            mcp_registry: McpRegistry::default(),
            #[cfg(feature = "a2a")]
            a2a_registry: A2aRegistry::default(),
            global_tool_pool: Arc::new(ToolPool::new()),
            tool_pool: ToolPool::new(),
            tool_whitelist: None,
            #[cfg(feature = "skill")]
            skill_whitelist: None,
            #[cfg(feature = "sub_agent")]
            delegation_mode: SubAgentDelegationMode::default(),
            hooks: HookRegistry::new(),
        }
    }

    /// 以预构造的 [`AgentBundle`] 为种子创建构建器
    ///
    /// 适用于 bundle 已在外部装配完成的场景（最典型的是子智能体递归装配）：
    /// 直接把完整 bundle 注入 builder，无需绕道 source 抽象。
    /// 后续仍可通过 [`add_source`](Self::add_source) /
    /// [`add_source_arc`](Self::add_source_arc) 追加继承自父 Agent 的 source。
    ///
    /// # Parameters
    ///
    /// * `bundle` - 预构造的装配蓝图
    /// * `model` - 外部构造的 LLM 模型实例
    pub fn from_bundle(bundle: AgentBundle, model: Arc<dyn ModelAdapter>) -> Self {
        Self {
            sources: Vec::new(),
            bundle,
            model,
            services: RuntimeServices::noop(),
            #[cfg(feature = "mcp")]
            mcp_registry: McpRegistry::default(),
            #[cfg(feature = "a2a")]
            a2a_registry: A2aRegistry::default(),
            global_tool_pool: Arc::new(ToolPool::new()),
            tool_pool: ToolPool::new(),
            tool_whitelist: None,
            #[cfg(feature = "skill")]
            skill_whitelist: None,
            #[cfg(feature = "sub_agent")]
            delegation_mode: SubAgentDelegationMode::default(),
            hooks: HookRegistry::new(),
        }
    }

    /// 覆盖运行时服务
    ///
    /// 默认初始为 [`RuntimeServices::noop()`]，调用本方法可在不同环境
    /// （生产 / 测试 / 例子）动态切换实际的运行时服务实现。
    pub fn with_services(mut self, services: RuntimeServices) -> Self {
        self.services = services;
        self
    }

    /// 以 [`AgentConfig`] 填充内部 bundle 的身份字段
    ///
    /// 多次调用仅后一次生效（后写覆盖）。
    pub fn with_agent_config(mut self, config: AgentConfig) -> Self {
        self.bundle.agent_definition = config.agent_definition;
        self.bundle.model_config = config.model_config;
        self
    }

    /// 追加一个配置来源, 用于填充内部 bundle 的资源字段，例如 rules、skills等。智能体配置需要主动进行设置
    ///
    /// # Parameters
    ///
    /// * `source` - 配置来源
    pub fn add_source(mut self, source: impl AgentSource + 'static) -> Self {
        self.sources.push(Arc::new(source));
        self
    }

    /// 追加一个已包装为 `Arc` 的配置来源
    ///
    /// 与 [`add_source`](Self::add_source) 等价，但接受已包装的 `Arc`，
    /// 适用于子 Agent 透传场景（避免重复包装）。
    pub fn add_source_arc(mut self, source: Arc<dyn AgentSource>) -> Self {
        self.sources.push(source);
        self
    }

    // ===== 底层 API：注入已装配的对象 =====

    /// 注入已连接的 MCP 注册表
    ///
    /// 调用方应在外部完成 `McpRegistry::connects()` 后再注入。
    #[cfg(feature = "mcp")]
    pub fn with_mcp_registry(mut self, registry: McpRegistry) -> Self {
        self.mcp_registry = registry;
        self
    }

    /// 注入预构建的工具池
    pub fn with_global_tool_pool(mut self, pool: Arc<ToolPool>) -> Self {
        self.global_tool_pool = pool;
        self
    }

    // ===== 便捷 API：直接向 bundle 追加配置 =====

    /// 追加规则配置
    ///
    /// 多次调用累加，由 builder 在 `build()` 阶段按 `name` 去重
    /// （先注入者优先）。
    #[cfg(feature = "rule")]
    pub fn add_rules(mut self, configs: Vec<RuleConfig>) -> Self {
        self.bundle.rule_configs.extend(configs);
        self
    }

    /// 追加技能配置
    ///
    /// 多次调用累加，由 builder 在 `build()` 阶段按 `skill_code` 去重
    /// （先注入者优先，本地优先于 DB）。
    #[cfg(feature = "skill")]
    pub fn add_skills(mut self, configs: Vec<SkillConfig>) -> Self {
        self.bundle.skill_configs.extend(configs);
        self
    }

    /// 追加 MCP 服务配置
    ///
    /// 多次调用累加，由 `McpRegistry` 在连接阶段按 `service_name` 去重。
    #[cfg(feature = "mcp")]
    pub fn add_mcps(mut self, configs: Vec<McpServerConfig>) -> Self {
        self.bundle.mcp_configs.extend(configs);
        self
    }

    /// 追加命令配置
    ///
    /// 多次调用累加，由 builder 在 `build()` 阶段按 `name` 去重
    /// （先注入者优先）。
    #[cfg(feature = "command")]
    pub fn add_commands(mut self, configs: Vec<CommandConfig>) -> Self {
        self.bundle.command_configs.extend(configs);
        self
    }

    /// 追加子智能体装配包
    ///
    /// 多次调用累加，由 builder 在 `build()` 阶段按 `agent_definition.name`
    /// 去重（先注入者优先），并根据父级 `max_delegation_depth` 截断子树深度。
    #[cfg(feature = "sub_agent")]
    pub fn add_sub_agents(mut self, bundles: Vec<AgentBundle>) -> Self {
        self.bundle.sub_agent_bundles.extend(bundles);
        self
    }

    /// 追加 a2a 服务配置列表
    ///
    /// 多次调用累加，由 builder 在 `build()` 阶段按 `name` 去重
    /// （先注入优先）。
    #[cfg(feature = "a2a")]
    pub fn add_a2as(mut self, configs: Vec<A2aServiceConfig>) -> Self {
        self.bundle.a2a_configs.extend(configs);
        self
    }

    /// 注入已连接的 A2A 注册表
    ///
    /// 调用方应在外部完成 `A2aRegistry::connect_all()` 后再注入。
    #[cfg(feature = "a2a")]
    pub fn with_a2a_registry(mut self, registry: A2aRegistry) -> Self {
        self.a2a_registry = registry;
        self
    }

    /// 设置技能白名单
    ///
    /// 仅白名单中列出的技能会被包含在最终输出中。
    #[cfg(feature = "skill")]
    pub fn allow_skills<I, S>(mut self, skill_codes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.skill_whitelist = Some(skill_codes.into_iter().map(Into::into).collect());
        self
    }

    /// 追加单个候选工具
    ///
    /// 同名工具会被覆盖。作为内置工具（`ToolSource::Builtin`）入池。
    pub fn add_tool(mut self, tool: impl Tool + 'static) -> Self {
        self.tool_pool.register_rig_tool(tool, ToolSource::Builtin);
        self
    }

    /// 追加多个候选工具
    pub fn add_tools(mut self, tools: Vec<DynamicTool>) -> Self {
        self.tool_pool.register_many(tools, ToolSource::Builtin);
        self
    }

    /// 设置工具白名单
    pub fn allow_tools<I, S>(mut self, tool_whitelist: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tool_whitelist = Some(tool_whitelist.into_iter().map(Into::into).collect());
        self
    }

    /// 设置子智能体委托模式
    #[cfg(feature = "sub_agent")]
    pub fn with_delegation_mode(mut self, mode: SubAgentDelegationMode) -> Self {
        self.delegation_mode = mode;
        self
    }

    // ─── Hook 便捷 API ──────────────────

    /// 注册生命周期钩子
    pub fn on_hook(mut self, hook: impl AgentHook + 'static) -> Self {
        self.hooks.register_hook(Arc::new(hook));
        self
    }

    /// 注册已包装为 `Arc` 的生命周期钩子
    pub fn on_hook_arc(mut self, hook: Arc<dyn AgentHook>) -> Self {
        self.hooks.register_hook(hook);
        self
    }
}

// ===== Build =====
impl AgentBuilder {
    /// 构建最终的 `Agent`
    ///
    /// ## 处理流程
    ///
    /// 1. 依次调用所有 source 的 `load()`，按顺序 `merge` 到 `self.bundle`
    ///    （`merge` 内部会自动触发 first-wins 去重 + 委派深度截断）
    /// 2. 对手动注入场景补一次 `bundle.normalize()`
    /// 3. 加载内置工具
    /// 4. 各 `load_*` 方法读取已规范化的 bundle，装配对应工具
    /// 5. 候选池冻结为 `Arc<ToolPool>`
    /// 6. 从 bundle 抽取 `AgentConfig` + 能力配置，构造 `LlmAgent`
    ///
    /// # Errors
    ///
    /// 当 source 加载失败或 MCP 服务连接失败时返回错误。
    pub async fn build(mut self) -> AgentResult<LlmAgent> {
        // 1) 依次加载所有 source 并合并到 bundle
        self.load_sources().await?;

        // 2) 对手动注入的能力字段补一次规范化（去重 + 深度截断）
        //    `merge` 内部已会触发 normalize，这里主要覆盖 `add_*` 便捷 API 场景
        self.bundle.normalize();

        // 3) 内置工具池预填
        self.load_builtin_tools().await?;
        self.build_extra_tools()?;

        // 4) 装配各来源工具到 ToolPool（commands/rules/skills/mcp/a2a）
        #[cfg(feature = "command")]
        self.load_commands().await?;
        #[cfg(feature = "rule")]
        self.load_rules().await?;
        #[cfg(feature = "skill")]
        self.load_skills().await?;
        #[cfg(feature = "mcp")]
        self.load_mcp().await?;
        #[cfg(feature = "a2a")]
        self.load_a2a().await?;

        // 5) 装配子 agents to ToolPool
        #[cfg(feature = "sub_agent")]
        self.load_sub_agents().await?;

        // 6) 候选池冻结成共享 Arc。
        let shared_pool = Arc::new(std::mem::take(&mut self.tool_pool));

        let agent_config = self.bundle.to_agent_config();
        let agent = LlmAgent::new(
            agent_config,
            self.model,
            self.services,
            Arc::new(self.hooks.clone()),
            self.bundle.rule_configs,
            self.bundle.skill_configs,
            self.bundle.command_configs,
            shared_pool,
        );

        Ok(agent)
    }

    /// 依次加载所有 source 并合并到 bundle
    ///
    /// `sources` 字段不被消费 —— 之后子 Agent 可以继承同一份 source 列表，
    /// 通过 [`Arc`] 引用计数复用 source 实例。
    async fn load_sources(&mut self) -> Result<(), Error> {
        for source in &self.sources {
            let fragment = source.load().await?;
            self.bundle.merge(fragment);
        }
        Ok(())
    }

    /// Load builtin tools from the tool pool.
    async fn load_builtin_tools(&mut self) -> Result<(), Error> {
        self.tool_pool
            .register_builtin(KnowledgeQueryTool::new(self.services.clone()));

        Ok(())
    }

    /// Load rules from the bundle.
    ///
    /// `bundle` 在 `merge` / `normalize` 阶段已完成去重，本方法仅负责
    /// 注册 `rule_list`（摘要内省）与 `rule_load`（按名称取正文）工具。
    #[cfg(feature = "rule")]
    async fn load_rules(&mut self) -> Result<(), Error> {
        if self.bundle.rule_configs.is_empty() {
            info!("No rules found, skipping rule injection");
            return Ok(());
        }

        // 注册规则内省工具（默认 Builtin 来源）
        self.tool_pool
            .register_builtin(RuleListTool::new(&self.bundle.rule_configs))
            .register_builtin(RuleLoadTool::new(&self.bundle.rule_configs));

        tracing::info!(
            rule_count = self.bundle.rule_configs.len(),
            "Registered rule_list and rule_load tools for rule introspection"
        );

        Ok(())
    }

    /// Load skills from the bundle and register skill tools.
    ///
    /// 装配顺序：
    /// 1. 若设置了 `skill_whitelist`，按白名单过滤
    /// 2. 注册 `skill_load` / `skill_list_docs` / `skill_select_docs` / `skill_exec` 工具
    ///
    /// `bundle.skill_configs` 在 `merge` / `normalize` 阶段已按 `skill_code` 去重（先注入者优先）。
    #[cfg(feature = "skill")]
    async fn load_skills(&mut self) -> Result<(), Error> {
        if let Some(whitelist) = self.skill_whitelist.take() {
            let whitelist_set: HashSet<String> = whitelist.into_iter().collect();
            self.bundle
                .skill_configs
                .retain(|s| whitelist_set.contains(&s.skill_code));
        }

        if self.bundle.skill_configs.is_empty() {
            info!("No skills found, skipping skill tools");
            return Ok(());
        }

        // Available skill codes shared across all four skill tools
        let available_skill_codes: Vec<String> = self
            .bundle
            .skill_configs
            .iter()
            .map(|s| s.skill_code.clone())
            .collect();
        let skills_root = LocalSkillLoader::default().skills_root().to_path_buf();

        self.tool_pool
            .register_skill(
                SkillLoadTool::new(&skills_root)
                    .with_available_skills(available_skill_codes.clone()),
            )
            .register_skill(
                SkillListDocsTool::new(&skills_root)
                    .with_available_skills(available_skill_codes.clone()),
            )
            .register_skill(
                SkillSelectDocsTool::new(&skills_root)
                    .with_available_skills(available_skill_codes.clone()),
            )
            .register_skill(
                SkillExecTool::new(&skills_root).with_available_skills(available_skill_codes),
            );

        tracing::info!(
            skill_count = self.bundle.skill_configs.len(),
            "Registered skill_load, skill_list_docs, skill_select_docs, skill_exec tools"
        );

        Ok(())
    }

    /// Build MCP tools from the `McpRegistry` into the candidate pool.
    ///
    /// 装配顺序：
    /// 1. 注入来自 `bundle.mcp_configs` 的配置（由各 source 合并产出）
    /// 2. 连接所有 pending 服务
    /// 3. 调用 [`McpRegistry::build_tools`] 拿到 boxed 工具列表，
    ///    通过 [`ToolPool::register_many`] 一次性入池（统一标记 `ToolSource::Mcp`）
    #[cfg(feature = "mcp")]
    async fn load_mcp(&mut self) -> Result<(), Error> {
        if !self.bundle.mcp_configs.is_empty() {
            self.mcp_registry = std::mem::take(&mut self.mcp_registry)
                .with_configs(self.bundle.mcp_configs.clone());
        }

        if self.mcp_registry.pending_count() > 0 {
            self.mcp_registry.connect_all().await?;
        }

        if self.mcp_registry.connection_count() == 0 {
            return Ok(());
        }

        let mcp_tools = self.mcp_registry.build_tools().await;
        self.tool_pool.register_many(mcp_tools, ToolSource::Mcp);

        Ok(())
    }

    /// Load A2A remote agent tools from the bundle into the candidate pool.
    ///
    /// 装配顺序：
    /// 1. 将 `bundle.a2a_configs` 注入 `a2a_registry`（pending 队列）
    /// 2. 调用 `A2aRegistry::connect_all()` 统一建立连接
    /// 3. 调用 [`A2aRegistry::build_tools`] 拿到工具列表，
    ///    逐个转成 [`DynamicTool`](rig_core::tool::DynamicTool) 后入池（标记 `ToolSource::A2a`）
    #[cfg(feature = "a2a")]
    async fn load_a2a(&mut self) -> Result<(), Error> {
        if !self.bundle.a2a_configs.is_empty() {
            self.a2a_registry = std::mem::take(&mut self.a2a_registry)
                .with_configs(self.bundle.a2a_configs.clone());
        }

        if self.a2a_registry.pending_count() > 0 {
            self.a2a_registry.connect_all().await?;
        }

        if self.a2a_registry.connection_count() == 0 {
            return Ok(());
        }

        let a2a_tools = self.a2a_registry.build_tools();
        let loaded = a2a_tools.len();
        for tool in a2a_tools {
            self.tool_pool
                .register_dynamic(tool.into_dynamic_tool(), ToolSource::A2a);
        }
        tracing::info!(
            a2a_count = loaded,
            "Registered A2A agent tools for remote delegation"
        );

        Ok(())
    }

    /// Load commands from the bundle.
    ///
    /// `bundle` 在 `merge` / `normalize` 阶段已完成去重，本方法仅负责
    /// 注册 `command_load` / `command_list` 工具供 LLM 按需加载命令。
    #[cfg(feature = "command")]
    async fn load_commands(&mut self) -> Result<(), Error> {
        if self.bundle.command_configs.is_empty() {
            info!("No commands found, skipping command injection");
            return Ok(());
        }

        // 命令内省工具按 Builtin 来源标记（实际执行 command 由 LLM 通过工具调用驱动）
        self.tool_pool
            .register_command(CommandLoadTool::new(&self.bundle.command_configs))
            .register_command(CommandListTool::new(&self.bundle.command_configs));

        tracing::info!(
            command_count = self.bundle.command_configs.len(),
            "Registered command_load and command_list tools for on-demand command loading"
        );

        Ok(())
    }

    /// Load sub-agents from the bundle and register delegation tools.
    ///
    /// `bundle.sub_agent_bundles` 在 `merge` / `normalize` 阶段已完成去重与委派深度
    /// 截断，本方法仅负责根据委托模式注册对应工具供 LLM 触发委派。
    #[cfg(feature = "sub_agent")]
    async fn load_sub_agents(&mut self) -> Result<(), Error> {
        if self.bundle.sub_agent_bundles.is_empty() {
            info!("No sub-agents found, skipping sub-agent injection");
            return Ok(());
        }

        match self.delegation_mode {
            SubAgentDelegationMode::Flat => {
                // Flat mode: each sub-agent gets its own independent tool
                // (`sub_agent_{name}`).
                let flat_tools = FlatSubAgentTool::from_bundles(
                    &self.bundle.sub_agent_bundles,
                    self.global_tool_pool.clone(),
                    self.services.clone(),
                );

                for tool in flat_tools {
                    self.tool_pool
                        .register_dynamic(tool.into_dynamic_tool(), ToolSource::SubAgent);
                }

                tracing::info!(
                    sub_agent_count = self.bundle.sub_agent_bundles.len(),
                    mode = "flat",
                    "Registered flat sub-agent tools for sub-agent delegation"
                );
            }
            SubAgentDelegationMode::SingleTool => {
                // Single-tool mode: all sub-agents behind one
                // `delegate_sub_agent` tool.
                let delegate_tool = DelegateSubAgentTool::new(
                    &self.bundle.sub_agent_bundles,
                    self.global_tool_pool.clone(),
                    self.services.clone(),
                );

                self.tool_pool.register_sub_agent(delegate_tool);

                tracing::info!(
                    sub_agent_count = self.bundle.sub_agent_bundles.len(),
                    mode = "single_tool",
                    "Registered delegate_sub_agent tool for sub-agent delegation"
                );
            }
        }

        Ok(())
    }

    /// Load extra tools from the tool registry and register them.
    /// If a whitelist is provided, only the tools in the whitelist are registered.
    fn build_extra_tools(&mut self) -> Result<(), Error> {
        match self.tool_whitelist.take() {
            Some(whitelist) => {
                self.tool_pool.retag(&whitelist, ToolSource::Builtin);
            }
            None => {
                self.tool_pool.retag_all(ToolSource::Builtin);
            }
        }

        tracing::info!(
            extra_tools_count = self.tool_pool.len(),
            "Registered extra tools for sub-agent delegation"
        );

        Ok(())
    }
}
