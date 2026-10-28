//! LLM 推理智能体（默认实现）
//!
//! 提供的主要内容：
//! - [`LlmAgent`] - 实现 ReAct 循环的核心智能体
//!
//! ## 执行流程
//!
//! ```text
//! 用户输入
//!   └─► before_agent 钩子
//!         └─► 构建初始消息（系统 Prompt + 手动历史上下文 + 用户输入）
//!               └─► [循环] ModelCaller::call(…) -> ModelTurn
//!                     ├─ 含 tool_calls           → 下一轮
//!                     └─ 仅 text（无 tool_calls） → finalize_answer + 退出
//!                     └─► after_agent 钩子
//! ```
//!
//! ## 手动历史上下文
//!
//! 调用方可通过 [`LlmAgent::with_history_messages`] 或
//! [`LlmAgent::push_history_user`] / [`LlmAgent::push_history_assistant`]
//! 注入一段已发生的对话。这些消息会在系统提示词之后、
//! 当前用户输入之前合并到上下文中，用于满足"基于既有对话片段继续推理"
//! 等场景（多轮对话回放、外部上下文桥接等）。
//!
//! ## 数据流向
//!
//! 事件流是唯一事实源：
//!
//! - [`Agent::run_stream`] —— 启动一次运行，直接返回 [`AgentEventStream`]
//! - [`Agent::run`] —— 内部调用 `run_stream`，并通过
//!   [`RunOutcome::from_stream`] 把事件流下采样为结构化结果
//!
//! 因此底层只有一个数据出口（事件流），结构化的 [`RunOutcome`] 是流的视图，
//! 二者天然一致，不存在对账问题。
//!
//! ## 可配置点
//!
//! - `max_parallel_tools` — 工具调用的最大并发数（1 即串行，默认 8）
//! - [`HookRegistry`] — Agent / Model / Tool 三层共 6 个钩子点

use std::sync::Arc;

use rig_core::completion::Usage;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    MessageContext, ModelTurn,
    constant::DEFAULT_EVENT_CHANNEL_CAPACITY,
    error::{AgentResult, Error},
    hook::HookRegistry,
    message::{ConversationMessage, DistilledMemory, MEMORY_RECALL_TOOL_NAME},
    model::ModelAdapter,
    prompt::SystemPrompt,
    runtime::{
        RuntimeServices,
        agent::traits::{Agent, AgentOpenAI},
        llm::{ModelCaller, ToolExecutor},
    },
    session::{AgentEventStream, AgentStreamEvent, FinishReason},
    tool::ToolPool,
    types::{
        agent_meta::AgentMeta,
        command::CommandConfig,
        hook::{AfterAgentArgs, BeforeAgentArgs},
        rule::RuleConfig,
        runtime::{AgentConfig, LoopResult, RunConfig},
        session::{ContextScope, SessionStatus},
        skill::SkillConfig,
        tool_call_retry::ToolCallRetryConfig,
    },
};
/// LLM 推理智能体
///
/// `agent-runtime` 的核心默认实现，承载 ReAct 循环：推理 → 工具 → 推理 → ... → 最终回答。
/// 通过组合 [`HookRegistry`] 与 `max_parallel_tools` 实现生命周期钩子、
/// 并发限流。
///
/// ## 事件流模型
///
/// `LlmAgent` 自身不持有任何事件发送端。每次 [`Agent::run_stream`] 都会即时
/// 创建一对 `mpsc::channel`：
///
/// - 写端 `Sender` 注入到内部新构建的 [`ModelCaller`] / [`ToolExecutor`]，
///   用于推送运行进展事件
/// - 读端 `Receiver` 包装为 [`AgentEventStream`] 返回给调用方
///
/// 子智能体事件透传由 `DelegateSubAgentTool` 在工具执行点消费子 stream 并
/// 包装为 [`AgentStreamEvent::SubAgentEvent`] 写入父 sender 完成。父 sender / `run_id`
/// 通过每次调用装配的 [`ToolContext`](rig_core::tool::ToolContext) 下发
/// （身份走 inbound、事件通道走 scope），无需任何共享可变态。
#[derive(Clone)]
pub struct LlmAgent {
    /// 智能体名称（用于日志、事件标识、组合场景）
    name: String,
    /// 智能体运行时配置
    agent_config: AgentConfig,
    /// LLM 模型适配器
    model: Arc<dyn ModelAdapter>,
    /// ReAct 循环运行时参数
    run_config: RunConfig,
    /// 规则配置（仅用于构建 system prompt）
    rule_configs: Vec<RuleConfig>,
    /// 技能配置（仅用于构建 system prompt）
    skill_configs: Vec<SkillConfig>,
    /// 命令配置（仅用于构建 system prompt）
    command_configs: Vec<CommandConfig>,
    /// 钩子注册表
    hooks: Arc<HookRegistry>,
    /// 运行时服务聚合（包含记忆、知识、会话等）
    services: RuntimeServices,
    /// 动态覆盖的系统提示词
    ///
    /// 通过 [`LlmAgent::with_system_prompt`] 设置后，
    /// [`LlmAgent::build_system_prompt`] 会优先使用此值作为 base prompt，
    /// 而非 `AgentConfig.agent_definition.system_prompt`。
    /// 设置为 `None` 则恢复使用配置中的默认值。
    system_prompt_override: Option<String>,
    /// 用户手动注入的历史对话上下文
    ///
    /// 在 [`LlmAgent::build_initial_messages`] 中，会按原顺序合并到系统提示
    /// 与自动召回记忆之后、当前用户输入之前。默认空列表。
    history_messages: Vec<ConversationMessage>,
    /// 候选工具池
    ///
    /// 运行时供 [`ToolExecutor`] / [`ModelCaller`] 查表，在发射
    /// `ToolCallStarted` / `ToolCallFinished` / `ToolCallsPlanned` 事件时
    /// 填充 `source` 字段。在 builder 装配阶段冻结后，运行时只读。
    tool_pool: Arc<ToolPool>,
    /// 智能体运行元数据（来源标识 + 父级关联信息）
    ///
    /// 默认为 [`AgentMeta::primary()`]（主智能体）。
    /// 工具层构建子智能体时通过 [`Self::with_meta`] 注入父级信息，
    /// `run_stream_inner` / `infer_stream_inner` 根据此字段决定是否执行
    /// session 生命周期管理。
    meta: AgentMeta,
}

impl LlmAgent {
    /// 由 [`AgentBuilder`] 直接构造 `LlmAgent`
    ///
    /// 装配期确定的字段（rules / skills / commands 等）整体作为不可变快照
    /// 传入；运行时可调字段（`max_turns` / `max_parallel_tools` 等）保持默认值，
    /// 由后续链式 setter 覆盖。
    ///
    /// 仅供 `agent-runtime` crate 内部的 [`AgentBuilder`] 调用，
    /// 外部使用方应统一通过 [`AgentBuilder`] 装配 Agent。
    ///
    /// # Parameters
    ///
    /// * `agent_config` - 智能体运行时配置
    /// * `model` - LLM 模型适配器
    /// * `services` - 运行时服务聚合
    /// * `hooks` - 钩子注册表
    /// * `rule_configs` - 规则配置列表（仅用于构建 system prompt）
    /// * `skill_configs` - 技能配置列表（仅用于构建 system prompt）
    /// * `command_configs` - 命令配置列表（仅用于构建 system prompt）
    /// * `tool_pool` - 候选工具池（builder 装配阶段冻结的 `Arc<ToolPool>`）
    ///
    /// [`AgentBuilder`]: crate::runtime::agent::AgentBuilder
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        agent_config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        services: RuntimeServices,
        hooks: Arc<HookRegistry>,
        rule_configs: Vec<RuleConfig>,
        skill_configs: Vec<SkillConfig>,
        command_configs: Vec<CommandConfig>,
        tool_pool: Arc<ToolPool>,
    ) -> Self {
        let name = agent_config.agent_definition.name.clone();

        Self {
            name,
            agent_config,
            model,
            run_config: RunConfig::default(),
            rule_configs,
            skill_configs,
            command_configs,
            hooks,
            services,
            system_prompt_override: None,
            history_messages: Vec::new(),
            tool_pool,
            meta: AgentMeta::primary(),
        }
    }

    /// 设置最大推理轮次（运行时可调链式 setter）
    ///
    /// # Parameters
    ///
    /// * `max_turns` - 最大推理轮次（必须大于 0，传入 0 会被规整为 1）
    pub fn with_max_turns(mut self, max_turns: u32) -> Self {
        self.run_config.max_turns = max_turns.max(1);
        self
    }

    /// 设置工具调用的最大并发数（链式 setter）
    ///
    /// 小于 1 会被视作 1（严格串行）以避免死锁。
    pub fn with_max_parallel_tools(mut self, n: usize) -> Self {
        self.run_config.max_parallel_tools = n.max(1);
        self
    }

    /// 将工具调用设为严格串行模式（等价于 `with_max_parallel_tools(1)`）
    ///
    /// 适用于工具之间存在顺序依赖、或需要确保执行顺序确定性的场景。
    pub fn with_sequential_tools(mut self) -> Self {
        self.run_config.max_parallel_tools = 1;
        self
    }

    /// 配置单次工具调用的重试策略
    pub fn with_tool_call_retry(mut self, cfg: ToolCallRetryConfig) -> Self {
        self.run_config.retry = cfg;
        self
    }

    /// 启用或禁用工具调用参数的 JSON 容错修复
    pub fn with_tool_call_arguments_json_repair(mut self, enabled: bool) -> Self {
        self.run_config.arguments_json_repair = enabled;
        self
    }

    /// 设置取消状态轮询间隔（链式 setter）
    ///
    /// 流式推理中定时查询数据库检测取消信号的间隔。
    ///
    /// # Parameters
    ///
    /// * `interval` - 轮询间隔
    pub fn with_cancel_poll_interval(mut self, interval: std::time::Duration) -> Self {
        self.run_config.cancel_poll_interval = interval;
        self
    }

    /// 动态覆盖系统提示词（运行时可调链式 setter）
    ///
    /// 设置后会覆盖 `AgentConfig.agent_definition.system_prompt` 中的 base prompt，
    /// 在 [`Self::build_system_prompt`] 时使用此值。Rules / Skills / Commands
    /// 仍然会正常拼接到 base prompt 之后。
    ///
    /// 传入空字符串等价于清除系统提示词的 base 部分（Rules 等仍保留）。
    ///
    /// # Parameters
    ///
    /// * `prompt` - 新的系统提示词内容
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agent_runtime::LlmAgent;
    ///
    /// // AgentBuilder::build() 返回 LlmAgent，可链式调用 with_system_prompt
    /// fn configure(agent: LlmAgent) -> LlmAgent {
    ///     agent.with_system_prompt("You are a coding assistant.")
    /// }
    /// ```
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt_override = Some(prompt.into());
        self
    }

    /// 清除动态覆盖的系统提示词，恢复使用配置中的默认值
    pub fn clear_system_prompt_override(mut self) -> Self {
        self.system_prompt_override = None;
        self
    }

    /// 设置用户手动注入的历史对话上下文（链式 setter，整体替换）
    ///
    /// 传入的消息会按原顺序合并到 system prompt 与自动召回记忆之后、
    /// 当前用户输入之前。再次调用会**覆盖**先前的设置。
    ///
    /// 通常调用方不应在此传入 [`MessageRole::System`] 消息：系统提示已由
    /// [`SystemPrompt`] 单独装配。如确有需要，调用方需自行确保语义合理。
    ///
    /// # Parameters
    ///
    /// * `messages` - 已发生的对话消息列表（user/assistant 交替为典型用法）
    ///
    /// [`MessageRole::System`]: crate::message::MessageRole::System
    pub fn with_history_messages(mut self, messages: Vec<ConversationMessage>) -> Self {
        self.history_messages = messages;
        self
    }

    /// 清空已注入的手动历史对话上下文
    pub fn clear_history_messages(mut self) -> Self {
        self.history_messages.clear();
        self
    }

    /// 设置智能体运行元数据（链式 setter）
    ///
    /// 工具层构建子智能体时调用，注入父级 run_id / agent_id / session_id。
    /// 子智能体设置此字段后，`run_stream_inner` 将跳过 session 生命周期管理，
    /// 避免覆盖父智能体的 `Running` 状态。
    ///
    /// # Parameters
    ///
    /// * `meta` - 智能体运行元数据
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agent_runtime::{LlmAgent, types::agent_meta::AgentMeta};
    ///
    /// fn configure_sub_agent(agent: LlmAgent, parent_run_id: String) -> LlmAgent {
    ///     agent.with_meta(AgentMeta::sub_agent(parent_run_id, 1, "sess-1".into()))
    /// }
    /// ```
    pub fn with_meta(mut self, meta: AgentMeta) -> Self {
        self.meta = meta;
        self
    }

    /// 智能体名称
    pub fn agent_name(&self) -> &str {
        &self.name
    }

    /// 当前最大推理轮次
    pub fn max_turns(&self) -> u32 {
        self.run_config.max_turns
    }

    /// 获取运行时配置的只读引用
    pub fn run_config(&self) -> &RunConfig {
        &self.run_config
    }
}

impl LlmAgent {
    /// 构建初始消息列表
    ///
    /// 拼接顺序（差异化处理三粒度上下文）：
    ///
    /// 1. **系统 Prompt**（base prompt + rules/skills/commands）
    /// 2. **长期记忆注入**：把 [`ContextScope::Agent`] 与 [`ContextScope::User`]
    ///    两个粒度的蒸馏记忆以 `tool` 角色消息形式注入（虚拟工具名
    ///    `memory_recall`），避免 system 段落膨胀。注入前会插入一条简短的
    ///    system 提示告知 LLM 接下来的 tool 结果是长期记忆召回。
    /// 3. **手动历史对话上下文**（[`Self::with_history_messages`] 注入）
    /// 4. **会话历史回放**：把 [`ContextScope::Session`] 中的全部上下文按角色
    ///    （user / assistant / tool）原样回放，保证 ReAct 工具调用链的连续性。
    /// 5. **当前用户输入**
    ///
    /// 写入语义：在读完会话历史 *之后*、push 当前输入 *之前*，把当前输入以
    /// `ConversationMessage::user` 的形式追加进 `Session` 上下文存储。这样既保证
    /// 回放历史不包含当前输入（避免重复），也让"写一次"的动作内聚在本方法里。
    pub(crate) async fn build_initial_messages(
        &self,
        session_id: &str,
        input: &str,
        agent_id: i64,
        user_id: &str,
    ) -> AgentResult<MessageContext> {
        let mut messages = MessageContext::new();

        // [1] 系统 Prompt
        let full_system_prompt = self.build_system_prompt();
        if !full_system_prompt.is_empty() {
            messages.push_system(full_system_prompt);
        }

        // 一次性拉取三粒度上下文（容错 + 并发）
        let scoped = self
            .services
            .list_all_contexts(session_id, agent_id, user_id)
            .await;

        // [2] 长期记忆注入（Agent + User → 各自聚合为单条 tool 角色消息）
        Self::inject_long_term_memory(&mut messages, &scoped.agent, &scoped.user);

        // [3] 手动注入的历史对话
        for msg in &self.history_messages {
            messages.push(msg.clone());
        }

        // [4] 会话级上下文整体回放
        for msg in scoped.session {
            messages.push(msg);
        }

        // 把当前用户输入写入 Session 上下文（容错），随后再加入本轮 messages
        self.services
            .try_append_context(
                ContextScope::Session {
                    session_id,
                    agent_id,
                    user_id,
                },
                &ConversationMessage::user(input),
            )
            .await;

        // [5] 当前用户输入
        messages.push_user(input);
        Ok(messages)
    }

    /// 构建完整 system prompt（base prompt + Rules + Skills + Commands）
    ///
    /// 优先使用 [`Self::with_system_prompt`] 设置的动态覆盖值作为 base prompt，
    /// 若未设置则回退到 `AgentConfig.agent_definition.system_prompt`。
    pub(crate) fn build_system_prompt(&self) -> String {
        let base = self
            .system_prompt_override
            .as_deref()
            .unwrap_or(&self.agent_config.agent_definition.system_prompt);

        // 在基础提示词后添加明确指导，告诉 LLM 在完成任务后应该停止调用工具
        let enhanced_base = if base.is_empty() {
            base.to_string()
        } else {
            format!(
                "{}\n\n## 重要指导\n当你完成所有必要的工具调用并收集到足够信息后，请返回一个完整的、面向用户的纯文本答案，不要继续调用工具。如果你认为需要更多信息，可以继续调用工具，但一旦你有了足够信息来回答用户的问题，就请停止调用工具并返回最终答案。",
                base
            )
        };

        SystemPrompt::new()
            .with_base(enhanced_base)
            .with_rules(self.rule_configs.clone())
            .with_skills(self.skill_configs.clone())
            .with_commands(self.command_configs.clone())
            .build()
    }

    /// 向 events 通道发送一个事件
    async fn emit(
        &self,
        events: &mpsc::Sender<AgentStreamEvent>,
        event: AgentStreamEvent,
    ) -> AgentResult<()> {
        events
            .send(event)
            .await
            .map_err(|e| Error::Streaming(e.to_string()))
    }

    /// 容错落库 + 推流的双写组合
    ///
    /// 仅适用于「必须双写」的低频生命周期事件
    /// （`RunStarted` / `RunFinished` / `Error` 等）：
    /// 先调用 [`RuntimeServices::try_append_event`] 容错落库，再通过
    /// [`Self::emit`] 推送到事件流。
    ///
    /// 高频流式事件（`TextDelta` / `ReasoningDelta` / `StepStarted` 等）
    /// 不应使用本方法，仍走 [`Self::emit`] 单纯推流，避免事件表写放大。
    ///
    /// # Parameters
    ///
    /// * `events` - 本次运行的事件流写端
    /// * `session_id` - 会话 ID（落库目标）
    /// * `event` - 待双写的事件
    ///
    /// 确保 session 处于可运行状态（仅主智能体调用）。
    ///
    /// 子智能体复用父智能体的 session，不重复管理状态，
    /// 避免覆盖父智能体的 `Running` 状态，因此此方法仅在
    /// [`AgentSource::Primary`] 时执行。
    ///
    /// 处理逻辑：
    /// - session 不存在 → `create_session`
    /// - session 已存在且为 Running → 返回错误（拒绝重复启动）
    /// - session 已存在 → `update_session(Running)`
    ///
    /// # Returns
    ///
    /// `Ok(())` 表示 session 已就绪；`Err` 表示初始化失败，调用方应终止本次运行。
    async fn ensure_session_running(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> Result<(), Error> {
        if !self.meta.agent_source.is_primary() {
            return Ok(());
        }

        match self
            .services
            .get_session(session_id, agent_id, user_id)
            .await
        {
            Ok(Some(status)) => {
                if status.is_running() {
                    Err(Error::Internal(
                        "该会话已有活跃运行，请等待当前运行结束或取消后再试".to_string(),
                    ))
                } else {
                    self.services
                        .update_session(session_id, agent_id, user_id, Some(SessionStatus::Running))
                        .await
                }
            }
            Ok(None) => {
                self.services
                    .create_session(session_id, agent_id, user_id)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    async fn emit_persisted(
        &self,
        events: &mpsc::Sender<AgentStreamEvent>,
        session_id: &str,
        event: AgentStreamEvent,
    ) -> AgentResult<()> {
        self.services
            .try_append_event(
                session_id,
                self.agent_config.agent_definition.id,
                &self.agent_config.agent_definition.user_id,
                &event,
            )
            .await;
        self.emit(events, event).await
    }

    /// 基于本次运行的 events 通道构建 ModelCaller
    fn build_model_caller(
        &self,
        events: mpsc::Sender<AgentStreamEvent>,
        run_config: RunConfig,
    ) -> Arc<ModelCaller> {
        let tool_executor = Arc::new(ToolExecutor::new(
            events.clone(),
            self.services.clone(),
            Arc::clone(&self.hooks),
            run_config.clone(),
            self.agent_config.agent_definition.id,
            self.agent_config.agent_definition.user_id.clone(),
            Arc::clone(&self.tool_pool),
        ));

        Arc::new(ModelCaller::new(
            Arc::clone(&self.model),
            self.agent_config.clone(),
            events,
            Arc::clone(&self.hooks),
            tool_executor,
            self.services.clone(),
            Arc::clone(&self.tool_pool),
            run_config,
        ))
    }

    /// 处理 ReAct 循环结果
    ///
    /// 根据 [`LoopResult`] 的 `finish_reason` 执行不同的后处理逻辑：
    ///
    /// - `MaxTurns`：返回空字符串和对应的错误信息
    /// - `FinalizeAction`：按需追加总结轮，调用 `finalize_answer` 进行持久化
    /// - 其他：返回未知终止原因的降级结果
    ///
    /// 返回的五元组可直接用于构造 `RunFinished` 事件。
    #[allow(clippy::too_many_arguments)]
    async fn process_loop_result(
        &self,
        events: &mpsc::Sender<AgentStreamEvent>,
        model_caller: &Arc<ModelCaller>,
        run_id: &str,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        max_turns: u32,
        loop_result: LoopResult,
    ) -> (String, FinishReason, u32, Option<String>, Option<Usage>) {
        match loop_result.finish_reason {
            FinishReason::MaxTurns => (
                String::new(),
                FinishReason::MaxTurns,
                max_turns,
                Some(format!("已达到最大推理轮次 {}", max_turns)),
                loop_result.accumulated_usage,
            ),
            FinishReason::FinalizeAction => {
                let (mut step, mut accumulated_usage, mut messages) = (
                    loop_result.step,
                    loop_result.accumulated_usage,
                    loop_result.messages,
                );

                let last_turn = match loop_result.last_turn {
                    Some(turn) => turn,
                    None => {
                        return (
                            String::new(),
                            FinishReason::Error,
                            step,
                            Some(
                                "Agent finished with FinalizeAction but last_turn is missing"
                                    .to_string(),
                            ),
                            loop_result.accumulated_usage,
                        );
                    }
                };

                let answer = if step > 1 && last_turn.text.is_none() {
                    // 最后一轮是纯工具调用（无 TextDelta），需要总结轮来生成面向用户的文本
                    // 但是似乎在循环中当存在工具调用时会继续进行循环
                    step += 1;
                    self.summarize_final_answer(
                        events,
                        model_caller,
                        run_id,
                        session_id,
                        step,
                        &mut messages,
                        &mut accumulated_usage,
                        &last_turn,
                    )
                    .await
                } else {
                    last_turn.text.unwrap_or_default()
                };

                if let Err(e) = self
                    .finalize_answer(events, run_id, step, &answer, session_id, agent_id, user_id)
                    .await
                {
                    warn!(run_id = %run_id, step = step, error = %e, "finalize_answer 失败");
                }

                (
                    answer,
                    FinishReason::FinalizeAction,
                    step,
                    None,
                    accumulated_usage,
                )
            }
            _ => (
                String::new(),
                loop_result.finish_reason,
                loop_result.step,
                Some("未知的循环终止原因".to_string()),
                loop_result.accumulated_usage,
            ),
        }
    }

    /// 追加最终总结轮
    ///
    /// 但是似乎在循环中当存在工具调用时会继续进行循环，该汇总不是必须的
    ///
    ///
    /// 总结轮会：
    /// - 追加 system 提示引导 LLM 进行总结
    /// - 发射 `StepStarted` 事件
    /// - 调用 `model_caller.call` 进行单轮推理（不执行工具）
    /// - 累加本次调用的 token 用量
    ///
    /// 如果总结调用失败，降级为返回 `last_turn` 的文本内容。
    #[allow(clippy::too_many_arguments)]
    async fn summarize_final_answer(
        &self,
        events: &mpsc::Sender<AgentStreamEvent>,
        model_caller: &Arc<ModelCaller>,
        run_id: &str,
        session_id: &str,
        step: u32,
        messages: &mut MessageContext,
        accumulated_usage: &mut Option<Usage>,
        last_turn: &ModelTurn,
    ) -> String {
        messages.push_user(
            "你已经完成了所有必要的工具调用。请基于上述全部信息，\
            用清晰、结构化的方式给出一个完整的最终回答。"
                .to_string(),
        );

        let _ = self
            .emit_persisted(
                events,
                session_id,
                AgentStreamEvent::StepStarted {
                    run_id: run_id.to_string(),
                    step,
                },
            )
            .await;

        info!(run_id = %run_id, step = step, "开始最终总结");

        let toolset = ToolPool::new();

        let summary_turn = match model_caller
            .call(run_id, step, messages, &toolset, session_id)
            .await
        {
            Ok(turn) => turn,
            Err(err) => {
                warn!(run_id = %run_id, error = %err, "追加总结轮失败");
                last_turn.clone()
            }
        };

        if let Some(u2) = &summary_turn.usage {
            *accumulated_usage = Some(Self::accumulate_usage(*accumulated_usage, u2));
        }

        summary_turn.text.unwrap_or_default()
    }

    /// 终止本轮运行：发布整段答案镜像事件、落库会话事件、写入助手记忆
    ///
    /// 注意：逐 token 的回答已在流式聚合阶段通过 [`AgentStreamEvent::TextDelta`]
    /// 推送给客户端。本方法仅负责**整段最终回答**的镜像投递与持久化，
    /// 客户端可据此完成会话落库、`finish_reason: stop` 映射等终止动作。
    #[allow(clippy::too_many_arguments)]
    async fn finalize_answer(
        &self,
        events: &mpsc::Sender<AgentStreamEvent>,
        run_id: &str,
        step: u32,
        answer: &str,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> AgentResult<()> {
        info!(run_id = %run_id, step = step, "Agent 产出最终回答");

        // 整段最终回答一次性发布：容错落库 + 推流作为终止标志
        let answer_event = AgentStreamEvent::Answer {
            run_id: run_id.to_string(),
            content: answer.to_string(),
        };
        self.emit_persisted(events, session_id, answer_event)
            .await?;

        // 同步写入助手回答上下文（落入会话级上下文存储）
        let message = ConversationMessage::assistant(answer.to_string());
        self.services
            .try_append_context(
                ContextScope::Session {
                    session_id,
                    agent_id,
                    user_id,
                },
                &message,
            )
            .await;

        Ok(())
    }

    // ── Agent 级钩子辅助方法 ────────────────────────

    /// 触发 `before_agent` 钩子链
    ///
    /// 钩子仅用于审计 / 指标 / 链路追踪，不能拦截运行。如需输入校验 / 鉴权 /
    /// 限流，请在调用 [`Agent::run`] 之前由外层服务自行短路。
    async fn hook_before_agent(
        &self,
        session_id: &str,
        user_id: &str,
        input: &str,
    ) -> AgentResult<()> {
        self.hooks
            .fire_before_agent(BeforeAgentArgs {
                session_id: session_id.to_string(),
                user_id: user_id.to_string(),
                input: input.to_string(),
            })
            .await?;
        Ok(())
    }

    /// 触发 `after_agent` 钩子链
    ///
    /// 钩子仅用于审计 / 指标 / 链路追踪，不能覆盖最终回答。如需内容过滤 /
    /// 敏感词替换 / 格式标准化，请由调用方拿到 [`RunOutcome`] 后自行后处理。
    async fn hook_after_agent(
        &self,
        session_id: &str,
        final_answer: &str,
        total_steps: u32,
    ) -> AgentResult<()> {
        self.hooks
            .fire_after_agent(AfterAgentArgs {
                session_id: session_id.to_string(),
                final_answer: if final_answer.is_empty() {
                    None
                } else {
                    Some(final_answer.to_string())
                },
                error: None,
                total_steps,
            })
            .await?;
        Ok(())
    }

    /// 累加两步 Usage
    ///
    /// 将 `step_usage` 累加到 `current` 上。如果 `current` 为 `None`，
    /// 则直接返回 `step_usage` 的克隆。
    fn accumulate_usage(current: Option<Usage>, step_usage: &Usage) -> Usage {
        // rig-core 0.45 的 Usage 字段均为 Option（provider 未上报时为空），
        // 累加时把「双方都为空」保持为 None，其余按 0 参与求和。
        fn add(left: Option<u64>, right: Option<u64>) -> Option<u64> {
            match (left, right) {
                (None, None) => None,
                (left, right) => Some(left.unwrap_or(0) + right.unwrap_or(0)),
            }
        }

        let mut acc = current.unwrap_or_default();
        acc.input_tokens = add(acc.input_tokens, step_usage.input_tokens);
        acc.output_tokens = add(acc.output_tokens, step_usage.output_tokens);
        acc.total_tokens = add(acc.total_tokens, step_usage.total_tokens);
        acc.cached_input_tokens = add(acc.cached_input_tokens, step_usage.cached_input_tokens);
        acc.cache_creation_input_tokens = add(
            acc.cache_creation_input_tokens,
            step_usage.cache_creation_input_tokens,
        );
        acc
    }

    /// 将 Agent 与 User 两个粒度的蒸馏记忆以 tool 消息形式注入上下文
    ///
    /// 注入策略（tool-based memory injection）：
    ///
    /// 每个粒度的 [`DistilledMemory`] 列表被聚合为**单条** tool 角色消息
    /// （包含引导标题 + 所有记忆条目的 bullet list），以 `memory_recall`
    /// 虚拟工具名包装后 push 到上下文中。
    ///
    /// 这种方式的优势：
    /// - 避免 system 段落膨胀（system 应只包含指令和规则）
    /// - 利用 LLM 对 tool 结果的天然理解能力
    /// - 同粒度记忆聚合在一条消息中，语义紧凑、上下文不分散
    /// - 引导词与对应记忆紧密关联，语义更清晰
    ///
    /// 如果 Agent 和 User 粒度均为空，则不注入任何内容。
    ///
    /// # Parameters
    ///
    /// * `messages` - 待注入的消息上下文
    /// * `agent_memories` - Agent 粒度蒸馏记忆条目列表
    /// * `user_memories` - User 粒度蒸馏记忆条目列表
    fn inject_long_term_memory(
        messages: &mut MessageContext,
        agent_memories: &[DistilledMemory],
        user_memories: &[DistilledMemory],
    ) {
        if let Some(msg) =
            Self::aggregate_memories(agent_memories, "agent", "## 长期记忆 — Agent 知识(跨会话)")
        {
            messages.push(msg);
        }

        if let Some(msg) =
            Self::aggregate_memories(user_memories, "user", "## 长期记忆 — 用户偏好(跨 Agent)")
        {
            messages.push(msg);
        }
    }

    /// 将蒸馏记忆列表聚合为单条 tool 角色消息
    ///
    /// 所有条目以 bullet list 格式拼接到引导标题之后，生成一条
    /// `tool` 角色的 [`ConversationMessage`]（虚拟工具名 `memory_recall`）。
    /// 输入为空时返回 `None`。
    ///
    /// # Parameters
    ///
    /// * `memories` - 蒸馏记忆条目列表
    /// * `scope_label` - 作用域标签（`"agent"` / `"user"`），用于生成 call_id
    /// * `guide_title` - 引导标题（如 "## 长期记忆 — Agent 知识(跨会话)"）
    fn aggregate_memories(
        memories: &[DistilledMemory],
        scope_label: &str,
        guide_title: &str,
    ) -> Option<ConversationMessage> {
        if memories.is_empty() {
            return None;
        }

        let mut buf = String::from(guide_title);
        buf.push('\n');

        for memory in memories {
            buf.push_str("- ");
            buf.push_str(&memory.content);
            buf.push('\n');
        }

        let call_id = format!("{scope_label}_memory");
        Some(ConversationMessage::tool_result(
            MEMORY_RECALL_TOOL_NAME,
            call_id,
            buf.trim_end(),
        ))
    }
}

impl LlmAgent {
    /// 核心执行循环（事件流唯一事实源）
    ///
    /// 完整运行逻辑：初始设置 → before_agent 钩子 → 消息构建 → ModelCaller 循环 → after_agent 钩子。
    /// 所有运行进展都通过 `events` 通道发布为 [`AgentStreamEvent`]，
    /// 不再独立返回结构化结果 —— 调用方可消费事件流自行聚合，或通过
    /// [`Agent::run`] 拿到由 [`RunOutcome::from_stream`] 聚合后的视图。
    ///
    /// 错误处理：当出现不可恢复错误时，仅发布
    /// `RunFinished { success: false, finish_reason: Error, error_message: Some(..) }`
    /// 作为唯一终止信号，不再额外发布 [`AgentStreamEvent::Error`]，避免与
    /// `RunFinished.error_message` 重复。流中早期告警类 `Error` 仅由
    /// [`ModelCaller`] 在更细粒度的失败点（如 `model_error`）发布。
    async fn run_stream_inner(
        &self,
        events: mpsc::Sender<AgentStreamEvent>,
        session_id: &str,
        user_id: &str,
        input: &str,
    ) {
        let run_id = Uuid::new_v4().to_string();

        let agent_id = self.agent_config.agent_definition.id;
        let agent_name = self.agent_config.agent_definition.name.clone();
        let max_turns = self.run_config.max_turns;
        let start_time = std::time::Instant::now();

        info!(run_id = %run_id, session_id = session_id, agent_id = agent_id, user_id = user_id, "Agent 运行开始");

        // ── 会话主表保障（仅主智能体执行） ──
        //
        // 后续事件 / 上下文 / 工具审计写入与查询都强依赖此主表存在，
        // 因此一旦失败必须直接终止本次运行，避免后续环节产生级联异常。
        // 子智能体复用父智能体的 session，直接跳过。
        if let Err(err) = self
            .ensure_session_running(session_id, agent_id, user_id)
            .await
        {
            warn!(run_id = %run_id, session_id = %session_id, error = %err, "会话主表初始化失败，终止运行");
            let _ = self
                .emit_persisted(
                    &events,
                    session_id,
                    AgentStreamEvent::RunFinished {
                        run_id: run_id.clone(),
                        total_steps: 0,
                        finish_reason: FinishReason::Error,
                        error_message: Some(err.to_string()),
                        duration_ms: start_time.elapsed().as_millis() as i64,
                        usage: None,
                    },
                )
                .await;
            return;
        }

        // ── Before Agent 钩子 ─────────────────────
        if let Err(e) = self.hook_before_agent(session_id, user_id, input).await {
            warn!("Before Agent 钩子执行失败: {}", e);
        }
        // RunStarted：容错落库 + 推流（任一发布失败都直接终结流，channel 已关闭无可挤回）
        if let Err(e) = self
            .emit_persisted(
                &events,
                session_id,
                AgentStreamEvent::RunStarted {
                    run_id: run_id.clone(),
                    agent_id,
                    agent_name: agent_name.clone(),
                    session_id: session_id.to_string(),
                    user_id: user_id.to_string(),
                    meta: self.meta.clone(),
                },
            )
            .await
        {
            warn!(run_id = %run_id, error = %e, "发布 RunStarted 事件失败，提前结束");
            return;
        }
        // UserMessage：将用户输入作为独立的"消息事件"落库，
        // 与 RunStarted 的"运行元数据"语义分离，便于历史页直接渲染用户气泡。
        if let Err(e) = self
            .emit_persisted(
                &events,
                session_id,
                AgentStreamEvent::UserMessage {
                    run_id: run_id.clone(),
                    content: input.to_string(),
                },
            )
            .await
        {
            warn!(run_id = %run_id, error = %e, "发布 UserMessage 事件失败，提前结束");
            return;
        }

        // 构建初始消息上下文（系统 Prompt + 记忆 + 历史 + 用户输入）
        info!(run_id = %run_id, session_id = session_id, agent_id = agent_id, user_id = user_id, "开始处理用户请求");
        let messages = match self
            .build_initial_messages(session_id, input, agent_id, user_id)
            .await
        {
            Ok(m) => m,
            Err(err) => {
                warn!(run_id = %run_id, error = %err, "构建初始消息失败");
                let _ = self
                    .emit_persisted(
                        &events,
                        session_id,
                        AgentStreamEvent::RunFinished {
                            run_id: run_id.clone(),
                            total_steps: 0,
                            finish_reason: FinishReason::Error,
                            error_message: Some(err.to_string()),
                            duration_ms: start_time.elapsed().as_millis() as i64,
                            usage: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let toolset = Arc::clone(&self.tool_pool);
        let model_caller = self.build_model_caller(events.clone(), self.run_config.clone());

        // 委托内部 ReAct 循环；失败时降级为「空回答 + Error」
        info!(run_id = %run_id, session_id = session_id, agent_id = agent_id, user_id = user_id, "开始准备 ReAct 循环");
        let loop_result = match self
            .drive_react_loop(
                &events,
                &model_caller,
                &run_id,
                session_id,
                user_id,
                agent_id,
                max_turns,
                messages,
                toolset,
            )
            .await
        {
            Ok(result) => result,
            Err(err) => {
                warn!(run_id = %run_id, error = %err, "Agent 运行异常终止");
                let _ = self
                    .emit_persisted(
                        &events,
                        session_id,
                        AgentStreamEvent::RunFinished {
                            run_id: run_id.clone(),
                            total_steps: 0,
                            finish_reason: FinishReason::Error,
                            error_message: Some(err.to_string()),
                            duration_ms: start_time.elapsed().as_millis() as i64,
                            usage: None,
                        },
                    )
                    .await;
                return;
            }
        };

        info!(run_id = %run_id, session_id = session_id, agent_id = agent_id, user_id = user_id, "ReAct 循环已结束");

        // ── 循环后处理：最终答案总结与持久化 ───
        let (final_answer, finish_reason, total_steps, error_message, accumulated_usage) = self
            .process_loop_result(
                &events,
                &model_caller,
                &run_id,
                session_id,
                agent_id,
                user_id,
                max_turns,
                loop_result,
            )
            .await;

        // ── After Agent 钩子 ──────────────────
        if let Err(e) = self
            .hook_after_agent(session_id, &final_answer, total_steps)
            .await
        {
            warn!(run_id = %run_id, "After Agent 钩子执行失败: {}", e);
        }

        // ── 更新会话状态为终态（仅主智能体执行） ──
        //
        // 子智能体不管理 session 状态，终态更新由主智能体负责。
        if self.meta.agent_source.is_primary() {
            let final_status = match &finish_reason {
                FinishReason::FinalizeAction => SessionStatus::Completed,
                FinishReason::Cancelled => SessionStatus::Cancelled,
                FinishReason::MaxTurns | FinishReason::Error => SessionStatus::Completed,
            };
            if let Err(e) = self
                .services
                .update_session(session_id, agent_id, user_id, Some(final_status))
                .await
            {
                warn!(run_id = %run_id, error = %e, "更新会话终态失败");
            }
        }

        // RunFinished：容错落库 + 推流（构造一次，双写一次）
        let duration_ms = start_time.elapsed().as_millis() as i64;
        if let Err(e) = self
            .emit_persisted(
                &events,
                session_id,
                AgentStreamEvent::RunFinished {
                    run_id: run_id.clone(),
                    total_steps,
                    finish_reason: finish_reason.clone(),
                    error_message,
                    duration_ms,
                    usage: accumulated_usage,
                },
            )
            .await
        {
            warn!(run_id = %run_id, error = %e, "发布 RunFinished 事件失败");
        }

        info!(run_id = %run_id, total_steps = total_steps, "Agent 运行结束");
    }

    /// ReAct 主循环：推理 ↔ 工具 → 直至产出最终回答或达到最大轮次
    ///
    /// 返回 [`ReActLoopResult`]，包含循环终止时的最后轮次状态（最后一轮
    /// [`ModelTurn`]、步数、累计用量、消息上下文、工具集）。
    /// 调用方（如 [`Self::run_stream_inner`]）负责在循环外执行最终答案的
    /// 总结（`summarize_final_answer`）与持久化（`finalize_answer`），
    /// 从而将循环策略与循环后处理解耦，便于替换核心循环实现。
    ///
    /// `error_message` 的语义与 [`FinishReason`] 按需对齐：
    /// - `FinalizeAction` → `None`
    /// - `MaxTurns` → 「已达到最大推理轮次 N」
    /// - `Cancelled` → 「运行已被取消」（预留，当前未达进入）
    /// - `Error` → 由调用点（`run_stream_inner` 的 `Err` 分支）填充真实错误文案
    #[allow(clippy::too_many_arguments)]
    async fn drive_react_loop(
        &self,
        events: &mpsc::Sender<AgentStreamEvent>,
        model_caller: &Arc<ModelCaller>,
        run_id: &str,
        session_id: &str,
        user_id: &str,
        agent_id: i64,
        max_turns: u32,
        mut messages: MessageContext,
        toolset: Arc<ToolPool>,
    ) -> AgentResult<LoopResult> {
        let mut step: u32 = 0;
        let mut accumulated_usage: Option<Usage> = None;

        loop {
            step += 1;

            // 检查最大轮次
            if step > max_turns {
                warn!(run_id = %run_id, step = step, max_turns = max_turns, "达到最大推理轮次");

                // 达到最大轮次后直接返回
                return Ok(LoopResult {
                    last_turn: None,
                    finish_reason: FinishReason::MaxTurns,
                    step,
                    accumulated_usage,
                    messages,
                });
            }

            // ── 消费注入消息（从队列表 drain） ──
            let messages_injected = self.services.drain_pending_input(session_id).await;
            if !messages_injected.is_empty() {
                info!(run_id = %run_id, count = messages_injected.len(), "消费注入消息");
                for msg in &messages_injected {
                    self.services
                        .try_append_context(
                            ContextScope::Session {
                                session_id,
                                agent_id,
                                user_id,
                            },
                            msg,
                        )
                        .await;
                }
                for msg in messages_injected {
                    messages.push(msg);
                }
            }

            self.emit_persisted(
                events,
                session_id,
                AgentStreamEvent::StepStarted {
                    run_id: run_id.to_string(),
                    step,
                },
            )
            .await?;

            info!(run_id = %run_id, step = step, "开始 LLM 推理");

            // 单步推理 + 动作执行（含 before/after_model 钩子、工具执行）
            let turn = model_caller
                .call_with_tool(
                    run_id,
                    step,
                    &mut messages,
                    &toolset,
                    agent_id,
                    session_id,
                    user_id,
                )
                .await?;

            if let Some(ref step_usage) = turn.usage {
                accumulated_usage = Some(Self::accumulate_usage(accumulated_usage, step_usage));
            }

            // ReAct 循环判定：纯文本响应即视为候选终止点
            // 是否作为最终回答，由循环外统一判断（支持额外总结轮）
            if turn.is_final_answer() {
                return Ok(LoopResult {
                    last_turn: Some(turn),
                    finish_reason: FinishReason::FinalizeAction,
                    step,
                    accumulated_usage,
                    messages,
                });
            }
        }
    }

    /// 原始单轮 LLM 调用的核心执行循环（事件流唯一事实源）
    ///
    /// 与 [`Self::run_stream_inner`] 的完整生命周期一致：
    /// - 会话主表保障（get + create/update）
    /// - before_agent / after_agent 钩子
    /// - RunStarted / UserMessage / StepStarted / RunFinished 事件
    /// - before_model / after_model 钩子（通过 `ModelCaller::call_raw`）
    /// - TextDelta / ToolCallsPlanned 等流式事件推送
    ///
    /// 唯一区别：**不执行工具调用**。LLM 返回的 tool_calls 作为原始结果
    /// 透传到事件流中，由上层 `infer` / `infer_stream`
    /// 转换为 OpenAI 兼容格式。
    ///
    /// # Parameters
    ///
    /// * `events` - 事件流写端
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本（从 messages 中提取的最后一条 user 消息）
    /// * `user_id` - 用户 ID
    async fn infer_stream_inner(
        &self,
        events: mpsc::Sender<AgentStreamEvent>,
        session_id: &str,
        user_id: &str,
        input: &str,
    ) {
        let run_id = Uuid::new_v4().to_string();

        let agent_id = self.agent_config.agent_definition.id;
        let agent_name = self.agent_config.agent_definition.name.clone();
        let start_time = std::time::Instant::now();

        info!(run_id = %run_id, session_id = session_id, agent_id = agent_id, user_id = user_id, "Agent raw 运行开始");

        // ── 会话主表保障（仅主智能体执行） ──
        if let Err(err) = self
            .ensure_session_running(session_id, agent_id, user_id)
            .await
        {
            warn!(run_id = %run_id, session_id = %session_id, error = %err, "会话主表初始化失败，终止运行");
            let _ = self
                .emit_persisted(
                    &events,
                    session_id,
                    AgentStreamEvent::RunFinished {
                        run_id: run_id.clone(),
                        total_steps: 0,
                        finish_reason: FinishReason::Error,
                        error_message: Some(err.to_string()),
                        duration_ms: start_time.elapsed().as_millis() as i64,
                        usage: None,
                    },
                )
                .await;
            return;
        }

        // ── Before Agent 钩子 ──
        if let Err(e) = self.hook_before_agent(session_id, user_id, input).await {
            warn!("Before Agent 钩子执行失败: {}", e);
        }

        // RunStarted 事件
        if let Err(e) = self
            .emit_persisted(
                &events,
                session_id,
                AgentStreamEvent::RunStarted {
                    run_id: run_id.clone(),
                    agent_id,
                    agent_name: agent_name.clone(),
                    session_id: session_id.to_string(),
                    user_id: user_id.to_string(),
                    meta: self.meta.clone(),
                },
            )
            .await
        {
            warn!(run_id = %run_id, error = %e, "发布 RunStarted 事件失败，提前结束");
            return;
        }

        // UserMessage 事件
        if let Err(e) = self
            .emit_persisted(
                &events,
                session_id,
                AgentStreamEvent::UserMessage {
                    run_id: run_id.clone(),
                    content: input.to_string(),
                },
            )
            .await
        {
            warn!(run_id = %run_id, error = %e, "发布 UserMessage 事件失败，提前结束");
            return;
        }

        // 构建初始消息上下文（系统 Prompt + 记忆 + 历史 + 用户输入）
        let mut messages = match self
            .build_initial_messages(session_id, input, agent_id, user_id)
            .await
        {
            Ok(m) => m,
            Err(err) => {
                warn!(run_id = %run_id, error = %err, "构建初始消息失败");
                let _ = self
                    .emit_persisted(
                        &events,
                        session_id,
                        AgentStreamEvent::RunFinished {
                            run_id: run_id.clone(),
                            total_steps: 0,
                            finish_reason: FinishReason::Error,
                            error_message: Some(err.to_string()),
                            duration_ms: start_time.elapsed().as_millis() as i64,
                            usage: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let toolset = Arc::clone(&self.tool_pool);
        let model_caller = self.build_model_caller(events.clone(), self.run_config.clone());

        // StepStarted 事件（单轮，step = 1）
        let step: u32 = 1;
        if let Err(e) = self
            .emit_persisted(
                &events,
                session_id,
                AgentStreamEvent::StepStarted {
                    run_id: run_id.clone(),
                    step,
                },
            )
            .await
        {
            warn!(run_id = %run_id, error = %e, "发布 StepStarted 事件失败");
        }

        // 单轮 LLM 调用（不执行工具）
        let (final_answer, finish_reason, error_message, raw_usage) = match model_caller
            .call(&run_id, step, &mut messages, &toolset, session_id)
            .await
        {
            Ok(turn) => {
                let usage = turn.usage;
                let answer = turn.text.unwrap_or_default();

                // 最终回答的统一镜像投递与持久化
                let agent_id = self.agent_config.agent_definition.id;
                let user_id = &self.agent_config.agent_definition.user_id;
                if let Err(e) = self
                    .finalize_answer(
                        &events, &run_id, step, &answer, session_id, agent_id, user_id,
                    )
                    .await
                {
                    warn!(run_id = %run_id, step = step, error = %e, "finalize_answer 失败");
                }

                (answer, FinishReason::FinalizeAction, None, usage)
            }
            Err(err) => {
                warn!(run_id = %run_id, error = %err, "Agent raw 运行异常终止");
                (
                    String::new(),
                    FinishReason::Error,
                    Some(err.to_string()),
                    None,
                )
            }
        };

        // ── After Agent 钩子 ──
        if let Err(e) = self.hook_after_agent(session_id, &final_answer, step).await {
            warn!(run_id = %run_id, "After Agent 钩子执行失败: {}", e);
        }

        // ── 更新会话状态为终态（仅主智能体执行） ──
        if self.meta.agent_source.is_primary() {
            let final_status = match &finish_reason {
                FinishReason::FinalizeAction => SessionStatus::Completed,
                FinishReason::Cancelled => SessionStatus::Cancelled,
                FinishReason::MaxTurns | FinishReason::Error => SessionStatus::Completed,
            };
            if let Err(e) = self
                .services
                .update_session(session_id, agent_id, user_id, Some(final_status))
                .await
            {
                warn!(run_id = %run_id, error = %e, "更新会话终态失败");
            }
        }

        // RunFinished 事件
        let duration_ms = start_time.elapsed().as_millis() as i64;
        if let Err(e) = self
            .emit_persisted(
                &events,
                session_id,
                AgentStreamEvent::RunFinished {
                    run_id: run_id.clone(),
                    total_steps: step,
                    finish_reason: finish_reason.clone(),
                    error_message,
                    duration_ms,
                    usage: raw_usage,
                },
            )
            .await
        {
            warn!(run_id = %run_id, error = %e, "发布 RunFinished 事件失败");
        }

        info!(run_id = %run_id, "Agent raw 运行结束");
    }
}

#[async_trait::async_trait]
impl Agent for LlmAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn model(&self) -> &str {
        &self.agent_config.model_config.model
    }

    /// 执行一次智能体运行（流式视图，唯一事实源）
    ///
    /// 即时创建 `mpsc::channel`，写端注入新构建的 [`ModelCaller`] / [`ToolExecutor`]，
    /// 读端包装为 [`AgentEventStream`] 返回给调用方。运行循环在独立任务中执行。
    fn run_stream(&self, session_id: &str, input: &str) -> AgentResult<AgentEventStream> {
        let (tx, rx) = mpsc::channel(DEFAULT_EVENT_CHANNEL_CAPACITY);

        let agent = self.clone();
        let session_id = session_id.to_string();
        let input = input.to_string();
        let user_id = self.agent_config.agent_definition.user_id.clone();

        tokio::spawn(async move {
            agent
                .run_stream_inner(tx, &session_id, &user_id, &input)
                .await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    /// 启动单轮 LLM 推理的事件流
    ///
    /// 即时创建 `mpsc::channel`，写端注入 [`Self::infer_stream_inner`]，
    /// 读端包装为 [`AgentEventStream`] 返回给调用方。
    ///
    /// 特点：**不执行工具调用**。LLM 返回的 tool_calls 作为原始结果
    /// 透传到事件流中，由上层 `infer` / `infer_stream` 转换为 OpenAI 兼容格式。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    fn infer_stream(&self, session_id: &str, input: &str) -> AgentResult<AgentEventStream> {
        let (tx, rx) = mpsc::channel(DEFAULT_EVENT_CHANNEL_CAPACITY);

        let agent = self.clone();
        let session_id = session_id.to_string();
        let input = input.to_string();
        let user_id = self.agent_config.agent_definition.user_id.clone();

        tokio::spawn(async move {
            agent
                .infer_stream_inner(tx, &session_id, &user_id, &input)
                .await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}

#[async_trait::async_trait]
impl AgentOpenAI for LlmAgent {}
