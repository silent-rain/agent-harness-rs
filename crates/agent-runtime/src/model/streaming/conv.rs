//! rig 数据结构 → [`StreamChunk`] 的翻译工具
//!
//! 集中放置从 rig-core 类型到本 crate 类型擦除流式表示之间的转换逻辑，
//! 避免散落在 `model/adapter.rs` 与各处 mock 中。
//!
//! 提供的主要内容：
//! - [`flatten_reasoning`]: 把 [`Reasoning`] 多段内容平铺为 `(id, text)`
//! - [`map_streamed_content`]: rig 流式 token 类型擦除为 [`StreamChunk`]

use rig_core::completion::message::{Reasoning, ReasoningContent};
use rig_core::streaming::{StreamedAssistantContent, ToolCallDeltaContent};

use super::chunk::{StreamChunk, ToolCallChunkContent, ToolCallDelta};

/// 把 [`Reasoning`] 的多段内容平铺为 `(id, text)`。
///
/// 对加密 / 截断等专用变体降级为占位字符串。
///
/// # Parameters
///
/// * `reasoning` - rig-core 的多段推理结构
///
/// # Returns
///
/// `(reasoning.id, 拼接后的纯文本)`
pub(crate) fn flatten_reasoning(reasoning: &Reasoning) -> (Option<String>, String) {
    let mut buf = String::new();
    for (idx, item) in reasoning.content.iter().enumerate() {
        if idx > 0 {
            buf.push('\n');
        }
        match item {
            ReasoningContent::Text { text, .. } => buf.push_str(text),
            ReasoningContent::Summary(text) => buf.push_str(text),
            ReasoningContent::Encrypted(_) => buf.push_str("<encrypted reasoning>"),
            ReasoningContent::Redacted { .. } => buf.push_str("<redacted reasoning>"),
            _ => {}
        }
    }
    (reasoning.id.clone(), buf)
}

/// 把 rig 的 [`StreamedAssistantContent`] 类型擦除为 [`StreamChunk`]。
///
/// `Final(R)` 仅承载 provider 私有结构，无法跨越类型擦除边界，
/// 流末尾的 token 用量统一通过 `StreamChunk::Done` 事件透出。
///
/// # Parameters
///
/// * `content` - rig-core 的流式 assistant 内容分片
///
/// # Returns
///
/// 翻译后的 [`StreamChunk`]；对于无需透出的分片返回 `None`
pub(crate) fn map_streamed_content<R>(content: StreamedAssistantContent<R>) -> Option<StreamChunk>
where
    R: Clone,
{
    match content {
        StreamedAssistantContent::Text(text) => {
            if text.text.is_empty() {
                None
            } else {
                Some(StreamChunk::TextDelta(text.text))
            }
        }
        StreamedAssistantContent::ToolCall {
            tool_call,
            internal_call_id,
        } => Some(StreamChunk::ToolCall {
            id: tool_call.id,
            internal_call_id,
            name: tool_call.function.name,
            arguments: tool_call.function.arguments,
        }),
        StreamedAssistantContent::ToolCallDelta {
            id,
            internal_call_id,
            content,
        } => {
            let content = match content {
                ToolCallDeltaContent::Name(name) => ToolCallChunkContent::Name(name),
                ToolCallDeltaContent::Delta(delta) => ToolCallChunkContent::ArgumentsDelta(delta),
            };
            Some(StreamChunk::ToolCallDelta(ToolCallDelta {
                id,
                internal_call_id,
                content,
            }))
        }
        StreamedAssistantContent::Reasoning(reasoning) => {
            let (id, text) = flatten_reasoning(&reasoning);
            Some(StreamChunk::Reasoning { id, text })
        }
        StreamedAssistantContent::ReasoningDelta { id, reasoning } => {
            Some(StreamChunk::ReasoningDelta {
                id,
                text: reasoning,
            })
        }
        // Final 仅承载 provider 私有结构，对外只通过 Done 事件汇报结束
        StreamedAssistantContent::Final(_) => None,
        // Unknown 是 rig 未建模的 provider 原生输出项，仅透传供检查，不进入我们的流
        StreamedAssistantContent::Unknown(_) => None,
    }
}
