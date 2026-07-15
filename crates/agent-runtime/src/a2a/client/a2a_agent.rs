//! A2AAgent：将远程 A2A 服务透明代理为本地 Agent
//!
//! `A2AAgent` 实现了 [`Agent`] trait，让调用方可以像使用本地 Agent 一样
//! 使用远程 A2A 服务，无需感知任何协议细节。
//!
//! ## 工作原理
//!
//! ```text
//! 调用方
//!   │ Agent::run(session_id, input)
//!   ▼
//! A2AAgent
//!   │ POST /v1/message:send (A2A JSON-RPC)
//!   ▼
//! 远程 A2A Server
//!   │ 返回 Task / Message
//!   ▼
//! A2AAgent → RunOutcome
//!   │
//!   ▼
//! 调用方
//! ```
//!
//! ## 流式工作原理
//!
//! ```text
//! 调用方
//!   │ Agent::run_stream(session_id, input)
//!   ▼
//! A2AAgent
//!   │ POST /v1/message:stream (SSE)
//!   ▼
//! 远程 A2A Server → SSE 事件流
//!   │ StreamingMessageResult → AgentStreamEvent（反向映射）
//!   ▼
//! 调用方收到 AgentEventStream（与本地 Agent 完全相同）
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use a2a_rs_client::{A2aClient, ClientConfig};
use a2a_rs_core::{
    AgentCard, Message, Part, Role, SendMessageResult, StreamingMessageResult, TaskState,
};
use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use reqwest_012::header::{HeaderName, HeaderValue};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{
    Error,
    error::AgentResult,
    runtime::Agent,
    session::{AgentEventStream, AgentStreamEvent, FinishReason, RunOutcome},
    types::agent_meta::AgentMeta,
};

/// 将远程 A2A 服务透明代理为本地 Agent
///
/// 通过 [`A2AAgent::connect`] 或 [`A2AAgentBuilder`] 构造，
/// 构造时自动从远程服务发现 [`AgentCard`]。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_base::a2a::A2AAgent;
/// use agent_base::Agent;
///
/// # async fn example() -> anyhow::Result<()> {
/// let remote = A2AAgent::connect("http://remote-agent:3000").await?;
/// println!("Connected to: {}", remote.name());
///
/// // 像本地 Agent 一样使用
/// let outcome = remote.run("session-1", "你好").await?;
/// println!("{}", outcome.final_answer);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct A2AAgent {
    /// 远程 Agent 的 AgentCard（服务发现时获取）
    card: AgentCard,
    /// A2A 客户端
    client: Arc<A2aClient>,
    meta: AgentMeta,
}

impl A2AAgent {
    /// 设置代理元数据
    ///
    /// # Parameters
    ///
    /// * `meta` - 代理元数据
    pub fn with_meta(mut self, meta: AgentMeta) -> Self {
        self.meta = meta;
        self
    }

    /// 连接到远程 A2A 服务（自动发现 AgentCard）
    ///
    /// 构造时会向 `<base_url>/.well-known/agent-card.json` 发起请求，
    /// 获取远程 Agent 的元数据。
    ///
    /// # Parameters
    ///
    /// * `base_url` - 远程 A2A 服务的基础 URL（如 `"http://remote-agent:3000"`）
    ///
    /// # Returns
    ///
    /// 连接成功后返回 `A2AAgent` 实例
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agent_base::a2a::A2AAgent;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let agent = A2AAgent::connect("http://localhost:3000").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(base_url: impl Into<String>) -> AgentResult<Self> {
        A2AAgentBuilder::new(base_url).build().await
    }

    /// 获取远程 Agent 的 AgentCard
    pub fn agent_card(&self) -> &AgentCard {
        &self.card
    }
}

#[async_trait]
impl Agent for A2AAgent {
    fn name(&self) -> &str {
        &self.card.name
    }

    fn model(&self) -> &str {
        // 远程 Agent 的模型对调用方不可见
        ""
    }

    /// 非流式调用远程 A2A Agent
    ///
    /// 发送 `message/send` 请求，等待任务完成后返回 `RunOutcome`。
    async fn run(&self, session_id: &str, input: &str) -> AgentResult<RunOutcome> {
        let message = build_message(input, session_id);

        let result = self
            .client
            .send_message(message, None, None)
            .await
            .map_err(|e| Error::Internal(format!("A2A send_message failed: {e}")))?;

        match result {
            SendMessageResult::Task(task) => {
                // 如果任务未完成，轮询直到完成
                let final_task = if task.status.state.is_terminal() {
                    task
                } else {
                    self.client
                        .poll_until_complete(&task.id, None)
                        .await
                        .map_err(|e| Error::Internal(format!("A2A poll failed: {e}")))?
                };

                let answer = extract_answer_from_task(&final_task);
                let success = final_task.status.state == TaskState::Completed;
                Ok(if success {
                    RunOutcome::success(answer, 1)
                } else {
                    RunOutcome::failure(answer, 1)
                })
            }
            SendMessageResult::Message(msg) => {
                let text = msg
                    .parts
                    .iter()
                    .filter_map(|p| p.as_text())
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(RunOutcome::success(text, 1))
            }
        }
    }

    /// 流式调用远程 A2A Agent
    ///
    /// 发送 `message/stream` 请求，将 SSE 事件流映射为 `AgentEventStream`。
    /// 远程 Agent 的 `AgentStreamEvent` 通过 A2A 协议无损传递给调用方。
    fn run_stream(&self, session_id: &str, input: &str) -> AgentResult<AgentEventStream> {
        let message = build_message(input, session_id);
        let client = Arc::clone(&self.client);
        let run_id = Uuid::new_v4().to_string();
        let agent_name = self.card.name.clone();
        let session_id = session_id.to_string();

        // Inject parent context so the child skips session lifecycle management
        // and avoids overwriting the parent's Running status.
        let meta = self.meta.clone();

        let output_stream = stream! {
            // 发出 RunStarted 事件
            yield AgentStreamEvent::RunStarted {
                run_id: run_id.clone(),
                agent_id: 0,
                agent_name: agent_name.clone(),
                session_id: session_id.clone(),
                user_id: String::new(),
                meta
            };

            // 发起流式请求
            let mut sse_stream = match client.send_message_streaming(message, None, None).await {
                Ok(s) => s,
                Err(e) => {
                    let err_msg = format!("A2A stream failed: {e}");
                    yield AgentStreamEvent::RunFinished {
                        run_id: run_id.clone(),
                        total_steps: 0,
                        finish_reason: FinishReason::Error,
                        error_message: Some(err_msg),
                        duration_ms: 0,
                        usage: None,
                    };
                    return;
                }
            };

            let mut answer_buf = String::new();
            let mut step: u32 = 0;
            let mut finish_reason = FinishReason::FinalizeAction;
            let mut error_message: Option<String> = None;

            while let Some(item) = sse_stream.next().await {
                match item {
                    Err(e) => {
                        warn!("A2A SSE stream error: {e}");
                        finish_reason = FinishReason::Error;
                        error_message = Some(format!("Stream error: {e}"));
                        break;
                    }
                    Ok(result) => {
                        match result {
                            StreamingMessageResult::Task(task) => {
                                debug!("A2A: initial task state={:?}", task.status.state);
                                step += 1;
                                yield AgentStreamEvent::StepStarted {
                                    run_id: run_id.clone(),
                                    step,
                                };

                                // 服务端已同步完成（Completed），直接从 artifacts 提取 answer。
                                // 当服务端 handle_message 同步返回 Completed Task 时，
                                // a2a-rs-server 检测到 initial_is_terminal=true 后立即关闭 SSE 流，
                                // 不会再发出任何 ArtifactUpdate 事件，需要在此处提取内容。
                                if task.status.state.is_terminal() {
                                    let text = extract_answer_text(&task);
                                    if !text.is_empty() {
                                        answer_buf = text.clone();
                                        yield AgentStreamEvent::TextDelta {
                                            run_id: run_id.clone(),
                                            delta: text.clone(),
                                        };
                                        yield AgentStreamEvent::Answer {
                                            run_id: run_id.clone(),
                                            content: text,
                                        };
                                    }
                                    finish_reason = if task.status.state == TaskState::Completed {
                                        FinishReason::FinalizeAction
                                    } else {
                                        FinishReason::Error
                                    };
                                    break;
                                }
                            }

                            StreamingMessageResult::StatusUpdate(update) => {
                                match update.status.state {
                                    TaskState::Working => {
                                        // Working 状态：提取 message 文本作为 TextDelta
                                        if let Some(msg) = &update.status.message {
                                            let text = msg.parts.iter()
                                                .filter_map(|p| p.as_text())
                                                .collect::<Vec<_>>()
                                                .join("");
                                            if !text.is_empty() {
                                                debug!("A2A: working status: {text}");
                                            }
                                        }
                                    }
                                    TaskState::Completed => {
                                        finish_reason = FinishReason::FinalizeAction;
                                        break;
                                    }
                                    TaskState::Failed => {
                                        finish_reason = FinishReason::Error;
                                        error_message = update.status.message
                                            .as_ref()
                                            .and_then(|m| {
                                                m.parts.iter()
                                                    .filter_map(|p| p.as_text())
                                                    .next()
                                                    .map(|s| s.to_string())
                                            });
                                        break;
                                    }
                                    TaskState::Canceled => {
                                        finish_reason = FinishReason::Cancelled;
                                        break;
                                    }
                                    _ => {}
                                }
                            }

                            StreamingMessageResult::ArtifactUpdate(update) => {
                                // 将 artifact 内容映射为 TextDelta / Answer
                                let artifact_name = update.artifact.name.as_deref().unwrap_or("");
                                let text = update.artifact.parts.iter()
                                    .filter_map(|p| p.as_text())
                                    .collect::<Vec<_>>()
                                    .join("");

                                if !text.is_empty()
                                    && artifact_name == "answer" {
                                        answer_buf.push_str(&text);
                                        yield AgentStreamEvent::TextDelta {
                                            run_id: run_id.clone(),
                                            delta: text,
                                        };
                                    }
                                    // 其他 artifact（reasoning、tool_result 等）暂时忽略

                                // last_chunk = true 时发出 Answer 事件
                                if update.last_chunk == Some(true) && artifact_name == "answer" {
                                    yield AgentStreamEvent::Answer {
                                        run_id: run_id.clone(),
                                        content: answer_buf.clone(),
                                    };
                                }
                            }

                            StreamingMessageResult::Message(msg) => {
                                // 直接消息（非 Task 模式）
                                let text = msg.parts.iter()
                                    .filter_map(|p| p.as_text())
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                if !text.is_empty() {
                                    answer_buf = text.clone();
                                    yield AgentStreamEvent::Answer {
                                        run_id: run_id.clone(),
                                        content: text,
                                    };
                                }
                                finish_reason = FinishReason::FinalizeAction;
                                break;
                            }
                        }
                    }
                }
            }

            // 如果有 answer 但没有发出 Answer 事件，补发一次
            if !answer_buf.is_empty() && matches!(finish_reason, FinishReason::FinalizeAction) {
                // Answer 事件已在 ArtifactUpdate 处理中发出，此处不重复
            }

            yield AgentStreamEvent::RunFinished {
                run_id: run_id.clone(),
                total_steps: step,
                finish_reason,
                error_message,
                duration_ms: 0,
                usage: None,
            };
        };

        Ok(Box::pin(output_stream))
    }
}

/// A2AAgent 构造器
///
/// 提供链式 API 配置连接参数，最后调用 `build()` 完成服务发现。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_base::a2a::client::a2a_agent::A2AAgentBuilder;
///
/// # async fn example() -> anyhow::Result<()> {
/// let agent = A2AAgentBuilder::new("http://localhost:3000")
///     .with_timeout(std::time::Duration::from_secs(30))
///     .with_header("Authorization", "Bearer token")
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct A2AAgentBuilder {
    base_url: String,
    timeout: Duration,
    max_polls: u32,
    poll_interval_ms: u64,
    /// 自定义 HTTP 请求头
    headers: HashMap<String, String>,
}

impl A2AAgentBuilder {
    /// 创建构造器
    ///
    /// # Parameters
    ///
    /// * `base_url` - 远程 A2A 服务的基础 URL
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout: Duration::from_secs(60),
            max_polls: 30,
            poll_interval_ms: 2000,
            headers: HashMap::new(),
        }
    }

    /// 设置请求超时时间
    ///
    /// # Parameters
    ///
    /// * `timeout` - 超时时间
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 设置轮询最大次数（非流式调用时使用）
    ///
    /// # Parameters
    ///
    /// * `max_polls` - 最大轮询次数
    pub fn with_max_polls(mut self, max_polls: u32) -> Self {
        self.max_polls = max_polls;
        self
    }

    /// 设置轮询间隔（毫秒）
    ///
    /// # Parameters
    ///
    /// * `interval_ms` - 轮询间隔（毫秒）
    pub fn with_poll_interval_ms(mut self, interval_ms: u64) -> Self {
        self.poll_interval_ms = interval_ms;
        self
    }

    /// 添加单个自定义 HTTP 请求头
    ///
    /// # Parameters
    ///
    /// * `name` - 请求头名称（如 `"Authorization"`）
    /// * `value` - 请求头值（如 `"Bearer token"`）
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// 批量设置自定义 HTTP 请求头
    ///
    /// # Parameters
    ///
    /// * `headers` - 请求头映射
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers.extend(headers);
        self
    }

    /// 构建 A2AAgent（执行服务发现）
    ///
    /// 向远程服务发起 AgentCard 发现请求，成功后返回 `A2AAgent`。
    ///
    /// # Returns
    ///
    /// 连接成功后返回 `A2AAgent` 实例
    pub async fn build(self) -> AgentResult<A2AAgent> {
        let mut header_map = reqwest_012::header::HeaderMap::new();
        for (name, value) in &self.headers {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| Error::Internal(format!("Invalid A2A header name '{name}': {e}")))?;
            let header_value = HeaderValue::from_str(value).map_err(|e| {
                Error::Internal(format!("Invalid A2A header value for '{name}': {e}"))
            })?;
            header_map.insert(header_name, header_value);
        }

        // Build custom reqwest client (compatible with a2a-rs-client) with headers and timeout
        let mut builder = reqwest_012::Client::builder().timeout(self.timeout);
        if !header_map.is_empty() {
            builder = builder.default_headers(header_map);
        }
        let http_client = builder
            .build()
            .map_err(|e| Error::Internal(format!("Failed to build A2A HTTP client: {e}")))?;

        let config = ClientConfig {
            server_url: self.base_url,
            max_polls: self.max_polls,
            poll_interval_ms: self.poll_interval_ms,
            oauth: None,
            http_client: Some(http_client),
            ..Default::default()
        };

        let client = A2aClient::new(config)
            .map_err(|e| Error::Internal(format!("A2A client init failed: {e}")))?;

        let card = client
            .fetch_agent_card()
            .await
            .map_err(|e| Error::Internal(format!("A2A agent card fetch failed: {e}")))?;

        Ok(A2AAgent {
            card,
            client: Arc::new(client),
            meta: AgentMeta::primary(),
        })
    }
}

// ─── 辅助函数 ──────────────────────────────────────

/// 构造 A2A Message
fn build_message(input: &str, session_id: &str) -> Message {
    Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: Role::User,
        parts: vec![Part::text(input)],
        context_id: Some(session_id.to_string()),
        task_id: None,
        extensions: vec![],
        reference_task_ids: None,
        metadata: None,
    }
}

/// 从 Task 中提取最终回答文本
///
/// 优先从 artifacts 中提取 "answer" artifact 的文本，
/// 其次从 history 中提取最后一条 Agent 消息的文本。
fn extract_answer_from_task(task: &a2a_rs_core::Task) -> String {
    extract_answer_text(task)
}

/// 从 Task 中提取回答文本（内部实现）
///
/// 优先从 `artifacts["answer"]` 提取，其次从 `status.message`，
/// 最后从 `history` 中最后一条 Agent 消息提取。
fn extract_answer_text(task: &a2a_rs_core::Task) -> String {
    // 优先从 artifacts 中提取
    if let Some(artifacts) = &task.artifacts {
        for artifact in artifacts {
            if artifact.name.as_deref() == Some("answer") {
                let text = artifact
                    .parts
                    .iter()
                    .filter_map(|p| p.as_text())
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    return text;
                }
            }
        }
        // 如果没有 "answer" artifact，取第一个 artifact 的文本
        if let Some(first) = artifacts.first() {
            let text = first
                .parts
                .iter()
                .filter_map(|p| p.as_text())
                .collect::<Vec<_>>()
                .join("");
            if !text.is_empty() {
                return text;
            }
        }
    }

    // 从 status.message 提取
    if let Some(msg) = &task.status.message {
        let text = msg
            .parts
            .iter()
            .filter_map(|p| p.as_text())
            .collect::<Vec<_>>()
            .join("");
        if !text.is_empty() {
            return text;
        }
    }

    // 从 history 中提取最后一条 Agent 消息
    if let Some(history) = &task.history {
        for msg in history.iter().rev() {
            if msg.role == Role::Agent {
                let text = msg
                    .parts
                    .iter()
                    .filter_map(|p| p.as_text())
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    return text;
                }
            }
        }
    }

    String::new()
}
