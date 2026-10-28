//! rig 数据结构 → [`StreamChunk`] 的翻译工具
//!
//! 集中放置从 rig-core 0.45 流式事件 [`Item<StreamEvent>`] 到本 crate
//! 类型擦除流式表示之间的转换逻辑，避免散落在 `model/adapter.rs` 与各处 mock 中。
//!
//! 提供的主要内容：
//! - [`map_stream_event`]: rig 流式事件类型擦除为 [`StreamChunk`]

use rig_core::{
    message::AssistantContent,
    streaming::{Item, PartKind, StreamEvent},
};

use super::chunk::{StreamChunk, ToolCallChunkContent, ToolCallDelta};

/// 把 rig 流式事件 [`Item<StreamEvent>`] 类型擦除为 [`StreamChunk`]。
///
/// rig-core 0.45 的流以「part」为粒度：`Start` 声明部件、`Text` /
/// `Reasoning` / `Arguments` 增长、`End` 定稿。这里把它们折叠成本仓库
/// 既有的扁平分片语义：
///
/// - 文本 / 推理增量 → `TextDelta` / `ReasoningDelta`
/// - 工具调用：`Start` 出名字增量、`Arguments` 出参数增量、`End` 出完整调用
/// - 推理 `End` → `Reasoning` 整段（供落库去重，客户端已收到增量）
///
/// `internal_call_id` 取部件下标，用于把同一条工具调用的多个 delta 聚合。
///
/// # Parameters
///
/// * `item` - rig-core 的流式条目（事件或未建模载荷）
///
/// # Returns
///
/// 翻译后的 [`StreamChunk`]；对于无需透出的条目返回 `None`
pub(crate) fn map_stream_event(item: &Item<StreamEvent>) -> Option<StreamChunk> {
    let event = match item {
        Item::Event(event) => event,
        // Unknown 是 rig 未建模的 provider 原生输出项，不进入我们的流
        Item::Unknown(_) => return None,
    };

    match event {
        StreamEvent::Text { text, .. } => {
            if text.is_empty() {
                None
            } else {
                Some(StreamChunk::TextDelta(text.clone()))
            }
        }
        StreamEvent::Reasoning { text, .. } => {
            if text.is_empty() {
                None
            } else {
                Some(StreamChunk::ReasoningDelta {
                    id: None,
                    text: text.clone(),
                })
            }
        }
        StreamEvent::Start {
            part,
            kind: PartKind::ToolCall,
            name,
        } => {
            let name = name.as_ref()?;
            Some(StreamChunk::ToolCallDelta(ToolCallDelta {
                id: String::new(),
                internal_call_id: part.index().to_string(),
                content: ToolCallChunkContent::Name(name.to_string()),
            }))
        }
        StreamEvent::Arguments { part, json } => Some(StreamChunk::ToolCallDelta(ToolCallDelta {
            id: String::new(),
            internal_call_id: part.index().to_string(),
            content: ToolCallChunkContent::ArgumentsDelta(json.clone()),
        })),
        StreamEvent::End {
            part,
            content: AssistantContent::ToolCall(call),
        } => Some(StreamChunk::ToolCall {
            id: call.id.to_string(),
            internal_call_id: part.index().to_string(),
            name: call.function.name.to_string(),
            arguments: serde_json::Value::Object(call.function.arguments.clone()),
        }),
        StreamEvent::End {
            content: AssistantContent::Reasoning(reasoning),
            ..
        } => {
            if reasoning.text.is_empty() {
                None
            } else {
                Some(StreamChunk::Reasoning {
                    id: None,
                    text: reasoning.text.clone(),
                })
            }
        }
        // 其余部件的开始 / 结束（文本已增量透出、图片等模态不参与决策）忽略
        StreamEvent::Start { .. } | StreamEvent::End { .. } => None,
    }
}
