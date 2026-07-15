//! LLM Token 级流式响应类型
//!
//! 提供 object-safe 的流式响应数据结构，用于在不暴露 rig-core
//! `CompletionModel::StreamingResponse` 关联类型的前提下，
//! 让 `Arc<dyn ModelAdapter>` 也能产出 token 级的增量数据。
//!
//! 提供的主要内容：
//! - [`StreamChunk`] - 类型擦除后的流式数据片段
//! - [`ToolCallDelta`] / [`ToolCallChunkContent`] - 工具调用的流式增量
//! - [`BoxedCompletionStream`] - 装箱的流类型别名
//! - [`conv`] - rig 流式数据 → [`StreamChunk`] 的翻译工具（crate 内部使用）

pub mod chunk;
pub(crate) mod conv;

pub use chunk::{BoxedCompletionStream, StreamChunk, ToolCallChunkContent, ToolCallDelta};
