//! 原始单轮 LLM 调用的事件流到 OpenAI Chat Completions 协议产物的适配器
//!
//! 提供的主要内容：
//! - [`OpenAIRawStreamAdapter`]: 将 `Agent::infer_stream` 产出的事件流
//!   翻译为 OpenAI Chat Completions 兼容格式
//!
//! ## 与 [`OpenAIStreamAdapter`](super::stream_adapter::OpenAIStreamAdapter) 的区别
//!
//! | 事件 | `OpenAIStreamAdapter`（完整循环） | `OpenAIRawStreamAdapter`（原始单轮） |
//! |------|------|------|
//! | `ToolCallsPlanned` | → Extension 扩展事件 | → `delta.tool_calls[]` 标准字段 |
//! | `finish_reason` | 始终 `stop` / `length` | 有 tool_calls 时为 `tool_calls` |
//!
//! ## 设计决策
//!
//! raw 模式不执行工具，LLM 返回的 tool_calls 需要原样暴露给调用方，
//! 因此 `ToolCallsPlanned` 必须翻译为 OpenAI 标准的 `delta.tool_calls[]` 字段，
//! 而非扩展事件。这使得调用方可以像使用原生 OpenAI API 一样处理 tool_calls。

use crate::openai::{
    ChatCompletionChunk, ChoiceDelta, ChunkChoice, ChunkFunctionDelta, ChunkToolCallDelta,
    FinishReason as OpenAIFinishReason, Role,
};
use crate::session::AgentStreamEvent;

/// OpenAI 流式 chunk 对象类型字面量
const OBJECT_CHUNK: &str = "chat.completion.chunk";

/// 原始单轮 LLM 调用的协议适配器
///
/// 将 `Agent::infer_stream` 产出的内部事件流翻译为 OpenAI Chat Completions
/// 兼容的流式 chunk 序列。与 [`OpenAIStreamAdapter`](super::stream_adapter::OpenAIStreamAdapter)
/// 的关键区别在于 `ToolCallsPlanned` 事件的处理方式。
///
/// 不可跨 run 复用：每次 Agent 运行需创建一个新实例。
///
/// # Examples
///
/// ```
/// use agent_runtime::openai::OpenAIRawStreamAdapter;
/// use agent_runtime::session::AgentStreamEvent;
///
/// let mut adapter = OpenAIRawStreamAdapter::new("session-1", "gpt-4");
/// let chunks = adapter.translate(AgentStreamEvent::TextDelta {
///     run_id: "run-1".to_string(),
///     delta: "Hello".to_string(),
/// });
/// assert_eq!(chunks.len(), 1);
/// ```
pub struct OpenAIRawStreamAdapter {
    /// chunk.id（一般为 `chatcmpl-<session_id>`）
    chat_id: String,
    /// 模型代码
    model: String,
    /// chunk.created 时间戳（创建时固化，所有 chunk 复用）
    created: u32,
    /// 是否已发出首条带 role 的 chunk
    role_emitted: bool,
    /// tool_calls 索引计数器
    tool_call_index: u32,
}

/// 适配器输出
#[derive(Debug, Clone)]
pub enum RawAdapterOutput {
    /// OpenAI 标准分片
    Chunk(ChatCompletionChunk),
    /// 流结束信号
    Done,
    /// 此事件不需要对外产物
    Skip,
}

impl OpenAIRawStreamAdapter {
    /// 创建适配器
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID，用于生成 `chatcmpl-<session_id>` 形式的 chunk.id
    /// * `model` - 对外暴露的模型代码
    pub fn new(session_id: impl AsRef<str>, model: impl Into<String>) -> Self {
        Self {
            chat_id: format!("chatcmpl-{}", session_id.as_ref()),
            model: model.into(),
            created: chrono::Utc::now().timestamp() as u32,
            role_emitted: false,
            tool_call_index: 0,
        }
    }

    /// 将单个内部事件翻译为适配器输出
    ///
    /// # Parameters
    ///
    /// * `event` - nucleus 内部事件
    ///
    /// # Returns
    ///
    /// 输出列表（一个事件可能产出多个 chunk，如 ToolCallsPlanned 中的多个 tool_calls）
    #[allow(deprecated)]
    pub fn translate(&mut self, event: AgentStreamEvent) -> Vec<RawAdapterOutput> {
        match event {
            AgentStreamEvent::TextDelta { delta, .. } => {
                let chunk = self.content_chunk(delta);
                vec![RawAdapterOutput::Chunk(chunk)]
            }

            AgentStreamEvent::ToolCallsPlanned { calls, .. } => {
                let mut outputs = Vec::with_capacity(calls.len());
                for call in calls {
                    let chunk =
                        self.tool_call_chunk(call.call_id, call.tool_name, call.arguments_json);
                    outputs.push(RawAdapterOutput::Chunk(chunk));
                    self.tool_call_index += 1;
                }
                outputs
            }

            AgentStreamEvent::RunStarted { .. } => {
                self.created = chrono::Utc::now().timestamp() as u32;
                vec![RawAdapterOutput::Skip]
            }

            AgentStreamEvent::RunFinished { .. } => {
                let finish_reason = if self.tool_call_index > 0 {
                    OpenAIFinishReason::ToolCalls
                } else {
                    OpenAIFinishReason::Stop
                };
                vec![
                    RawAdapterOutput::Chunk(self.finish_chunk(finish_reason)),
                    RawAdapterOutput::Done,
                ]
            }

            // All other events are skipped in raw mode
            _ => vec![RawAdapterOutput::Skip],
        }
    }

    /// 是否存在 tool_calls（用于非流式聚合判断 finish_reason）
    pub fn has_tool_calls(&self) -> bool {
        self.tool_call_index > 0
    }

    /// 构造 content chunk
    #[allow(deprecated)]
    fn content_chunk(&mut self, content: String) -> ChatCompletionChunk {
        // Ensure role is emitted on first content chunk
        let role = if !self.role_emitted {
            self.role_emitted = true;
            Some(Role::Assistant)
        } else {
            None
        };

        ChatCompletionChunk {
            id: self.chat_id.clone(),
            object: OBJECT_CHUNK.to_string(),
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChoiceDelta {
                    role,
                    content: Some(content),
                    tool_calls: None,
                    refusal: None,
                    function_call: None,
                },
                finish_reason: None,
                logprobs: None,
            }],
            usage: None,
            system_fingerprint: None,
            service_tier: None,
        }
    }

    /// 构造 tool_call chunk（OpenAI 标准 delta.tool_calls 格式）
    #[allow(deprecated)]
    fn tool_call_chunk(
        &mut self,
        call_id: String,
        name: String,
        arguments: String,
    ) -> ChatCompletionChunk {
        // Ensure role is emitted on first tool_call chunk
        let role = if !self.role_emitted {
            self.role_emitted = true;
            Some(Role::Assistant)
        } else {
            None
        };

        ChatCompletionChunk {
            id: self.chat_id.clone(),
            object: OBJECT_CHUNK.to_string(),
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChoiceDelta {
                    role,
                    content: None,
                    tool_calls: Some(vec![ChunkToolCallDelta {
                        index: self.tool_call_index,
                        id: Some(call_id),
                        function: Some(ChunkFunctionDelta {
                            name: Some(name),
                            arguments: Some(arguments),
                        }),
                        r#type: None,
                    }]),
                    refusal: None,
                    function_call: None,
                },
                finish_reason: None,
                logprobs: None,
            }],
            usage: None,
            system_fingerprint: None,
            service_tier: None,
        }
    }

    /// 构造 finish_reason chunk
    #[allow(deprecated)]
    fn finish_chunk(&self, reason: OpenAIFinishReason) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: self.chat_id.clone(),
            object: OBJECT_CHUNK.to_string(),
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChoiceDelta {
                    role: None,
                    content: None,
                    tool_calls: None,
                    refusal: None,
                    function_call: None,
                },
                finish_reason: Some(reason),
                logprobs: None,
            }],
            usage: None,
            system_fingerprint: None,
            service_tier: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::PlannedToolCall;

    /// TextDelta 应产出带 role 的 content chunk（首次）
    #[test]
    fn test_text_delta_emits_content_with_role() {
        let mut adapter = OpenAIRawStreamAdapter::new("session-1", "gpt-4");
        let out = adapter.translate(AgentStreamEvent::TextDelta {
            run_id: "run-1".to_string(),
            delta: "hello".to_string(),
        });
        assert_eq!(out.len(), 1);
        let RawAdapterOutput::Chunk(c) = &out[0] else {
            panic!("expected Chunk");
        };
        assert_eq!(c.choices[0].delta.role, Some(Role::Assistant));
        assert_eq!(c.choices[0].delta.content.as_deref(), Some("hello"));
    }

    /// 第二个 TextDelta 不应再发 role
    #[test]
    fn test_subsequent_text_delta_skips_role() {
        let mut adapter = OpenAIRawStreamAdapter::new("session-1", "gpt-4");
        let _ = adapter.translate(AgentStreamEvent::TextDelta {
            run_id: "run-1".to_string(),
            delta: "a".to_string(),
        });
        let out = adapter.translate(AgentStreamEvent::TextDelta {
            run_id: "run-1".to_string(),
            delta: "b".to_string(),
        });
        let RawAdapterOutput::Chunk(c) = &out[0] else {
            panic!("expected Chunk");
        };
        assert!(c.choices[0].delta.role.is_none());
        assert_eq!(c.choices[0].delta.content.as_deref(), Some("b"));
    }

    /// ToolCallsPlanned 应映射为标准 delta.tool_calls
    #[test]
    fn test_tool_calls_planned_maps_to_tool_calls_delta() {
        let mut adapter = OpenAIRawStreamAdapter::new("session-1", "gpt-4");
        let out = adapter.translate(AgentStreamEvent::ToolCallsPlanned {
            run_id: "run-1".to_string(),
            step: 1,
            calls: vec![
                PlannedToolCall {
                    call_id: "call_abc".to_string(),
                    tool_name: "search_web".to_string(),
                    arguments_json: "{\"q\":\"rust\"}".to_string(),
                    source: Default::default(),
                },
                PlannedToolCall {
                    call_id: "call_def".to_string(),
                    tool_name: "read_file".to_string(),
                    arguments_json: "{\"path\":\"/tmp\"}".to_string(),
                    source: Default::default(),
                },
            ],
        });
        assert_eq!(out.len(), 2);

        // First tool call
        let RawAdapterOutput::Chunk(c0) = &out[0] else {
            panic!("expected Chunk");
        };
        let tc0 = c0.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(tc0[0].index, 0);
        assert_eq!(tc0[0].id.as_deref(), Some("call_abc"));
        assert_eq!(
            tc0[0].function.as_ref().unwrap().name.as_deref(),
            Some("search_web")
        );

        // Second tool call
        let RawAdapterOutput::Chunk(c1) = &out[1] else {
            panic!("expected Chunk");
        };
        let tc1 = c1.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(tc1[0].index, 1);
        assert_eq!(tc1[0].id.as_deref(), Some("call_def"));
    }

    /// RunFinished 无 tool_calls 时 finish_reason 为 stop
    #[test]
    fn test_run_finished_without_tools_emits_stop() {
        let mut adapter = OpenAIRawStreamAdapter::new("session-1", "gpt-4");
        let out = adapter.translate(AgentStreamEvent::RunFinished {
            run_id: "run-1".to_string(),
            total_steps: 1,
            finish_reason: crate::session::FinishReason::FinalizeAction,
            error_message: None,
            duration_ms: 100,
            usage: None,
        });
        assert_eq!(out.len(), 2);
        let RawAdapterOutput::Chunk(c) = &out[0] else {
            panic!("expected Chunk");
        };
        assert_eq!(c.choices[0].finish_reason, Some(OpenAIFinishReason::Stop));
        assert!(matches!(out[1], RawAdapterOutput::Done));
    }

    /// RunFinished 有 tool_calls 时 finish_reason 为 tool_calls
    #[test]
    fn test_run_finished_with_tools_emits_tool_calls_reason() {
        let mut adapter = OpenAIRawStreamAdapter::new("session-1", "gpt-4");
        // Simulate a tool call first
        let _ = adapter.translate(AgentStreamEvent::ToolCallsPlanned {
            run_id: "run-1".to_string(),
            step: 1,
            calls: vec![PlannedToolCall {
                call_id: "call_abc".to_string(),
                tool_name: "search".to_string(),
                arguments_json: "{}".to_string(),
                source: Default::default(),
            }],
        });
        let out = adapter.translate(AgentStreamEvent::RunFinished {
            run_id: "run-1".to_string(),
            total_steps: 1,
            finish_reason: crate::session::FinishReason::FinalizeAction,
            error_message: None,
            duration_ms: 100,
            usage: None,
        });
        let RawAdapterOutput::Chunk(c) = &out[0] else {
            panic!("expected Chunk");
        };
        assert_eq!(
            c.choices[0].finish_reason,
            Some(OpenAIFinishReason::ToolCalls)
        );
    }

    /// has_tool_calls 正确反映状态
    #[test]
    fn test_has_tool_calls() {
        let mut adapter = OpenAIRawStreamAdapter::new("session-1", "gpt-4");
        assert!(!adapter.has_tool_calls());

        let _ = adapter.translate(AgentStreamEvent::ToolCallsPlanned {
            run_id: "run-1".to_string(),
            step: 1,
            calls: vec![PlannedToolCall {
                call_id: "call_abc".to_string(),
                tool_name: "test".to_string(),
                arguments_json: "{}".to_string(),
                source: Default::default(),
            }],
        });
        assert!(adapter.has_tool_calls());
    }
}
