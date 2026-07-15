//! AgentExecAdapter：将 `Arc<dyn Agent>` 适配为 A2A `MessageHandler`
//!
//! 这是 A2A Server 端的核心适配层，负责：
//! 1. 将 A2A `Message` 转换为 `Agent::run_stream(session_id, input)` 调用
//! 2. 将 `AgentStreamEvent` 流映射为 A2A `StreamResponse` 事件
//! 3. 通过 broadcast channel 将流式事件推送给 A2A Server 的 SSE 层

use std::sync::{Arc, OnceLock};

use a2a_rs_core::{
    AgentCard, Artifact, Message, Part, Role, SendMessageResponse, StreamResponse, Task,
    TaskArtifactUpdateEvent, TaskState, TaskStatus, TaskStatusUpdateEvent, now_iso8601,
};
use a2a_rs_server::{AuthContext, HandlerResult, MessageHandler};
use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::{
    a2a::card::AgentCardBuilder,
    runtime::Agent,
    session::{AgentStreamEvent, FinishReason},
    types::runtime::AgentBundle,
};

/// A2A 流式事件发送器的共享槽
///
/// 使用 `Arc<OnceLock<...>>` 解决"先有 handler 还是先有 server"的鸡蛋问题：
/// 1. 构造 handler 时创建空槽
/// 2. 将 `Arc` 克隆后移入 handler
/// 3. Server 构造完成后，调用 `get_event_sender()` 填充槽
/// 4. handler 在处理请求时从槽中取出 sender
pub type EventSenderSlot = Arc<OnceLock<broadcast::Sender<StreamResponse>>>;

/// 将 `Arc<dyn Agent>` 适配为 A2A v1.0 `MessageHandler`
///
/// 持有本地 Agent 的引用，在每次 A2A 请求到来时：
/// - 提取 `Message` 中的文本输入
/// - 以 `context_id`（或 `task_id`）作为 `session_id` 调用 `Agent::run_stream`
/// - 将 `AgentStreamEvent` 流映射为 A2A `StreamResponse` 并通过 broadcast 推送
///
/// ## 流式工作模式
///
/// `handle_message` 立即返回 `Working` 状态的 Task，
/// 后台 `tokio::spawn` 任务驱动整个流式生命周期。
pub struct AgentExecAdapter {
    /// 本地 Agent 实例
    agent: Arc<dyn Agent>,
    /// AgentCard（由 bundle 自动生成）
    card: AgentCard,
    /// 流式事件发送器槽（Server 构造后填充）
    event_tx: EventSenderSlot,
}

impl AgentExecAdapter {
    /// 创建适配器
    ///
    /// # Parameters
    ///
    /// * `agent` - 本地 Agent 实例
    /// * `bundle` - 智能体装配包（用于生成 AgentCard）
    /// * `base_url` - 服务的公开基础 URL
    pub fn new(agent: Arc<dyn Agent>, bundle: &AgentBundle, base_url: &str) -> Self {
        let card = AgentCardBuilder::from_bundle(bundle, base_url).build();
        Self {
            agent,
            card,
            event_tx: Arc::new(OnceLock::new()),
        }
    }

    /// 使用自定义 AgentCard 创建适配器
    ///
    /// # Parameters
    ///
    /// * `agent` - 本地 Agent 实例
    /// * `card` - 自定义 AgentCard
    pub fn with_card(agent: Arc<dyn Agent>, card: AgentCard) -> Self {
        Self {
            agent,
            card,
            event_tx: Arc::new(OnceLock::new()),
        }
    }

    /// 获取事件发送器槽的克隆（用于传递给 A2aServer）
    pub fn event_sender_slot(&self) -> EventSenderSlot {
        Arc::clone(&self.event_tx)
    }

    /// 填充事件发送器（由 A2aServer 在构造完成后调用）
    ///
    /// # Parameters
    ///
    /// * `tx` - broadcast sender
    pub fn init_event_sender(&self, tx: broadcast::Sender<StreamResponse>) {
        self.event_tx
            .set(tx)
            .unwrap_or_else(|_| warn!("A2A event sender already initialized"));
    }

    /// 获取事件发送器
    #[allow(dead_code)]
    fn get_event_sender(&self) -> Option<broadcast::Sender<StreamResponse>> {
        self.event_tx.get().cloned()
    }

    /// 从 A2A Message 中提取纯文本输入
    ///
    /// 拼接所有 `Part::Text` 的内容，以换行符分隔。
    fn extract_input(message: &Message) -> String {
        message
            .parts
            .iter()
            .filter_map(|p| p.as_text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 从 A2A Message 中提取 session_id
    ///
    /// 优先使用 `context_id`（跨 task 的持久会话标识），
    /// 其次使用 `task_id`，最后生成新 UUID。
    fn extract_session_id(message: &Message) -> String {
        message
            .context_id
            .clone()
            .or_else(|| message.task_id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string())
    }
}

#[async_trait]
impl MessageHandler for AgentExecAdapter {
    /// 处理 A2A 消息（非流式）
    ///
    /// 同步等待 Agent 完成，直接返回 `Completed` 状态的 Task。
    /// 这样客户端无需轮询，立即拿到最终结果。
    async fn handle_message(
        &self,
        message: Message,
        _auth: Option<AuthContext>,
    ) -> HandlerResult<SendMessageResponse> {
        let input = Self::extract_input(&message);
        let session_id = Self::extract_session_id(&message);

        let task_id = Uuid::new_v4().to_string();
        let context_id = message
            .context_id
            .clone()
            .unwrap_or_else(|| session_id.clone());

        // 同步运行 Agent，收集所有流式事件，提取最终回答
        let stream = match self.agent.run_stream(&session_id, &input) {
            Ok(s) => s,
            Err(e) => {
                error!("A2A: Agent::run_stream failed: {e}");
                return Ok(SendMessageResponse::Task(Task {
                    kind: "task".to_string(),
                    id: task_id.clone(),
                    context_id: context_id.clone(),
                    status: TaskStatus {
                        state: TaskState::Failed,
                        message: Some(Message {
                            kind: "message".to_string(),
                            message_id: Uuid::new_v4().to_string(),
                            context_id: Some(context_id.clone()),
                            task_id: Some(task_id.clone()),
                            role: Role::Agent,
                            parts: vec![Part::text(format!("Error: {e}"))],
                            metadata: None,
                            extensions: vec![],
                            reference_task_ids: None,
                        }),
                        timestamp: Some(now_iso8601()),
                    },
                    history: Some(vec![message]),
                    artifacts: None,
                    metadata: None,
                }));
            }
        };

        // 收集流式事件，提取最终回答文本
        let answer = collect_answer(stream).await;
        let (state, answer_text) = match answer {
            Ok(text) => (TaskState::Completed, text),
            Err(e) => (TaskState::Failed, e),
        };

        let artifact = if state == TaskState::Completed && !answer_text.is_empty() {
            Some(vec![Artifact {
                artifact_id: Uuid::new_v4().to_string(),
                name: Some("answer".to_string()),
                description: None,
                parts: vec![Part::text(&answer_text)],
                metadata: None,
                extensions: vec![],
            }])
        } else {
            None
        };

        Ok(SendMessageResponse::Task(Task {
            kind: "task".to_string(),
            id: task_id.clone(),
            context_id: context_id.clone(),
            status: TaskStatus {
                state,
                message: Some(Message {
                    kind: "message".to_string(),
                    message_id: Uuid::new_v4().to_string(),
                    context_id: Some(context_id.clone()),
                    task_id: Some(task_id.clone()),
                    role: Role::Agent,
                    parts: vec![Part::text(&answer_text)],
                    metadata: None,
                    extensions: vec![],
                    reference_task_ids: None,
                }),
                timestamp: Some(now_iso8601()),
            },
            history: Some(vec![message]),
            artifacts: artifact,
            metadata: None,
        }))
    }

    /// 返回 AgentCard
    fn agent_card(&self, _base_url: &str) -> AgentCard {
        self.card.clone()
    }

    /// 支持流式输出
    fn supports_streaming(&self) -> bool {
        true
    }
}

/// 驱动 `AgentEventStream`，将事件映射为 A2A `StreamResponse` 并广播
///
/// 流式调用（SSE）路径专用，由 `a2a-rs-server` 框架通过 broadcast channel 间接驱动。
///
/// 事件映射规则：
///
/// | `AgentStreamEvent` | A2A 输出 |
/// |---|---|
/// | `RunStarted` | `StatusUpdate { Working }` |
/// | `StepStarted` | `StatusUpdate { Working }` |
/// | `Reasoning` / `ReasoningDelta` | `ArtifactUpdate { name: "reasoning" }` |
/// | `ToolCallStarted` | `StatusUpdate { Working, message: "calling <tool>" }` |
/// | `ToolCallFinished` | `ArtifactUpdate { name: "tool_result_<id>" }` |
/// | `TextDelta` | `ArtifactUpdate { name: "answer", append: true }` |
/// | `Answer` | `ArtifactUpdate { name: "answer", last_chunk: true }` |
/// | `RunFinished { FinalizeAction }` | `StatusUpdate { Completed }` |
/// | `RunFinished { Error }` | `StatusUpdate { Failed }` |
/// | `RunFinished { Cancelled }` | `StatusUpdate { Canceled }` |
/// | `RunFinished { MaxTurns }` | `StatusUpdate { Completed }` |
#[allow(dead_code)]
async fn drive_stream(
    mut stream: crate::session::AgentEventStream,
    event_tx: broadcast::Sender<StreamResponse>,
    task_id: String,
    context_id: String,
) {
    /// 构造一个 Working 状态的 StatusUpdate
    fn working_status(task_id: &str, context_id: &str, text: impl Into<String>) -> StreamResponse {
        StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
            kind: "status-update".to_string(),
            task_id: task_id.to_string(),
            context_id: context_id.to_string(),
            status: TaskStatus {
                state: TaskState::Working,
                message: Some(Message {
                    kind: "message".to_string(),
                    message_id: Uuid::new_v4().to_string(),
                    context_id: Some(context_id.to_string()),
                    task_id: Some(task_id.to_string()),
                    role: Role::Agent,
                    parts: vec![Part::text(text)],
                    metadata: None,
                    extensions: vec![],
                    reference_task_ids: None,
                }),
                timestamp: Some(now_iso8601()),
            },
            metadata: None,
        })
    }

    /// 构造一个终止状态的 StatusUpdate
    fn terminal_status(
        task_id: &str,
        context_id: &str,
        state: TaskState,
        text: impl Into<String>,
    ) -> StreamResponse {
        StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
            kind: "status-update".to_string(),
            task_id: task_id.to_string(),
            context_id: context_id.to_string(),
            status: TaskStatus {
                state,
                message: Some(Message {
                    kind: "message".to_string(),
                    message_id: Uuid::new_v4().to_string(),
                    context_id: Some(context_id.to_string()),
                    task_id: Some(task_id.to_string()),
                    role: Role::Agent,
                    parts: vec![Part::text(text)],
                    metadata: None,
                    extensions: vec![],
                    reference_task_ids: None,
                }),
                timestamp: Some(now_iso8601()),
            },
            metadata: None,
        })
    }

    /// 构造一个 ArtifactUpdate
    fn artifact_update(
        task_id: &str,
        context_id: &str,
        artifact_id: &str,
        name: &str,
        text: impl Into<String>,
        append: Option<bool>,
        last_chunk: Option<bool>,
    ) -> StreamResponse {
        StreamResponse::ArtifactUpdate(TaskArtifactUpdateEvent {
            kind: "artifact-update".to_string(),
            task_id: task_id.to_string(),
            context_id: context_id.to_string(),
            artifact: Artifact {
                artifact_id: artifact_id.to_string(),
                name: Some(name.to_string()),
                description: None,
                parts: vec![Part::text(text)],
                metadata: None,
                extensions: vec![],
            },
            append,
            last_chunk,
            metadata: None,
        })
    }

    // answer artifact 的固定 ID（同一次 run 内所有 TextDelta/Answer 共享同一 artifact）
    let answer_artifact_id = Uuid::new_v4().to_string();
    let mut answer_chunk_index: u32 = 0;

    while let Some(event) = stream.next().await {
        let response = match event {
            AgentStreamEvent::RunStarted { .. } => {
                debug!("A2A: RunStarted");
                working_status(&task_id, &context_id, "Agent started")
            }

            AgentStreamEvent::StepStarted { step, .. } => {
                debug!("A2A: StepStarted step={step}");
                working_status(&task_id, &context_id, format!("Step {step}"))
            }

            AgentStreamEvent::Reasoning { content, .. } => artifact_update(
                &task_id,
                &context_id,
                &Uuid::new_v4().to_string(),
                "reasoning",
                content,
                None,
                Some(true),
            ),

            AgentStreamEvent::ReasoningDelta { delta, .. } => {
                let append = Some(true);
                artifact_update(
                    &task_id,
                    &context_id,
                    &Uuid::new_v4().to_string(),
                    "reasoning",
                    delta,
                    append,
                    None,
                )
            }

            AgentStreamEvent::ToolCallStarted { toolname, .. } => {
                working_status(&task_id, &context_id, format!("Calling tool: {toolname}"))
            }

            AgentStreamEvent::ToolCallFinished {
                toolname,
                call_id,
                result,
                success,
                error_message,
                ..
            } => {
                let content = if success {
                    result.unwrap_or_default()
                } else {
                    error_message.unwrap_or_else(|| "Tool call failed".to_string())
                };
                artifact_update(
                    &task_id,
                    &context_id,
                    &format!("tool_result_{call_id}"),
                    &format!("tool_result_{toolname}"),
                    content,
                    None,
                    Some(true),
                )
            }

            AgentStreamEvent::TextDelta { delta, .. } => {
                let append = if answer_chunk_index > 0 {
                    Some(true)
                } else {
                    None
                };
                answer_chunk_index += 1;
                artifact_update(
                    &task_id,
                    &context_id,
                    &answer_artifact_id,
                    "answer",
                    delta,
                    append,
                    None,
                )
            }

            AgentStreamEvent::Answer { content, .. } => {
                // Answer 是最终完整回答，标记为 last_chunk
                let append = if answer_chunk_index > 0 {
                    Some(true)
                } else {
                    None
                };
                artifact_update(
                    &task_id,
                    &context_id,
                    &answer_artifact_id,
                    "answer",
                    content,
                    append,
                    Some(true),
                )
            }

            AgentStreamEvent::RunFinished {
                finish_reason,
                error_message,
                ..
            } => {
                let (state, text) = match finish_reason {
                    FinishReason::FinalizeAction => (TaskState::Completed, "Completed".to_string()),
                    FinishReason::MaxTurns => (
                        TaskState::Completed,
                        "Completed (max turns reached)".to_string(),
                    ),
                    FinishReason::Error => (
                        TaskState::Failed,
                        error_message.unwrap_or_else(|| "Agent error".to_string()),
                    ),
                    FinishReason::Cancelled => (TaskState::Canceled, "Cancelled".to_string()),
                };
                terminal_status(&task_id, &context_id, state, text)
            }

            // 以下事件不产生 A2A 输出（仅内部使用）
            AgentStreamEvent::UserMessage { .. }
            | AgentStreamEvent::ToolCallsPlanned { .. }
            | AgentStreamEvent::SubAgentEvent { .. } => {
                continue;
            }
        };

        if let Err(e) = event_tx.send(response) {
            // 没有订阅者时 broadcast::send 返回 Err，这是正常情况（非流式调用）
            debug!("A2A: broadcast send skipped (no subscribers): {e}");
        }
    }
}

/// 同步消费 `AgentEventStream`，提取最终回答文本
///
/// 用于非流式调用路径：等待 Agent 完整执行完毕，返回最终回答。
///
/// # Returns
///
/// - `Ok(text)` — Agent 成功完成，`text` 为最终回答
/// - `Err(msg)` — Agent 执行失败，`msg` 为错误信息
async fn collect_answer(mut stream: crate::session::AgentEventStream) -> Result<String, String> {
    let mut answer = String::new();
    let mut finish_reason = FinishReason::FinalizeAction;
    let mut error_message: Option<String> = None;

    while let Some(event) = stream.next().await {
        match event {
            AgentStreamEvent::Answer { content, .. } => {
                answer = content;
            }
            AgentStreamEvent::TextDelta { delta, .. } => {
                // TextDelta 作为备用：如果没有 Answer 事件，拼接 delta
                answer.push_str(&delta);
            }
            AgentStreamEvent::RunFinished {
                finish_reason: reason,
                error_message: err,
                ..
            } => {
                finish_reason = reason;
                error_message = err;
                break;
            }
            _ => {}
        }
    }

    match finish_reason {
        FinishReason::FinalizeAction | FinishReason::MaxTurns => Ok(answer),
        FinishReason::Error => Err(error_message.unwrap_or_else(|| "Agent error".to_string())),
        FinishReason::Cancelled => Err("Agent cancelled".to_string()),
    }
}
