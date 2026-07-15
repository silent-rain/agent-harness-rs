//! 内部事件流到 OpenAI Chat Completions 协议产物的单向适配器
//!
//! 提供的主要内容：
//! - [`OpenAIStreamAdapter`][]: 单 run 适配器，持有 chat_id / model / role 已发标志等
//! - [`AdapterOutput`][]: 适配器输出枚举
//!
//! ## 输出双通道
//!
//! - **主流**：[`AdapterOutput::Chunk`]，对齐 OpenAI Chat Completions stream 主体
//! - **扩展事件**：[`AdapterOutput::Extension`]，承载 step / tool_call_started /
//!   rag 等 nucleus 自有运维信号；调用方可决定是否对外透出
//! - **结束**：[`AdapterOutput::Done`]，对应 SSE 最后的 `data: [DONE]`
//! - **跳过**：[`AdapterOutput::Skip`]，事件被消化但无对外产物
//!
//! ## 设计决策
//!
//! - **TextDelta** → `delta.content`（OpenAI 官方文本流字段）
//! - **ReasoningDelta** → Skip（`async-openai` 标准类型暂不支持 `reasoning_content`）
//! - **ToolCallDelta** → `delta.tool_calls[]`（OpenAI 官方工具流字段）
//! - **AnswerDelta** → 默认 **Skip**（避免与 TextDelta 重复）
//! - **RunFinished** → 末尾 `finish_reason` chunk + `Done`

use std::collections::HashMap;

use serde::Serialize;
use serde_json::json;

use crate::openai::{
    ChatCompletionChunk, ChoiceDelta, ChunkChoice, FinishReason as OpenAIFinishReason, Role,
};
use crate::session::{AgentStreamEvent, FinishReason as InternalFinishReason};

/// OpenAI 流式 chunk 对象类型字面量
const OBJECT_CHUNK: &str = "chat.completion.chunk";

/// nucleus 扩展事件 SSE 名称前缀
pub const NUCLEUS_EVENT_PREFIX: &str = "nucleus";

/// 适配器输出
#[derive(Debug, Clone, PartialEq)]
pub enum AdapterOutput {
    /// OpenAI 标准分片，写入 SSE 主流（`data: {chunk}`）
    Chunk(ChatCompletionChunk),
    /// nucleus 扩展事件（写入 SSE `event: <name>` + `data: <payload>`）
    Extension {
        /// 事件名（如 `nucleus.tool_call.started`）
        event_name: String,
        /// 事件载荷（任意 JSON）
        payload: serde_json::Value,
    },
    /// 流结束信号（写入 `data: [DONE]`）
    Done,
    /// 此事件不需要对外产物（如 `RunStarted` 仅初始化适配器内部状态）
    Skip,
}

/// 单 run 的协议适配器
///
/// 不可跨 run 复用：每次 Agent 运行需创建一个新实例。
///
/// # Examples
///
/// ```
/// use agent_base::openai::OpenAIStreamAdapter;
/// use agent_base::session::AgentStreamEvent;
///
/// let mut adapter = OpenAIStreamAdapter::new("run-1", "gpt-4");
/// let outputs = adapter.translate(AgentStreamEvent::TextDelta {
///     run_id: "run-1".to_string(),
///     delta: "你好".to_string(),
/// });
/// // outputs[0] = role chunk（首次自动注入）
/// // outputs[1] = content chunk
/// assert_eq!(outputs.len(), 2);
/// ```
pub struct OpenAIStreamAdapter {
    /// chunk.id（一般为 `chatcmpl-<run_id>`）
    chat_id: String,
    /// 模型代码
    model: String,
    /// chunk.created 时间戳（创建时固化，所有 chunk 复用）
    created: u32,
    /// 是否已发出首条带 role 的 chunk
    role_emitted: bool,
}

impl OpenAIStreamAdapter {
    /// 创建适配器
    ///
    /// # Parameters
    ///
    /// * `run_id` - Agent 运行 ID，用于生成 `chatcmpl-<run_id>` 形式的 chunk.id
    /// * `model` - 对外暴露的模型代码（一般为 agent_config.model_config.model_code）
    pub fn new(run_id: impl AsRef<str>, model: impl Into<String>) -> Self {
        Self {
            chat_id: format!("chatcmpl-{}", run_id.as_ref()),
            model: model.into(),
            created: chrono::Utc::now().timestamp() as u32,
            role_emitted: false,
        }
    }

    /// 将单个内部事件翻译为 0..=N 个 OpenAI 输出
    ///
    /// # Parameters
    ///
    /// * `event` - nucleus 内部事件
    ///
    /// # Returns
    ///
    /// 输出数组，长度可能为 0（Skip 时返回空 vec 也合法，但当前实现使用 [`AdapterOutput::Skip`]）
    pub fn translate(&mut self, event: AgentStreamEvent) -> Vec<AdapterOutput> {
        match event {
            AgentStreamEvent::RunStarted { .. } | AgentStreamEvent::UserMessage { .. } => {
                vec![AdapterOutput::Skip]
            }

            AgentStreamEvent::TextDelta { delta, .. } => {
                let mut outputs = Vec::with_capacity(2);
                if !self.role_emitted {
                    outputs.push(AdapterOutput::Chunk(self.role_chunk()));
                    self.role_emitted = true;
                }
                outputs.push(AdapterOutput::Chunk(self.content_chunk(delta)));
                outputs
            }

            // Reasoning 事件在标准 async-openai 类型中无对应字段，暂 Skip
            AgentStreamEvent::Reasoning { .. } | AgentStreamEvent::ReasoningDelta { .. } => {
                vec![AdapterOutput::Skip]
            }

            AgentStreamEvent::ToolCallsPlanned { step, calls, .. } => {
                let serialised = serde_json::to_value(&calls).unwrap_or(serde_json::Value::Null);
                vec![extension(
                    "tool_calls.planned",
                    json!({
                        "step": step,
                        "calls": serialised,
                    }),
                )]
            }

            // Answer 是整段终答，已被 TextDelta 流覆盖，避免重复对外发出
            AgentStreamEvent::Answer { .. } => vec![AdapterOutput::Skip],

            AgentStreamEvent::RunFinished { finish_reason, .. } => {
                vec![
                    AdapterOutput::Chunk(self.finish_chunk(map_finish_reason(finish_reason))),
                    AdapterOutput::Done,
                ]
            }

            AgentStreamEvent::StepStarted { step, .. } => {
                vec![extension("step.started", json!({ "step": step }))]
            }

            AgentStreamEvent::ToolCallStarted {
                step,
                toolname,
                call_id,
                arguments_json,
                ..
            } => vec![extension(
                "tool_call.started",
                json!({
                    "step": step,
                    "toolname": toolname,
                    "call_id": call_id,
                    "arguments": arguments_json,
                }),
            )],

            AgentStreamEvent::ToolCallFinished {
                step,
                toolname,
                call_id,
                success,
                result,
                error_message,
                duration_ms,
                ..
            } => vec![extension(
                "tool_call.finished",
                json!({
                    "step": step,
                    "toolname": toolname,
                    "call_id": call_id,
                    "success": success,
                    "result": result,
                    "error_message": error_message,
                    "duration_ms": duration_ms,
                }),
            )],

            AgentStreamEvent::SubAgentEvent {
                delegate_target,
                delegate_type,
                event,
                ..
            } => {
                // 子智能体事件包装为单个扩展事件，调用方按需展开
                let inner = serde_json::to_value(&*event).unwrap_or(serde_json::Value::Null);
                vec![extension(
                    "sub_agent.event",
                    json!({
                        "delegate_target": delegate_target,
                        "delegate_type": delegate_type,
                        "event": inner,
                    }),
                )]
            }
        }
    }

    /// 构造仅含 role 的首条 chunk
    #[allow(deprecated)]
    fn role_chunk(&self) -> ChatCompletionChunk {
        self.base_chunk(ChoiceDelta {
            role: Some(Role::Assistant),
            content: None,
            function_call: None,
            tool_calls: None,
            refusal: None,
        })
    }

    /// 构造 content chunk
    #[allow(deprecated)]
    fn content_chunk(&self, content: String) -> ChatCompletionChunk {
        self.base_chunk(ChoiceDelta {
            content: Some(content),
            role: None,
            function_call: None,
            tool_calls: None,
            refusal: None,
        })
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
                    function_call: None,
                    tool_calls: None,
                    refusal: None,
                },
                finish_reason: Some(reason),
                logprobs: None,
            }],
            usage: None,
            service_tier: None,
            system_fingerprint: None,
        }
    }

    /// 构造一个填入 delta 的标准 chunk（finish_reason = None）
    #[allow(deprecated)]
    fn base_chunk(&self, delta: ChoiceDelta) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: self.chat_id.clone(),
            object: OBJECT_CHUNK.to_string(),
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta,
                finish_reason: None,
                logprobs: None,
            }],
            usage: None,
            service_tier: None,
            system_fingerprint: None,
        }
    }
}

/// 构造扩展事件输出（自动加 `nucleus.` 前缀）
fn extension(suffix: &str, payload: serde_json::Value) -> AdapterOutput {
    AdapterOutput::Extension {
        event_name: format!("{}.{}", NUCLEUS_EVENT_PREFIX, suffix),
        payload,
    }
}

/// 内部 FinishReason 映射到 OpenAI FinishReason
fn map_finish_reason(internal: InternalFinishReason) -> OpenAIFinishReason {
    match internal {
        InternalFinishReason::FinalizeAction => OpenAIFinishReason::Stop,
        InternalFinishReason::MaxTurns => OpenAIFinishReason::Length,
        InternalFinishReason::Error => OpenAIFinishReason::Stop,
        InternalFinishReason::Cancelled => OpenAIFinishReason::Stop,
    }
}

// 维持类型导入路径稳定，便于上层 use 语句一次性引入
#[allow(dead_code)]
fn _phantom_serialize<T: Serialize>(_t: &T) {}

/// 跨 chunk tool_calls 索引映射的预留扩展点
///
/// 当前 ModelCaller 已为每个 internal_call_id 分配稳定 index，
/// 适配器无需额外维护映射；该类型仅为未来扩展（如多 choice）保留占位。
#[derive(Debug, Default)]
pub struct ToolCallIndexTable(HashMap<String, u32>);

impl ToolCallIndexTable {
    /// 创建空表
    pub fn new() -> Self {
        Self::default()
    }

    /// 查询或分配 index
    pub fn assign(&mut self, key: &str) -> u32 {
        if let Some(idx) = self.0.get(key) {
            return *idx;
        }
        let idx = self.0.len() as u32;
        self.0.insert(key.to_string(), idx);
        idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 首个 TextDelta 应自动产出 role chunk + content chunk
    #[test]
    fn test_text_delta_emits_role_then_content() {
        let mut adapter = OpenAIStreamAdapter::new("run-1", "gpt-4");
        let out = adapter.translate(AgentStreamEvent::TextDelta {
            run_id: "run-1".to_string(),
            delta: "hello".to_string(),
        });
        assert_eq!(out.len(), 2);

        let AdapterOutput::Chunk(c0) = &out[0] else {
            panic!("expected Chunk");
        };
        assert_eq!(c0.choices[0].delta.role, Some(Role::Assistant));
        assert!(c0.choices[0].delta.content.is_none());

        let AdapterOutput::Chunk(c1) = &out[1] else {
            panic!("expected Chunk");
        };
        assert_eq!(c1.choices[0].delta.content.as_deref(), Some("hello"));
        assert!(c1.choices[0].delta.role.is_none());
    }

    /// 第二个 TextDelta 不应再发 role chunk
    #[test]
    fn test_subsequent_text_delta_skips_role() {
        let mut adapter = OpenAIStreamAdapter::new("run-1", "gpt-4");
        let _ = adapter.translate(AgentStreamEvent::TextDelta {
            run_id: "run-1".to_string(),
            delta: "a".to_string(),
        });
        let out = adapter.translate(AgentStreamEvent::TextDelta {
            run_id: "run-1".to_string(),
            delta: "b".to_string(),
        });
        assert_eq!(out.len(), 1);
        let AdapterOutput::Chunk(c) = &out[0] else {
            panic!("expected Chunk");
        };
        assert_eq!(c.choices[0].delta.content.as_deref(), Some("b"));
    }

    /// ReasoningDelta 在标准 async-openai 类型下应被 Skip
    #[test]
    fn test_reasoning_delta_is_skipped() {
        let mut adapter = OpenAIStreamAdapter::new("run-1", "deepseek-r1");
        let out = adapter.translate(AgentStreamEvent::ReasoningDelta {
            run_id: "run-1".to_string(),
            id: None,
            delta: "thinking...".to_string(),
        });
        assert_eq!(out, vec![AdapterOutput::Skip]);
    }

    /// ToolCallsPlanned 应映射为 nucleus.tool_calls.planned 扩展事件
    #[test]
    fn test_tool_calls_planned_maps_to_extension() {
        use crate::session::PlannedToolCall;
        let mut adapter = OpenAIStreamAdapter::new("run-1", "gpt-4");
        let out = adapter.translate(AgentStreamEvent::ToolCallsPlanned {
            run_id: "run-1".to_string(),
            step: 1,
            calls: vec![PlannedToolCall {
                call_id: "call_abc".to_string(),
                tool_name: "search_web".to_string(),
                arguments_json: "{\"q\":\"x\"}".to_string(),
                source: Default::default(),
            }],
        });
        match &out[0] {
            AdapterOutput::Extension {
                event_name,
                payload,
            } => {
                assert_eq!(event_name, "nucleus.tool_calls.planned");
                assert_eq!(payload["step"].as_u64(), Some(1));
                assert_eq!(payload["calls"].as_array().map(|a| a.len()), Some(1));
            }
            _ => panic!("expected Extension"),
        }
    }

    /// Answer 应被跳过（避免与 TextDelta 重复）
    #[test]
    fn test_answer_is_skipped() {
        let mut adapter = OpenAIStreamAdapter::new("run-1", "gpt-4");
        let out = adapter.translate(AgentStreamEvent::Answer {
            run_id: "run-1".to_string(),
            content: "final answer".to_string(),
        });
        assert_eq!(out, vec![AdapterOutput::Skip]);
    }

    /// RunFinished 应产出 finish_reason chunk + Done
    #[test]
    fn test_run_finished_emits_finish_chunk_and_done() {
        let mut adapter = OpenAIStreamAdapter::new("run-1", "gpt-4");
        let out = adapter.translate(AgentStreamEvent::RunFinished {
            run_id: "run-1".to_string(),
            total_steps: 3,
            finish_reason: InternalFinishReason::FinalizeAction,
            error_message: None,
            duration_ms: 0,
            usage: None,
        });
        assert_eq!(out.len(), 2);
        let AdapterOutput::Chunk(c) = &out[0] else {
            panic!("expected Chunk");
        };
        assert_eq!(c.choices[0].finish_reason, Some(OpenAIFinishReason::Stop));
        assert_eq!(out[1], AdapterOutput::Done);
    }

    /// MaxTurns 内部原因应映射为 OpenAI length
    #[test]
    fn test_max_turns_maps_to_length() {
        assert_eq!(
            map_finish_reason(InternalFinishReason::MaxTurns),
            OpenAIFinishReason::Length
        );
    }

    /// StepStarted 应映射为 nucleus.step.started 扩展事件
    #[test]
    fn test_step_started_maps_to_extension() {
        let mut adapter = OpenAIStreamAdapter::new("run-1", "gpt-4");
        let out = adapter.translate(AgentStreamEvent::StepStarted {
            run_id: "run-1".to_string(),
            step: 1,
        });
        match &out[0] {
            AdapterOutput::Extension {
                event_name,
                payload,
            } => {
                assert_eq!(event_name, "nucleus.step.started");
                assert_eq!(payload["step"].as_u64(), Some(1));
            }
            _ => panic!("expected Extension"),
        }
    }
}
