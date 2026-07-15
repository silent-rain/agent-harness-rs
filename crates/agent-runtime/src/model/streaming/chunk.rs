//! 流式响应数据类型（对外契约）
//!
//! 把 `rig::streaming::RawStreamingChoice<R>` 中携带 provider 私有响应类型 `R`
//! 的字段统一折叠为不带 `R` 的形态，使 `Arc<dyn ModelAdapter>` 可以产出
//! 与具体 provider 无关的流式分片。
//!
//! 提供的主要内容：
//! - [`StreamChunk`] - 类型擦除后的流式数据片段
//! - [`ToolCallDelta`] - 工具调用流式增量
//! - [`ToolCallChunkContent`] - 工具调用增量的内容种类
//! - [`BoxedCompletionStream`] - 装箱的流类型别名

use std::pin::Pin;

use futures::Stream;
use rig_core::completion::Usage;

use crate::error::Error;

/// 工具调用流式增量的内容种类
///
/// 与 rig-core `streaming::ToolCallDeltaContent` 一一对应。
#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallChunkContent {
    /// 工具名称增量（通常出现在第一个分片）
    Name(String),
    /// 参数 JSON 文本的增量
    ArgumentsDelta(String),
}

/// 工具调用流式增量
///
/// 用于在 LLM 边生成工具调用时即时透出。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallDelta {
    /// 提供方分配的工具调用 ID（可能为空）
    pub id: String,
    /// rig 内部生成的稳定 ID，用于在同一条工具调用内聚合多个 delta
    pub internal_call_id: String,
    /// 增量内容
    pub content: ToolCallChunkContent,
}

/// 类型擦除后的流式响应分片
///
/// 把 `rig::streaming::RawStreamingChoice<R>` 中携带 provider 私有响应类型 `R`
/// 的字段统一折叠为不带 `R` 的形态，使 `Arc<dyn ModelAdapter>` 可以输出此类型。
#[derive(Debug, Clone, PartialEq)]
pub enum StreamChunk {
    /// 文本增量（assistant message 的 token）
    TextDelta(String),
    /// 推理过程整体内容
    ///
    /// 仅承载文本以避免暴露 rig 内部的 `ReasoningContent` 结构。
    Reasoning {
        /// 提供方推理段 ID（可选）
        id: Option<String>,
        /// 推理文本（如有多段会被简单拼接）
        text: String,
    },
    /// 推理增量（思考链 token）
    ReasoningDelta {
        /// 提供方推理段 ID（可选）
        id: Option<String>,
        /// 增量推理文本
        text: String,
    },
    /// 完整工具调用（一次性返回所有参数）
    ToolCall {
        /// 提供方分配的 ID
        id: String,
        /// rig 内部稳定 ID
        internal_call_id: String,
        /// 工具名称
        name: String,
        /// 工具参数 JSON
        arguments: serde_json::Value,
    },
    /// 工具调用增量（参数 JSON 流式拼装）
    ToolCallDelta(ToolCallDelta),
    /// 提供方分配的消息 ID
    MessageId(String),
    /// 流结束信号，可附带 token 用量
    Done {
        /// 累计 token 用量（如可获取）
        usage: Option<Usage>,
    },
}

/// 装箱的流式响应类型别名
///
/// `'static` 边界保证可在 `Arc<dyn ModelAdapter>` 之间传递。
pub type BoxedCompletionStream =
    Pin<Box<dyn Stream<Item = Result<StreamChunk, Error>> + Send + 'static>>;
