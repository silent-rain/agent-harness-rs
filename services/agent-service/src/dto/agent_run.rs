//! 智能体运行请求/响应数据传输对象

use agent_base::types::runtime::{AgentDefinition, ModelConfig};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 运行请求 ─────────────────────────────────────────────────────────────────

/// 智能体运行请求
///
/// 用于 `POST /agent/run` 和 `POST /agent/run/stream`。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AgentRunReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// 会话 ID（为空时自动生成）
    pub session_id: Option<String>,
    /// 用户输入
    #[validate(length(min = 1, max = 10000, message = "用户输入不能为空且不超过 10000 字符"))]
    pub input: String,
    /// 调试模式（输出更详细的步骤信息）
    #[serde(default)]
    pub debug: bool,
}

/// 调试运行请求
///
/// 用于 `POST /agent/debug/run`，强制开启调试模式。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AgentDebugRunReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// 会话 ID（为空时自动生成）
    pub session_id: Option<String>,
    /// 用户输入
    #[validate(length(min = 1, max = 10000, message = "用户输入不能为空且不超过 10000 字符"))]
    pub input: String,
}

/// 通用配置运行请求
///
/// 用于 `POST /agent/config/run` 和 `POST /agent/config/run/stream`。
/// agent_config 与 model_config 直接从请求中传入，不从数据库加载。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AgentConfigRunReq {
    /// 智能体配置（身份信息）
    pub agent_definition: AgentDefinition,
    /// 模型配置（供应商、模型编码、API Key 等）
    pub model_config: ModelConfig,
    /// 会话 ID（为空时自动生成）
    pub session_id: Option<String>,
    /// 用户输入
    #[validate(length(min = 1, max = 10000, message = "用户输入不能为空且不超过 10000 字符"))]
    pub input: String,
}

// ─── 运行响应 ─────────────────────────────────────────────────────────────────

/// 同步运行响应
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentRunResp {
    /// 运行 ID
    pub run_id: String,
    /// 会话 ID
    pub session_id: String,
    /// 最终回答
    pub answer: String,
    /// 总步骤数
    pub total_steps: u32,
    /// 是否成功
    pub success: bool,
}

/// SSE 流式事件包装
///
/// 将 `AgentStreamEvent` 序列化为 SSE 事件格式。
#[derive(Debug, Serialize, Deserialize)]
pub struct SseEventData {
    /// 事件类型（对应 SSE `event:` 字段）
    pub event: String,
    /// 事件数据（JSON 序列化的 `AgentStreamEvent`）
    pub data: serde_json::Value,
}

impl SseEventData {
    /// 从 `AgentStreamEvent` 构建 SSE 事件数据
    pub fn from_agent_event(event: &agent_base::AgentStreamEvent) -> Self {
        let event_type = match event {
            agent_base::AgentStreamEvent::RunStarted { .. } => "run_started",
            agent_base::AgentStreamEvent::StepStarted { .. } => "step_started",
            agent_base::AgentStreamEvent::Reasoning { .. } => "reasoning",
            agent_base::AgentStreamEvent::ReasoningDelta { .. } => "reasoning_delta",
            agent_base::AgentStreamEvent::ToolCallsPlanned { .. } => "tool_calls_planned",
            agent_base::AgentStreamEvent::ToolCallStarted { .. } => "tool_call_started",
            agent_base::AgentStreamEvent::ToolCallFinished { .. } => "tool_call_finished",
            agent_base::AgentStreamEvent::SubAgentEvent { .. } => "sub_agent_event",
            agent_base::AgentStreamEvent::TextDelta { .. } => "text_delta",
            agent_base::AgentStreamEvent::Answer { .. } => "answer",
            agent_base::AgentStreamEvent::RunFinished { .. } => "run_finished",
            agent_base::AgentStreamEvent::UserMessage { .. } => "user_message",
        };

        Self {
            event: event_type.to_string(),
            data: serde_json::to_value(event).unwrap_or(serde_json::Value::Null),
        }
    }
}
