//! ModelCaller 中间产物与单步结果类型
//!
//! 提供的主要内容：
//! - [`ModelTurn`] - 一次 LLM 调用聚合后的可执行内容（文本 + 工具调用列表）

use rig_core::completion::Usage;

use crate::session::PlannedToolCall;

/// 一次 LLM 调用聚合后的可执行内容
///
/// 由 [`crate::runtime::llm::ModelCaller`] 在流式聚合完成后产出，
/// 是 `before_model` / `after_model` 钩子可观察 / 修改的中间产物。
///
/// # Semantics
///
/// - `text` 与 `tool_calls` **可同时存在**（OpenAI/Anthropic 原生支持）；
/// - `tool_calls` 为空且 `text` 也为空时视为非法响应；
/// - 模型可能在 tool_calls 为空时返回 text，但此 text 可能是工具执行后
///   的即时附带说明，而非面向用户的完整总结。需结合上下文判断。
#[derive(Debug, Clone, Default)]
pub struct ModelTurn {
    /// LLM 输出的纯文本（可能为空）
    pub text: Option<String>,
    /// LLM 输出的工具调用列表（可能为空）
    pub tool_calls: Vec<PlannedToolCall>,
    /// 本次 LLM 调用的 token 用量（如可获取）
    pub usage: Option<Usage>,
}

impl ModelTurn {
    /// 是否完全为空（无文本且无工具调用）
    pub fn is_empty(&self) -> bool {
        self.tool_calls.is_empty() && self.text.as_deref().is_none_or(str::is_empty)
    }

    /// 是否应作为候选最终回答（无 tool_calls 且有 text）
    pub fn is_final_answer(&self) -> bool {
        self.tool_calls.is_empty() && self.text.as_deref().is_some_and(|t| !t.is_empty())
    }
}
