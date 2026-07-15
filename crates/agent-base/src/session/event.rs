//! 流式事件模型
//!
//! 定义智能体运行时的统一事件流协议。
//! 所有可执行动作的生命周期事件都通过此模型传递给客户端。
//!
//! ## 子智能体事件透传
//!
//! 当子智能体执行时，其内部事件可通过 `SubAgentEvent` 变体透传给父智能体的调用者，
//! 使客户端能够感知子智能体的完整推理过程（工具调用、步骤等）。
//!
//! 运行时使用 [`tokio::sync::mpsc::Sender<AgentStreamEvent>`] 直接推送事件，
//! 不再提供额外的 `Publisher` 抽象。

use std::pin::Pin;

use rig_core::completion::Usage;
use serde::{Deserialize, Serialize};

use crate::types::agent_meta::AgentMeta;
use crate::types::tool::ToolSource;

/// 装箱事件流类型别名
///
/// Agent 的流式接口返回此异步流，消费者可逐事件消费，实现流式响应。
pub type BoxStream<T> = Pin<Box<dyn tokio_stream::Stream<Item = T> + Send>>;

/// Agent 流事件流别名
pub type AgentEventStream = BoxStream<AgentStreamEvent>;

/// 智能体流式事件
///
/// 覆盖从运行开始到结束的完整生命周期，
/// 包括结构化调用生命周期事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum AgentStreamEvent {
    /// 运行开始
    RunStarted {
        run_id: String,
        agent_id: i64,
        agent_name: String,
        session_id: String,
        user_id: String,
        /// 智能体运行元数据
        ///
        /// 包含来源标识与父级关联信息，是主子智能体事件关联的核心字段。
        /// 子智能体 / A2A 场景中可通过 `meta.agent_source` 取得父级 run_id、agent_id 等关联键。
        meta: AgentMeta,
    },
    /// 用户输入消息
    ///
    /// 在 `RunStarted` 之后由运行时发射，作为历史回放中"用户消息气泡"的源头事件。
    /// 仅承载用户输入的纯文本；未来若需要承载附件、引用等结构化载荷，
    /// 应通过新增显式字段（如 `attachments`、`quote`）进行扩展，
    /// 而不是引入泛化的"context"字段。
    UserMessage { run_id: String, content: String },
    /// 单步开始
    StepStarted { run_id: String, step: u32 },
    /// 推理整体内容（一次性整段思考块）
    ///
    /// 由 `ModelCaller` 在收到 [`crate::model::StreamChunk::Reasoning`] 时一次性发布，
    /// 对应 provider 一次性返回的完整思考段（如 OpenAI Responses API 的 reasoning summary、
    /// Anthropic 的整段 thinking 块）。
    ///
    /// 与 [`AgentStreamEvent::ReasoningDelta`] 的区别：
    /// - `Reasoning`：完整思考段，每段 **至多一条**，下游应直接整段渲染/落库
    /// - `ReasoningDelta`：思考链的 token 增量，下游需累积拼接
    Reasoning {
        run_id: String,
        /// provider 推理段 ID（可选），用于在多段推理之间区分归属
        id: Option<String>,
        content: String,
    },
    /// 推理增量（token 流）
    ReasoningDelta {
        run_id: String,
        /// provider 推理段 ID（可选），同段内多次增量共享同一 ID
        id: Option<String>,
        delta: String,
    },
    /// LLM 单步产出的整批待执行工具调用清单
    ///
    /// 由 [`crate::runtime::llm::ModelCaller`] 在 LLM 流式输出聚合完成后，
    /// 即将进入工具执行阶段前**一次性**发布。
    ToolCallsPlanned {
        run_id: String,
        step: u32,
        calls: Vec<PlannedToolCall>,
    },
    /// 工具调用开始
    ToolCallStarted {
        run_id: String,
        step: u32,
        toolname: String,
        call_id: String,
        arguments_json: String,
        /// 工具来源（builtin / mcp / skill / sub_agent）
        ///
        /// 由运行时通过 [`crate::tool::ToolPool::source_of`] 查表填充，
        /// 用于让前端区分卡片样式。历史事件反序列化时缺省为
        /// [`ToolSource::Builtin`]。
        #[serde(default)]
        source: ToolSource,
    },
    /// 工具调用完成
    ToolCallFinished {
        run_id: String,
        step: u32,
        toolname: String,
        call_id: String,
        success: bool,
        /// 工具结果（成功时为完整输出；失败时为 `None`）
        result: Option<String>,
        /// 错误信息（失败时填充；成功时为 `None`）
        error_message: Option<String>,
        /// 调用耗时（毫秒）
        duration_ms: i64,
        /// 工具来源（与 [`AgentStreamEvent::ToolCallStarted::source`] 同义）
        #[serde(default)]
        source: ToolSource,
    },
    /// 子智能体内部事件（透传）
    ///
    /// 当子智能体启用事件透传时，其内部产生的所有事件（工具调用、步骤等）
    /// 会被包装为此变体，附带父智能体上下文后推送给父智能体的调用者。
    SubAgentEvent {
        /// 父智能体的运行 ID
        parent_run_id: String,
        /// 父智能体的 agent ID
        parent_agent_id: i64,
        /// 父智能体的 session ID（冗余但方便落库关联）
        parent_session_id: String,
        /// 委派目标名称（本地子智能体名 / A2A 服务名）
        delegate_target: String,
        /// 委派类型
        delegate_type: DelegateType,
        /// 子智能体内部事件
        event: Box<AgentStreamEvent>,
    },
    /// LLM 文本 token 增量（真流）
    ///
    /// 由 `ModelCaller` 在每收到一段 `StreamChunk::TextDelta` 时立即发布，
    /// 对应 OpenAI Chat Completions stream 中的 `choices[].delta.content`。
    ///
    /// 与 [`AgentStreamEvent::Answer`] 的区别：
    /// - `TextDelta`：LLM 原始 token 流，每个 token 一条事件，**仅在产生答案的最后一轮发出**
    /// - `Answer`：运行终止时一次性发出的整段助手回答，每次运行**至多一条**
    TextDelta { run_id: String, delta: String },
    /// 最终回答（整段一次性发布）
    ///
    /// 由 [`crate::runtime::llm::ModelCaller`] 在本轮 LLM 输出仅包含纯文本
    /// （即没有任何 `tool_calls`）时发布，作为运行**终止标志**与整段答案的镜像投递。
    /// 整段答案一次性产出，不做 token 切分。
    ///
    /// 真正的逐 token 流以 [`AgentStreamEvent::TextDelta`] 承载；
    /// 客户端可据 `Answer` 完成会话落库、`finish_reason: stop` 映射等终止动作。
    Answer { run_id: String, content: String },
    /// 运行完成
    ///
    /// 终止信号事件：包含成功标志、统计信息与失败原因；最终回答内容
    /// 由独立的 [`AgentStreamEvent::Answer`] 事件承载，本事件不再重复携带。
    RunFinished {
        run_id: String,
        total_steps: u32,
        finish_reason: FinishReason,
        /// 错误信息（仅 `success = false` 时填充）
        error_message: Option<String>,
        /// 运行耗时（毫秒）
        duration_ms: i64,
        /// 累计 token 用量（所有 LLM 调用步骤的汇总）
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
    },
}

impl AgentStreamEvent {
    /// 返回事件的类型判别符（snake_case 字符串）
    ///
    /// 与 `#[serde(tag = "event_type", rename_all = "snake_case")]` 序列化后
    /// 写入 JSON 的 `event_type` 字段值保持一致，便于在不进行完整序列化的前提下
    /// 获取事件类型标签（如写入 `t_agent_session_event.event_type` 列、
    /// 调试日志、按类型过滤等场景）。
    ///
    /// # Examples
    ///
    /// ```
    /// use agent_base::session::AgentStreamEvent;
    ///
    /// let event = AgentStreamEvent::UserMessage {
    ///     run_id: "r-1".into(),
    ///     content: "hello".into(),
    /// };
    /// assert_eq!(event.event_type(), "user_message");
    /// ```
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::RunStarted { .. } => "run_started",
            Self::UserMessage { .. } => "user_message",
            Self::StepStarted { .. } => "step_started",
            Self::Reasoning { .. } => "reasoning",
            Self::ReasoningDelta { .. } => "reasoning_delta",
            Self::ToolCallsPlanned { .. } => "tool_calls_planned",
            Self::ToolCallStarted { .. } => "tool_call_started",
            Self::ToolCallFinished { .. } => "tool_call_finished",
            Self::SubAgentEvent { .. } => "sub_agent_event",
            Self::TextDelta { .. } => "text_delta",
            Self::Answer { .. } => "answer",
            Self::RunFinished { .. } => "run_finished",
        }
    }
    /// 提取事件关联的 `run_id`
    ///
    /// 大多数事件直接以 `run_id` 字段标识所属运行；`SubAgentEvent` 是子智能体
    /// 内部事件的透传包装，其内层 `event` 自身的 `run_id` 属于子智能体，
    /// 对父 Session 没有意义，因此返回外层的 `parent_run_id`，确保事件
    /// 始终落地到父 Session 对应的运行链路上。
    ///
    /// # Examples
    ///
    /// ```
    /// use agent_base::session::AgentStreamEvent;
    ///
    /// let event = AgentStreamEvent::TextDelta {
    ///     run_id: "r-123".into(),
    ///     delta: "x".into(),
    /// };
    /// assert_eq!(event.run_id(), Some("r-123"));
    /// ```
    pub fn run_id(&self) -> Option<&str> {
        match self {
            Self::RunStarted { run_id, .. }
            | Self::UserMessage { run_id, .. }
            | Self::StepStarted { run_id, .. }
            | Self::Reasoning { run_id, .. }
            | Self::ReasoningDelta { run_id, .. }
            | Self::ToolCallsPlanned { run_id, .. }
            | Self::ToolCallStarted { run_id, .. }
            | Self::ToolCallFinished { run_id, .. }
            | Self::TextDelta { run_id, .. }
            | Self::Answer { run_id, .. }
            | Self::RunFinished { run_id, .. } => Some(run_id.as_str()),
            Self::SubAgentEvent { parent_run_id, .. } => Some(parent_run_id.as_str()),
        }
    }
}

/// 委派目标类型
///
/// 标识 [`AgentStreamEvent::SubAgentEvent`] 中被委派的目标类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegateType {
    /// 主智能体（顶层 Agent）
    Agent,
    /// 本地子智能体（`DelegateSubAgentTool` / `FlatSubAgentTool`）
    SubAgent,
    /// 远程 A2A 智能体（`A2aAgentTool`）
    A2aAgent,
}

/// 运行结束原因
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// 产出了面向用户的最终回答
    FinalizeAction,
    /// 达到最大轮次
    MaxTurns,
    /// 运行出错
    Error,
    /// 被外部取消
    Cancelled,
}

/// 一次 LLM 单步聚合后产出的待执行工具调用
///
/// [`AgentStreamEvent::ToolCallsPlanned`] 的元素类型，用作
/// "LLM 已决定本轮要并行调哪些工具"的整批清单镜像。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PlannedToolCall {
    /// provider 分配的工具调用 ID
    pub call_id: String,
    /// 工具名称
    pub tool_name: String,
    /// 工具参数 JSON（聚合后已修复的整段字符串）
    pub arguments_json: String,
    /// 工具来源（builtin / mcp / skill / sub_agent）
    ///
    /// 由运行时通过 [`crate::tool::ToolPool::source_of`] 查表填充。
    /// 历史事件反序列化时缺省为 [`ToolSource::Builtin`]，
    /// 以保持向前兼容。
    #[serde(default)]
    pub source: ToolSource,
}

// ─── 单元测试 ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `event_type()` 返回与 `serde(tag = "event_type")` 一致的 snake_case 标签
    #[test]
    fn agent_stream_event_event_type_returns_snake_case_tag() {
        let event = AgentStreamEvent::UserMessage {
            run_id: "r".into(),
            content: "hi".into(),
        };
        assert_eq!(event.event_type(), "user_message");
    }

    /// `run_id()` 从带 `run_id` 字段的常规事件中返回内层值
    #[test]
    fn agent_stream_event_run_id_returns_inner_id() {
        let event = AgentStreamEvent::TextDelta {
            run_id: "r-123".into(),
            delta: "x".into(),
        };
        assert_eq!(event.run_id(), Some("r-123"));
    }

    /// `run_id()` 对于子智能体透传事件返回外层的 `parent_run_id`，
    /// 确保事件落地到父 Session 的运行链路上
    #[test]
    fn agent_stream_event_run_id_for_sub_agent_event_uses_parent() {
        let inner = AgentStreamEvent::TextDelta {
            run_id: "child".into(),
            delta: "x".into(),
        };
        let event = AgentStreamEvent::SubAgentEvent {
            parent_run_id: "parent".into(),
            parent_agent_id: 1,
            parent_session_id: "sess-1".into(),
            delegate_target: "c".into(),
            delegate_type: DelegateType::SubAgent,
            event: Box::new(inner),
        };
        assert_eq!(event.run_id(), Some("parent"));
    }
}
