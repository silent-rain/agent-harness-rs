//! LLM 流式调用、决策聚合与单步执行
//!
//! [`ModelCaller`] 是一个**纯单步原语**：调用 LLM、聚合输出、触发钩子、
//! 执行工具调用，并把聚合后的 [`ModelTurn`] 直接交还给主循环。
//!
//! 终止策略由调用方决定：
//!
//! - 若 [`ModelTurn::tool_calls`] 非空 → 工具结果已就地回灌到消息上下文，
//!   主循环应继续下一轮；
//! - 若 [`ModelTurn::tool_calls`] 为空且 [`ModelTurn::text`] 非空 →
//!   主循环应调用 [`ModelCaller::finalize_answer`] 完成镜像投递与持久化，
//!   随后退出循环；
//! - 两者都为空 → [`ModelCaller::call_with_tool`] 直接返回错误。
//!
//! 工具调用执行错误**不会**冒泡到主循环：失败的工具已通过
//! `ToolCallFinished { success: false }` 事件 + 工具消息回灌的方式让
//! LLM 在下一轮看到，从而进行 ReAct 式自我修正。
//!
//! 提供的主要内容：
//! - [`ModelCaller`] - 单步 LLM 推理与执行编排器
//! - [`StreamedToolCallState`] - 流式 tool_call 增量聚合状态（内部）

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures::{StreamExt, future::join_all};
use rig_core::{
    completion::{CompletionRequest, Usage},
    one_or_many::OneOrMany,
};
use tokio::{
    sync::{Semaphore, mpsc},
    time::interval,
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    ConversationMessage,
    context::MessageContext,
    error::{AgentResult, Error},
    hook::HookRegistry,
    model::{BoxedCompletionStream, ModelAdapter, StreamChunk, ToolCallChunkContent},
    runtime::{
        RuntimeServices,
        llm::{tool_executor::ToolExecutor, types::ModelTurn},
    },
    session::{AgentStreamEvent, PlannedToolCall},
    tool::{ContextualToolSet, ToolPool},
    types::{
        hook::{AfterModelArgs, BeforeModelArgs},
        runtime::{AgentConfig, RunConfig},
        session::ContextScope,
    },
};
/// 流式 tool_call 聚合状态
#[derive(Debug, Default)]
struct StreamedToolCallState {
    /// provider 分配的 tool_call ID（如有）
    id: Option<String>,
    /// 聚合后的工具名称
    name: String,
    /// 聚合后的原始参数 JSON 字符串
    arguments_json: String,
}

impl StreamedToolCallState {
    fn push_name(&mut self, delta: String) {
        self.name.push_str(&delta);
    }

    fn push_arguments(&mut self, delta: String) {
        self.arguments_json.push_str(&delta);
    }

    /// 将聚合结果转换为 [`PlannedToolCall`]
    ///
    /// 不做 JSON 解析或修复——原始参数字符串直接透传，
    /// 修复统一由 [`ToolExecutor`] 在实际执行时处理。
    fn into_planned_call(self) -> AgentResult<PlannedToolCall> {
        if self.name.trim().is_empty() {
            return Err(Error::FunctionCallProtocol(
                "流式 tool_call 缺少函数名，无法解析决策".to_string(),
            ));
        }

        let arguments_json = if self.arguments_json.trim().is_empty() {
            "{}".to_string()
        } else {
            self.arguments_json
        };

        Ok(PlannedToolCall {
            call_id: self.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            tool_name: self.name,
            arguments_json,
            // 装配阶段未查表；调用方会随后用 `tool_pool.source_of` 回填。
            source: Default::default(),
        })
    }
}

/// LLM 单步推理与执行编排器
///
/// 一次 [`call_with_tool`](Self::call_with_tool) 涵盖：
/// 1. `before_model` 观察钩子
/// 2. LLM 流式调用与聚合
/// 3. `after_model` 观察钩子
/// 4. 工具调用执行（信号量限流的并发，`max_parallel_tools = 1` 即退化为串行）
///
/// **不**承担循环终止判定：返回的 [`ModelTurn`] 由调用方解读，必要时再调用
/// [`finalize_answer`](Self::finalize_answer) 完成最终回答的镜像投递与落库。
pub struct ModelCaller {
    /// LLM 模型适配器
    model: Arc<dyn ModelAdapter>,
    /// 智能体运行时配置（用于 model_code / temperature / max_tokens 等）
    agent_config: AgentConfig,
    /// 事件发送端：所有运行进展通过此 channel 流式推送给消费方
    events: mpsc::Sender<AgentStreamEvent>,
    /// 生命周期钩子注册表
    hooks: Arc<HookRegistry>,
    /// 单工具执行器
    tool_executor: Arc<ToolExecutor>,
    /// 运行时服务（最终回答落库与记忆）
    services: RuntimeServices,
    /// 候选工具池
    ///
    /// 在生成 [`PlannedToolCall`] 与发射 `ToolCallsPlanned` 事件时查表，
    /// 为每条计划中的调用填充 `source` 字段。与 `Builder` 装配阶段
    /// 冻结的同一份 `Arc<ToolPool>` 共享。
    tool_pool: Arc<ToolPool>,
    /// ReAct 循环运行时参数
    run_config: RunConfig,
}

impl ModelCaller {
    /// 创建 ModelCaller
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: Arc<dyn ModelAdapter>,
        agent_config: AgentConfig,
        events: mpsc::Sender<AgentStreamEvent>,
        hooks: Arc<HookRegistry>,
        tool_executor: Arc<ToolExecutor>,
        services: RuntimeServices,
        tool_pool: Arc<ToolPool>,
        run_config: RunConfig,
    ) -> Self {
        Self {
            model,
            agent_config,
            events,
            hooks,
            tool_executor,
            services,
            tool_pool,
            run_config,
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
    /// 调用 [`RuntimeServices::try_append_event`] 容错落库，再通过
    /// [`Self::emit`] 推送；落库失败仅 `warn!`，不阻断推流。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID（落库目标）
    /// * `event` - 待双写的事件
    async fn emit_persisted(&self, session_id: &str, event: AgentStreamEvent) -> AgentResult<()> {
        self.services
            .try_append_event(
                session_id,
                self.agent_config.agent_definition.id,
                &self.agent_config.agent_definition.user_id,
                &event,
            )
            .await;
        self.emit(event).await
    }

    // ─── 钩子触发 ───────────────────────────────

    /// 触发 `before_model` 观察钩子
    async fn hook_before_model(
        &self,
        run_id: &str,
        step: u32,
        messages: &MessageContext,
        toolset: &ContextualToolSet,
    ) -> AgentResult<()> {
        self.hooks
            .fire_before_model(BeforeModelArgs {
                run_id: run_id.to_string(),
                step,
                message_count: messages.len(),
                tool_count: toolset.tool_definitions().await.len(),
            })
            .await?;
        Ok(())
    }

    /// 触发 `after_model` 观察钩子
    async fn hook_after_model(&self, run_id: &str, step: u32, turn: &ModelTurn) -> AgentResult<()> {
        self.hooks
            .fire_after_model(AfterModelArgs {
                run_id: run_id.to_string(),
                step,
                text: turn.text.clone(),
                tool_call_count: turn.tool_calls.len(),
            })
            .await?;
        Ok(())
    }
    // ─── LLM 流式调用 ───────────────────────────────────────────
}

impl ModelCaller {
    /// 执行单步推理与动作
    ///
    /// 行为：
    /// 1. 触发 `before_model` 观察钩子
    /// 2. 调用 LLM 并聚合流式输出为 [`ModelTurn`]
    /// 3. 触发 `after_model` 观察钩子
    /// 4. 若有工具调用：限流并发执行，结果就地追加到 `messages`
    ///
    /// 失败语义：
    /// - LLM 调用失败 → 发布 `Error` 事件并冒泡，由主循环决定是否中止
    /// - 工具调用失败 → **不冒泡**，错误已通过 `ToolCallFinished` 事件与
    ///   工具消息回灌让 LLM 在下一轮自行修正
    /// - LLM 返回空响应（既无 text 也无 tool_calls） → 直接报错
    ///
    /// # Parameters
    ///
    /// * `run_id` - 运行 ID
    /// * `step` - 当前步骤序号
    /// * `messages` - 当前消息上下文（工具结果会就地追加）
    /// * `toolset` - 业务工具集
    /// * `agent_id` - 智能体 ID（透传给 ToolExecutor）
    /// * `session_id` - 会话 ID
    /// * `user_id` - 用户 ID
    ///
    /// # Returns
    ///
    /// 本步聚合后的 [`ModelTurn`]。调用方据此决定是否继续循环：
    /// 通过 [`ModelTurn::is_final_answer`] 判定是否进入终止流程。
    #[allow(clippy::too_many_arguments)]
    pub async fn call_with_tool(
        &self,
        run_id: &str,
        step: u32,
        messages: &mut MessageContext,
        toolset: &ContextualToolSet,
        agent_id: i64,
        session_id: &str,
        user_id: &str,
    ) -> AgentResult<ModelTurn> {
        let turn = self
            .call(run_id, step, messages, toolset, session_id)
            .await?;

        // 若有工具调用：先将 assistant 消息（含 tool_calls）追加到消息上下文，
        // 再执行工具调用（工具结果也会追加到上下文之后）。
        // 这确保了下一轮 LLM 调用时上下文中的消息顺序符合 OpenAI 规范：
        //   [assistant + tool_calls] → [tool result 1] → [tool result 2] → ...
        if !turn.tool_calls.is_empty() {
            let tool_calls = turn.tool_calls.clone();

            // 构造包含 tool_calls 的 assistant 消息，追加到上下文
            let assistant_message = ConversationMessage::assistant_with_tool_calls(
                turn.text.clone().unwrap_or_default(),
                tool_calls.clone(),
            );
            messages.push(assistant_message.clone());
            self.services
                .try_append_context(
                    ContextScope::Session {
                        session_id,
                        agent_id,
                        user_id,
                    },
                    &assistant_message,
                )
                .await;

            // 执行工具调用，工具结果会作为 tool 角色消息追加到上下文
            self.execute_tool_calls(
                run_id,
                step,
                &turn.tool_calls,
                messages,
                agent_id,
                session_id,
                user_id,
            )
            .await?;
        }
        Ok(turn)
    }

    /// 原始单轮 LLM 调用（不执行工具）
    ///
    /// 与 [`Self::call_with_tool`] 的区别：
    /// - 触发 before_model / after_model 钩子 ✅
    /// - 流式推理 + 事件推流（TextDelta / ToolCallsPlanned 等） ✅
    /// - **不执行工具调用** ❌
    ///
    /// 返回的 [`ModelTurn`] 中 `tool_calls` 可能非空，但不会被执行，
    /// 调用方可据此构建 OpenAI 兼容的 `finish_reason = tool_calls` 响应。
    pub async fn call(
        &self,
        run_id: &str,
        step: u32,
        messages: &mut MessageContext,
        toolset: &ContextualToolSet,
        session_id: &str,
    ) -> AgentResult<ModelTurn> {
        // 1. before_model 钩子
        if let Err(e) = self
            .hook_before_model(run_id, step, messages, toolset)
            .await
        {
            error!(run_id = %run_id, step = step, error = %e, "before_model 钩子失败");
        }

        // 2. LLM 流式调用与聚合
        let turn = match self
            .invoke_llm(run_id, session_id, step, messages, toolset)
            .await
        {
            Ok(turn) => turn,
            Err(e) => {
                error!(run_id = %run_id, step = step, error = %e, "LLM 调用失败");
                return Err(e);
            }
        };

        // 3. after_model 钩子
        if let Err(e) = self.hook_after_model(run_id, step, &turn).await {
            error!(run_id = %run_id, step = step, error = %e, "fire_after_model 钩子失败");
        }

        if turn.is_empty() {
            return Err(Error::Internal("LLM 返回空响应，无法继续推理".to_string()));
        }

        // 注意：不执行 execute_tool_calls，直接返回原始 ModelTurn
        Ok(turn)
    }

    /// 调用 LLM 并把流聚合为 [`ModelTurn`]
    async fn invoke_llm(
        &self,
        run_id: &str,
        session_id: &str,
        step: u32,
        messages: &MessageContext,
        toolset: &ContextualToolSet,
    ) -> AgentResult<ModelTurn> {
        let rig_messages = messages.to_rig_messages();
        let chat_history = OneOrMany::many(rig_messages).map_err(|_| {
            Error::ModelCall("消息列表为空，无法构建 CompletionRequest".to_string())
        })?;
        let tool_definitions = toolset.tool_definitions().await;

        let request = CompletionRequest {
            model: Some(self.agent_config.model_config.model.clone()),
            preamble: None,
            chat_history,
            documents: vec![],
            tools: tool_definitions,
            temperature: self.agent_config.model_config.temperature,
            max_tokens: self.agent_config.model_config.max_tokens,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };

        debug!(
            run_id = %run_id,
            step = step,
            model = %self.agent_config.model_config.model,
            tools = request.tools.len(),
            "调用 LLM 进行流式推理"
        );

        let stream = self.model.stream_completion(request).await?;
        self.consume_stream(run_id, session_id, step, stream).await
    }

    /// 消费 LLM 流并聚合为 [`ModelTurn`]
    ///
    /// 推流与落库的解耦原则：
    /// - **循环内仅推流**：所有面向客户端的实时事件（`TextDelta` /
    ///   `Reasoning` 整段 / `ReasoningDelta`）通过 [`Self::emit`] 推送，
    ///   不在循环内同步触发 DB 写入，避免单次 DB 抖动阻塞 token 流。
    /// - **循环外统一落库**：本轮所有需要持久化的事件在循环结束后
    ///   按业务语义聚合一次性写库；`ReasoningDelta` 按 `id` 聚合为整段，
    ///   `Reasoning` 整段直接以原事件落库，`ToolCallsPlanned` 在聚合后落库。
    ///
    /// ## 取消检测
    ///
    /// 使用 `tokio::select!` + 定时轮询数据库状态实现流式推理中的实时取消：
    /// - 每 2 秒查询一次数据库中的会话状态
    /// - 若检测到 `Cancelled` 状态则立即终止流式读取
    /// - 完全无状态设计，不依赖外部共享原子量
    async fn consume_stream(
        &self,
        run_id: &str,
        session_id: &str,
        step: u32,
        mut stream: BoxedCompletionStream,
    ) -> AgentResult<ModelTurn> {
        let mut text_buffer = String::new();
        let mut tool_call_order: Vec<String> = Vec::new();
        let mut tool_calls: HashMap<String, StreamedToolCallState> = HashMap::new();
        // 收到整段 `Reasoning` 的段 id 集合：用于兜底阶段去重，避免与 delta 重复落库。
        let mut reasoning_emitted_full: HashSet<Option<String>> = HashSet::new();
        // 仅收到 `ReasoningDelta` 的段：循环内逐 token 推流，循环外按 id 聚合后落库。
        let mut reasoning_delta_buffer: HashMap<Option<String>, String> = HashMap::new();
        let mut chunk_count = 0usize;
        let mut stream_usage: Option<Usage> = None;

        // 定时轮询间隔（从 RunConfig 读取）
        let mut poll_interval = interval(self.run_config.cancel_poll_interval);
        poll_interval.tick().await; // 跳过首次立即触发

        let agent_id = self.agent_config.agent_definition.id;
        let user_id = &self.agent_config.agent_definition.user_id;

        loop {
            tokio::select! {
                biased;

                // 定时轮询数据库状态检测取消信号
                _ = poll_interval.tick() => {
                    if let Ok(Some(status)) = self.services
                        .get_session(session_id, agent_id, user_id)
                        .await
                        && status.is_cancelled() {
                            info!(run_id = %run_id, step = step, "流式读取中检测到取消信号，终止 LLM 推理");
                            return Ok(ModelTurn {
                                text: Some("运行已被取消".to_string()),
                                tool_calls: Vec::new(),
                                usage: stream_usage,
                            });
                        }
                }

                // 消费流式 chunk
                chunk = stream.next() => {
                    let item = match chunk {
                        Some(item) => item,
                        None => break, // 流结束
                    };

                    chunk_count += 1;
                    let done = self
                        .process_stream_chunk(
                            item?,
                            run_id,
                            session_id,
                            &mut text_buffer,
                            &mut tool_call_order,
                            &mut tool_calls,
                            &mut reasoning_emitted_full,
                            &mut reasoning_delta_buffer,
                            &mut stream_usage,
                        )
                        .await?;
                    if done {
                        break;
                    }
                }
            }
        }

        self.finalize_stream(
            run_id,
            session_id,
            step,
            chunk_count,
            text_buffer,
            tool_call_order,
            tool_calls,
            reasoning_emitted_full,
            reasoning_delta_buffer,
            stream_usage,
        )
        .await
    }

    /// 处理单个流式 chunk
    ///
    /// 返回 `true` 表示流已结束（收到 `Done` 信号），调用方应跳出循环。
    /// 内部仅做推流与内存累积，不触发 DB 写入。
    #[allow(clippy::too_many_arguments)]
    async fn process_stream_chunk(
        &self,
        chunk: StreamChunk,
        run_id: &str,
        session_id: &str,
        text_buffer: &mut String,
        tool_call_order: &mut Vec<String>,
        tool_calls: &mut HashMap<String, StreamedToolCallState>,
        reasoning_emitted_full: &mut HashSet<Option<String>>,
        reasoning_delta_buffer: &mut HashMap<Option<String>, String>,
        stream_usage: &mut Option<Usage>,
    ) -> AgentResult<bool> {
        match chunk {
            StreamChunk::TextDelta(delta) => {
                if !delta.is_empty() {
                    self.emit(AgentStreamEvent::TextDelta {
                        run_id: run_id.to_string(),
                        delta: delta.clone(),
                    })
                    .await?;
                    text_buffer.push_str(&delta);
                }
            }
            StreamChunk::Reasoning { id, text } => {
                // 整段思考块：provider 一次性返回的完整推理段。
                reasoning_emitted_full.insert(id.clone());
                if !text.is_empty() {
                    self.emit_persisted(
                        session_id,
                        AgentStreamEvent::Reasoning {
                            run_id: run_id.to_string(),
                            id: id.clone(),
                            content: text.clone(),
                        },
                    )
                    .await?;
                }
            }
            StreamChunk::ReasoningDelta { id, text } => {
                // 思考链 token 增量：下游需累积拼接；同时本地累积以备末尾兜底落库
                if !text.is_empty() {
                    self.emit(AgentStreamEvent::ReasoningDelta {
                        run_id: run_id.to_string(),
                        id: id.clone(),
                        delta: text.clone(),
                    })
                    .await?;
                    reasoning_delta_buffer
                        .entry(id)
                        .or_default()
                        .push_str(&text);
                }
            }
            StreamChunk::ToolCall {
                id,
                internal_call_id,
                name,
                arguments,
            } => {
                // 仅在内部累积，不推流 / 不落库。
                // 末尾一次性发 `ToolCallsPlanned` 整批镜像。
                if !tool_call_order.contains(&internal_call_id) {
                    tool_call_order.push(internal_call_id.clone());
                }
                tool_calls.insert(
                    internal_call_id,
                    StreamedToolCallState {
                        id: (!id.is_empty()).then_some(id),
                        name,
                        arguments_json: arguments.to_string(),
                    },
                );
            }
            StreamChunk::ToolCallDelta(delta) => {
                // 仅内部累积，不对外推流 / 不落库
                if !tool_call_order.contains(&delta.internal_call_id) {
                    tool_call_order.push(delta.internal_call_id.clone());
                }
                let entry = tool_calls.entry(delta.internal_call_id).or_default();
                if !delta.id.is_empty() {
                    entry.id = Some(delta.id);
                }
                match delta.content {
                    ToolCallChunkContent::Name(name) => entry.push_name(name),
                    ToolCallChunkContent::ArgumentsDelta(arguments_delta) => {
                        entry.push_arguments(arguments_delta)
                    }
                }
            }
            StreamChunk::MessageId(_) => {}
            StreamChunk::Done { usage } => {
                *stream_usage = usage;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 流式循环结束后的统一落库与聚合
    ///
    /// 将循环内累积的 `ReasoningDelta` 按 `id` 聚合落库，
    /// 组装 `ToolCallsPlanned` 事件双写，最终返回 [`ModelTurn`]。
    #[allow(clippy::too_many_arguments)]
    async fn finalize_stream(
        &self,
        run_id: &str,
        session_id: &str,
        step: u32,
        chunk_count: usize,
        text_buffer: String,
        tool_call_order: Vec<String>,
        mut tool_calls: HashMap<String, StreamedToolCallState>,
        reasoning_emitted_full: HashSet<Option<String>>,
        reasoning_delta_buffer: HashMap<Option<String>, String>,
        stream_usage: Option<Usage>,
    ) -> AgentResult<ModelTurn> {
        // 循环外统一落库（不再触发 emit，推流已在循环内完成）：
        // ReasoningDelta
        for (id, content) in reasoning_delta_buffer.into_iter() {
            if reasoning_emitted_full.contains(&id) || content.is_empty() {
                continue;
            }
            self.services
                .try_append_event(
                    session_id,
                    self.agent_config.agent_definition.id,
                    &self.agent_config.agent_definition.user_id,
                    &AgentStreamEvent::Reasoning {
                        run_id: run_id.to_string(),
                        id,
                        content,
                    },
                )
                .await;
        }

        let mut planned_calls = Vec::with_capacity(tool_call_order.len());
        for internal_call_id in tool_call_order {
            let state = tool_calls.remove(&internal_call_id).ok_or_else(|| {
                Error::FunctionCallProtocol(format!(
                    "流式 tool_call 聚合状态缺失: {}",
                    internal_call_id
                ))
            })?;
            let mut planned = state.into_planned_call()?;
            planned.source = self.tool_pool.source_of(&planned.tool_name);
            planned_calls.push(planned);
        }

        // 整批工具调用公告：本轮只要产生了任何工具调用，就以 `ToolCallsPlanned`
        // 一次性双写落库。合适作为客户端 "先列出待执行清单" 的快照。
        if !planned_calls.is_empty() {
            self.emit_persisted(
                session_id,
                AgentStreamEvent::ToolCallsPlanned {
                    run_id: run_id.to_string(),
                    step,
                    calls: planned_calls.clone(),
                },
            )
            .await?;
        }

        debug!(
            run_id = %run_id,
            step = step,
            chunk_count = chunk_count,
            tool_calls = planned_calls.len(),
            text_len = text_buffer.len(),
            "LLM 流式响应聚合完成"
        );

        Ok(ModelTurn {
            text: (!text_buffer.is_empty()).then_some(text_buffer),
            tool_calls: planned_calls,
            usage: stream_usage,
        })
    }

    // ─── 工具执行与终止处理 ──────────────────────────────────────

    /// 信号量限流的并发执行一组工具调用
    ///
    /// 错误聚合语义：
    /// - 单个工具失败**不会**提前中止其它工具
    /// - 工具失败**不会**让本方法返回 `Err`：失败工具已经通过
    ///   `ToolCallFinished { success: false }` 事件与回灌的工具消息
    ///   让 LLM 在下一轮看到，从而进行 ReAct 式自我修正
    /// - 所有工具消息按原始顺序合并到 `messages`
    ///
    /// `max_parallel_tools = 1` 时退化为严格串行执行。
    #[allow(clippy::too_many_arguments)]
    async fn execute_tool_calls(
        &self,
        run_id: &str,
        step: u32,
        tool_calls: &[PlannedToolCall],
        messages: &mut MessageContext,
        agent_id: i64,
        session_id: &str,
        user_id: &str,
    ) -> AgentResult<()> {
        let sem = Arc::new(Semaphore::new(self.run_config.max_parallel_tools.max(1)));

        let tasks = tool_calls.iter().map(|c| {
            let toolname = c.tool_name.clone();
            let call_id = c.call_id.clone();
            let arguments_json = c.arguments_json.clone();
            let sem = Arc::clone(&sem);
            let executor = Arc::clone(&self.tool_executor);
            let run_id = run_id.to_string();
            let session_id = session_id.to_string();
            async move {
                // 获取信号量许可，超出并发上限时在此等待
                let _permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(_) => {
                        return Err(Error::Internal("semaphore closed unexpectedly".to_string()));
                    }
                };

                executor
                    .run_tool_call(
                        &run_id,
                        step,
                        &toolname,
                        &call_id,
                        &arguments_json,
                        agent_id,
                        &session_id,
                        user_id,
                    )
                    .await
            }
        });

        let results = join_all(tasks).await;

        // 按原始顺序合并消息；工具错误不冒泡，已通过事件 + 消息回灌表达
        for res in results {
            match res {
                Ok(msg) => {
                    messages.push(msg);
                }
                Err(e) => {
                    warn!(
                        run_id = %run_id,
                        step = step,
                        error = %e,
                        "工具执行失败（错误已通过 ToolCallFinished 与回灌消息让 LLM 在下一轮看到）"
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_state_into_planned_call_requires_name() {
        let st = StreamedToolCallState::default();
        assert!(st.into_planned_call().is_err());
    }

    #[test]
    fn streamed_state_into_planned_call_defaults_empty_args() {
        let mut st = StreamedToolCallState::default();
        st.push_name("foo".to_string());
        let pc = st.into_planned_call().expect("应当成功");
        assert_eq!(pc.tool_name, "foo");
        assert_eq!(pc.arguments_json, "{}");
    }
}
