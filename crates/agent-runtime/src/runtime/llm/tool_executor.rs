//! 工具执行器
//!
//! 提供的主要内容：
//! - [`ToolExecutor`] - 单工具调用执行器
//!
//! 内部封装：参数 JSON 修复、重试、事件发布、钩子触发、上下文写入、
//! 消息上下文追加。供 [`ModelCaller`](crate::runtime::llm::model_caller::ModelCaller)
//! 调用以串行或信号量限流的并发编排多个工具。
//!
//! 工具调用的审计信息（参数 / 结果 / 耗时 / 错误）已合并到
//! [`AgentStreamEvent::ToolCallFinished`] 事件中，通过 `append_event` 统一落库，
//! 不再单独记录 `tool_audit` 事件。
//!
//! ## 上下文工具支持
//!
//! 工具调用统一通过 [`ContextualToolSet`] 派发：
//!
//! - 普通无状态工具（`read_file` / MCP / skill / ...）落入 `rig::ToolSet`
//!   走原有路径；
//! - 需要运行时上下文的工具（如子 Agent 委派）实现
//!   [`ContextualTool`](crate::tool::ContextualTool)，调用时由
//!   `ToolExecutor` 把当前 `run_id` / `session_id` / `events` 通过借用
//!   构造的 [`ToolCallCtx`] 传入，无需任何共享可变态。

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{
    error::{AgentResult, Error},
    hook::HookRegistry,
    message::ConversationMessage,
    runtime::{RuntimeServices, llm::repair_json_arguments},
    session::AgentStreamEvent,
    tool::{ContextualCallError, ContextualToolSet, ToolCallCtx, ToolPool},
    types::{
        hook::{AfterToolArgs, BeforeToolArgs},
        runtime::RunConfig,
        session::ContextScope,
    },
};

/// 工具调用执行器
///
/// 封装一次工具调用涉及的全部副作用：
/// - 事件发布（`ToolCallStarted` / `ToolCallFinished`，后者承载完整审计字段）
/// - 钩子触发（`before_tool` / `after_tool`）
/// - 参数 JSON 容错修复（可选）
/// - 失败重试（可选）
/// - 工具结果上下文写入（会话级）
/// - 消息上下文追加
pub struct ToolExecutor {
    /// 上下文工具集（已在 builder 阶段构建完成）
    ///
    /// 同时承载无状态 `rig::Tool` 与有状态 `ContextualTool`，由
    /// [`ContextualToolSet::call`] 在调用点统一派发。
    toolset: Arc<ContextualToolSet>,
    /// 事件发送端：所有运行进展通过此 channel 流式推送给消费方
    events: mpsc::Sender<AgentStreamEvent>,
    /// 运行时服务（写入会话事件 / 记忆）
    services: RuntimeServices,
    /// 钩子注册表
    hooks: Arc<HookRegistry>,
    /// ReAct 循环运行时参数（重试策略 / JSON 修复等）
    run_config: RunConfig,
    /// 智能体 ID（事件落库时填入 `t_agent_session_event.agent_id`）
    agent_id: i64,
    /// 用户 ID（事件落库时填入 `t_agent_session_event.user_id`；支持多种身份系统）
    user_id: String,
    /// 候选工具池
    ///
    /// 发射 `ToolCallStarted` / `ToolCallFinished` 事件时查表填充 `source` 字段。
    /// 与 `Builder` 装配阶段冻结的同一份 `Arc<ToolPool>` 共享。
    tool_pool: Arc<ToolPool>,
}

impl ToolExecutor {
    /// 创建工具执行器
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        toolset: Arc<ContextualToolSet>,
        events: mpsc::Sender<AgentStreamEvent>,
        services: RuntimeServices,
        hooks: Arc<HookRegistry>,
        run_config: RunConfig,
        agent_id: i64,
        user_id: impl Into<String>,
        tool_pool: Arc<ToolPool>,
    ) -> Self {
        Self {
            toolset,
            events,
            services,
            hooks,
            run_config,
            agent_id,
            user_id: user_id.into(),
            tool_pool,
        }
    }

    /// 发送一个事件到运行流，channel 关闭即视为流式失败
    async fn emit(&self, event: AgentStreamEvent) -> AgentResult<()> {
        self.events
            .send(event)
            .await
            .map_err(|e| Error::Streaming(e.to_string()))
    }

    /// 容错落库 + 推流的双写组合
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID（落库目标）
    /// * `event` - 待双写的事件
    async fn emit_persisted(&self, session_id: &str, event: AgentStreamEvent) -> AgentResult<()> {
        self.services
            .try_append_event(session_id, self.agent_id, &self.user_id, &event)
            .await;
        self.emit(event).await
    }

    /// 追加工具结果到会话级上下文
    ///
    /// 工具结果会以 `role = Tool` 的 [`ConversationMessage`] 落入会话级
    /// 上下文存储，作为后续 LLM 请求 messages 的来源；写入失败仅警告，
    /// 不阻断主流程。
    #[allow(clippy::too_many_arguments)]
    async fn append_tool_result_context(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        toolname: &str,
        call_id: &str,
        text: &str,
    ) {
        let message = ConversationMessage::tool_result(toolname, call_id, text);
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
    }

    /// 触发 `before_tool` 钩子
    async fn hook_before_tool(&self, args: BeforeToolArgs) {
        if let Err(e) = self.hooks.fire_before_tool(args).await {
            warn!(error = %e, "BeforeTool 钩子执行失败");
        }
    }

    /// 触发 `after_tool` 钩子
    async fn hook_after_tool(&self, args: AfterToolArgs) {
        if let Err(e) = self.hooks.fire_after_tool(args).await {
            warn!(error = %e, "AfterTool 钩子执行失败");
        }
    }
}

impl ToolExecutor {
    /// 派发单次工具调用：构造借用语义的 [`ToolCallCtx`]
    pub async fn call(
        &self,
        run_id: &str,
        step: u32,
        session_id: &str,
        toolname: &str,
        arguments_json: &str,
    ) -> Result<String, ContextualCallError> {
        let ctx = ToolCallCtx {
            run_id,
            step,
            events: &self.events,
            session_id,
            user_id: &self.user_id,
            agent_id: self.agent_id,
        };
        self.toolset
            .call(ctx, toolname, arguments_json.to_string())
            .await
    }

    /// 调用工具集执行单次工具调用，必要时按配置重试
    ///
    /// 仅在工具集抛出 `Err` 时触发重试；成功结果立即返回。
    ///
    /// # Parameters
    ///
    /// * `run_id` - 运行 ID
    /// * `step` - 当前步骤序号
    /// * `session_id` - 会话 ID（一并构造 [`ToolCallCtx`]）
    /// * `toolname` - 工具唯一编码
    /// * `arguments_json` - 已可能经过 JSON 修复的参数字符串
    pub async fn call_with_retry(
        &self,
        run_id: &str,
        step: u32,
        session_id: &str,
        toolname: &str,
        arguments_json: &str,
    ) -> Result<String, ContextualCallError> {
        // 首次调用
        let mut last_err = match self
            .call(run_id, step, session_id, toolname, arguments_json)
            .await
        {
            Ok(ok) => return Ok(ok),
            Err(e) => {
                error!(run_id = %run_id, step = step, toolname = %toolname, error = %e, "工具调用失败");
                e
            }
        };

        if !self.run_config.retry.is_enabled() {
            return Err(last_err);
        }

        let max_retries = self.run_config.retry.max_retries;
        for attempt in 1..=max_retries {
            let delay = self.run_config.retry.delay_for(attempt);
            debug!(
                run_id = %run_id,
                step = step,
                toolname = %toolname,
                attempt = attempt,
                max_retries = max_retries,
                delay_ms = delay.as_millis() as u64,
                error = %last_err,
                "工具调用失败，准备重试",
            );
            tokio::time::sleep(delay).await;

            match self
                .call(run_id, step, session_id, toolname, arguments_json)
                .await
            {
                Ok(ok) => {
                    debug!(
                        run_id = %run_id,
                        step = step,
                        toolname = %toolname,
                        attempt = attempt,
                        "工具调用重试成功",
                    );
                    return Ok(ok);
                }
                Err(e) => {
                    error!(run_id = %run_id, step = step, toolname = %toolname, error = %e, "工具调用失败");
                    last_err = e;
                }
            }
        }

        warn!(
            run_id = %run_id,
            step = step,
            toolname = %toolname,
            max_retries = max_retries,
            error = %last_err,
            "工具调用重试耗尽，仍然失败",
        );
        Err(last_err)
    }

    /// 执行单次工具调用并将结果追加到给定消息上下文
    ///
    /// 实现保证：
    /// - 发布 `ToolCallStarted` / `ToolCallFinished` 事件
    ///   （`ToolCallFinished` 已合并审计字段：`result` / `error_message` / `duration_ms`）
    /// - 触发 `before_tool` / `after_tool` 钩子
    /// - 写入工具结果到会话级上下文
    ///
    /// # Parameters
    ///
    /// * `run_id` - 运行 ID
    /// * `step` - 当前步骤序号
    /// * `toolname` - 工具唯一编码
    /// * `call_id` - 工具调用 ID
    /// * `arguments_json` - 工具参数 JSON
    /// * `messages` - 消息上下文（结果将追加至此）
    /// * `agent_id` - 智能体 ID
    /// * `session_id` - 会话 ID
    /// * `user_id` - 用户 ID
    #[allow(clippy::too_many_arguments)]
    pub async fn run_tool_call(
        &self,
        run_id: &str,
        step: u32,
        toolname: &str,
        call_id: &str,
        arguments_json: &str,
        agent_id: i64,
        session_id: &str,
        user_id: &str,
    ) -> AgentResult<ConversationMessage> {
        info!(run_id = %run_id, session_id = session_id, agent_id = agent_id, user_id = user_id, "开始执行工具调用");

        // ── 参数 JSON 修复（可选）────────────────────────
        // 启用时若原始 arguments_json 解析失败，调用 repair_json_arguments 做尽力修复。
        let arguments_for_call =
            self.maybe_repair_arguments(run_id, step, toolname, arguments_json);

        self.emit_persisted(
            session_id,
            AgentStreamEvent::ToolCallStarted {
                run_id: run_id.to_string(),
                step,
                toolname: toolname.to_string(),
                call_id: call_id.to_string(),
                arguments_json: arguments_for_call.to_string(),
                source: self.tool_pool.source_of(toolname),
            },
        )
        .await?;

        // ── before_tool 钩子 ──
        self.hook_before_tool(BeforeToolArgs {
            run_id: run_id.to_string(),
            step,
            toolname: toolname.to_string(),
            call_id: call_id.to_string(),
            arguments_json: arguments_for_call.to_string(),
        })
        .await;

        // ── Tool 调用（含重试）───────────────────────────
        let tool_start = std::time::Instant::now();
        let tool_result = self
            .call_with_retry(run_id, step, session_id, toolname, &arguments_for_call)
            .await;
        let duration_ms = tool_start.elapsed().as_millis() as i64;

        let (success, result_text, err_msg) = match tool_result {
            Ok(text) => (true, text, None),
            Err(e) => {
                let msg = e.to_string();
                (false, format!("工具执行失败: {}", msg), Some(msg))
            }
        };

        self.emit_persisted(
            session_id,
            AgentStreamEvent::ToolCallFinished {
                run_id: run_id.to_string(),
                step,
                toolname: toolname.to_string(),
                call_id: call_id.to_string(),
                success,
                result: success.then(|| result_text.clone()),
                error_message: err_msg.clone(),
                duration_ms,
                source: self.tool_pool.source_of(toolname),
            },
        )
        .await?;

        self.hook_after_tool(AfterToolArgs {
            run_id: run_id.to_string(),
            step,
            toolname: toolname.to_string(),
            call_id: call_id.to_string(),
            success,
            result_summary: Some(result_text.clone()),
            duration_ms,
        })
        .await;

        // 失败也记录工具结果上下文，便于后续召回时排除已知失败路径
        self.append_tool_result_context(
            session_id,
            agent_id,
            user_id,
            toolname,
            call_id,
            &result_text,
        )
        .await;

        let msg = ConversationMessage::tool_result(
            toolname.to_string(),
            call_id.to_string(),
            &result_text,
        );

        info!(run_id = %run_id, session_id = session_id, agent_id = agent_id, user_id = user_id, "工具调用结束");
        Ok(msg)
    }

    /// 应用参数 JSON 容错修复（启用且原始参数解析失败时）
    fn maybe_repair_arguments(
        &self,
        run_id: &str,
        step: u32,
        toolname: &str,
        arguments_json: &str,
    ) -> String {
        if !self.run_config.arguments_json_repair
            || serde_json::from_str::<serde_json::Value>(arguments_json).is_ok()
        {
            return arguments_json.to_string();
        }
        let repaired = repair_json_arguments(arguments_json);
        if repaired != arguments_json {
            debug!(
                run_id = %run_id,
                step = step,
                toolname = %toolname,
                "tool_call arguments 已通过 JSON 修复",
            );
        }
        repaired
    }
}
