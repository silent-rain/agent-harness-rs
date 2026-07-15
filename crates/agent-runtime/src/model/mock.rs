//! 测试辅助：公共 [`MockModel`] 与 [`completion_to_chunks`]
//!
//! 仅在测试代码中使用，提供：
//! - [`MockModel`]：覆盖 crate 内所有单测场景的 `ModelAdapter` 实现，
//!   通过 [`MockBehavior`] 三种预设行为参数化，无需各测试再手写 mock。
//! - [`completion_to_chunks`]：把一次性 `CompletionResponse` 折叠为
//!   [`StreamChunk`] 序列，让任意 mock 即便不实现真流式也能补齐
//!   [`ModelAdapter::stream_completion`]。
//!
//! 注意：`completion_to_chunks` 产生的是"伪流式"（所有 chunk 在同一时刻就绪），
//! 切勿在生产代码中使用。

use async_trait::async_trait;
use rig_core::OneOrMany;
use rig_core::completion::message::{
    AssistantContent, Message, Text, ToolCall, ToolFunction, UserContent,
};
use rig_core::completion::{CompletionRequest, CompletionResponse};

use crate::{
    error::AgentResult,
    model::{
        ModelAdapter,
        streaming::{BoxedCompletionStream, StreamChunk, conv},
    },
};

/// [`MockModel`] 的行为预设
///
/// 通过此枚举消除"每个单测都临时实现一个 `ModelAdapter`"的样板代码。
pub enum MockBehavior {
    /// 不应被调用的占位实现，调用即 `panic!`
    ///
    /// 适用于只需要把 `Arc<dyn ModelAdapter>` 注入构造函数、
    /// 但运行路径不会触达模型调用的测试（如策略路由层、工具构造测试）。
    Unreachable,

    /// 把 `chat_history` 中最后一条 user 文本回显为 `"Echo: {text}"`
    Echo,

    /// 永远以指定 `name` + `arguments` 产出一个 `ToolCall`
    ///
    /// 适用于模拟"无限循环"或"始终触发某工具"的场景。
    AlwaysToolCall {
        /// 工具调用名（写入 `ToolFunction::name`）
        tool_name: String,
        /// 工具调用参数（写入 `ToolFunction::arguments`）
        arguments: serde_json::Value,
    },
}

/// 公共测试用 [`ModelAdapter`] 实现
///
/// 取代 crate 内分散的 `EchoModel` / `LoopModel` / `DummyModel`
/// / `MockModelAdapter` 等临时桩，避免每个测试文件重复实现 trait。
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use agent_runtime::model::{MockModel, ModelAdapter};
///
/// // 1. 路径不应触达模型：占位即可
/// let model: Arc<dyn ModelAdapter> = Arc::new(MockModel::new());
///
/// // 2. Echo 行为：把最后一条 user 文本回显
/// let model: Arc<dyn ModelAdapter> = Arc::new(MockModel::echo());
///
/// // 3. 永远触发某工具调用（用于触发 max_turns 等异常路径）
/// let model: Arc<dyn ModelAdapter> = Arc::new(MockModel::always_tool_call(
///     "update_plan",
///     serde_json::json!({ "plan": "继续循环" }),
/// ));
/// ```
pub struct MockModel {
    behavior: MockBehavior,
}

impl Default for MockModel {
    fn default() -> Self {
        Self {
            behavior: MockBehavior::Unreachable,
        }
    }
}

impl MockModel {
    /// 创建 `Unreachable` 模式的 mock（调用即 panic）
    pub fn new() -> Self {
        Default::default()
    }

    /// 创建 Echo 模式的 mock
    pub fn echo() -> Self {
        Self {
            behavior: MockBehavior::Echo,
        }
    }

    /// 创建 AlwaysToolCall 模式的 mock
    ///
    /// # Parameters
    ///
    /// * `tool_name` - 工具名
    /// * `arguments` - 工具参数（JSON）
    pub fn always_tool_call(tool_name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            behavior: MockBehavior::AlwaysToolCall {
                tool_name: tool_name.into(),
                arguments,
            },
        }
    }

    /// 从 `chat_history` 中提取最后一条 User 文本
    fn last_user_text(request: &CompletionRequest) -> String {
        request
            .chat_history
            .iter()
            .filter_map(|m| match m {
                Message::User { content, .. } => content.iter().find_map(|c| match c {
                    UserContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                }),
                _ => None,
            })
            .last()
            .unwrap_or_else(|| "无输入".to_string())
    }
}

#[async_trait]
impl ModelAdapter for MockModel {
    async fn completion(&self, request: CompletionRequest) -> AgentResult<CompletionResponse<()>> {
        let choice = match &self.behavior {
            MockBehavior::Unreachable => {
                unreachable!("MockModel::new() 用于不应触达模型的测试路径，但被调用了")
            }
            MockBehavior::Echo => {
                let text = format!("Echo: {}", Self::last_user_text(&request));
                OneOrMany::one(AssistantContent::Text(Text::new(text)))
            }
            MockBehavior::AlwaysToolCall {
                tool_name,
                arguments,
            } => OneOrMany::one(AssistantContent::ToolCall(ToolCall::new(
                format!("call_{tool_name}"),
                ToolFunction::new(tool_name.clone(), arguments.clone()),
            ))),
        };

        Ok(CompletionResponse {
            choice,
            usage: Default::default(),
            raw_response: (),
            message_id: None,
        })
    }

    async fn stream_completion(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<BoxedCompletionStream> {
        // 复用非流式响应转伪流式，避免重复实现。
        let response = self.completion(request).await?;
        let chunks = completion_to_chunks(response);
        Ok(Box::pin(futures::stream::iter(chunks.into_iter().map(Ok))))
    }
}

/// 把一次性 [`CompletionResponse`] 折叠成等价的 [`StreamChunk`] 序列。
///
/// # Parameters
///
/// * `response` - 类型擦除后的完成响应（`raw_response = ()`）
///
/// # Returns
///
/// 与原响应语义等价的 chunk 列表（末尾带 `Done`）
fn completion_to_chunks(response: CompletionResponse<()>) -> Vec<StreamChunk> {
    let mut chunks = Vec::new();
    if let Some(message_id) = response.message_id {
        chunks.push(StreamChunk::MessageId(message_id));
    }
    for content in response.choice {
        match content {
            AssistantContent::Text(text) if !text.text.is_empty() => {
                chunks.push(StreamChunk::TextDelta(text.text));
            }
            AssistantContent::ToolCall(tool_call) => {
                chunks.push(StreamChunk::ToolCall {
                    id: tool_call.id,
                    internal_call_id: uuid::Uuid::new_v4().to_string(),
                    name: tool_call.function.name,
                    arguments: tool_call.function.arguments,
                });
            }
            AssistantContent::Reasoning(reasoning) => {
                let (id, text) = conv::flatten_reasoning(&reasoning);
                chunks.push(StreamChunk::Reasoning { id, text });
            }
            // 非穷尽枚举：图片等其他模态在文本/工具决策语义中无法直接映射，
            // 这里整体忽略（不影响后续工具调用解析与文本输出）。
            _ => {}
        }
    }
    chunks.push(StreamChunk::Done {
        usage: Some(response.usage),
    });
    chunks
}
