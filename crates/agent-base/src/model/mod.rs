//! LLM 模型适配层
//!
//! 提供与具体 provider 解耦的 LLM I/O 抽象，让 `Agent` 通过
//! `Arc<dyn ModelAdapter>` 持有任意 rig-core 兼容的模型实例。
//!
//! 提供的主要内容：
//! - [`ModelAdapter`] - object-safe 的 LLM 模型调用 trait
//!   （自动为所有 `rig::CompletionModel` 实现）
//! - [`StreamChunk`] / [`ToolCallDelta`] / [`ToolCallChunkContent`] -
//!   类型擦除后的 token 流式数据片段
//! - [`BoxedCompletionStream`] - 装箱的流类型别名
//! - [`MockModel`] - 测试用的公共 `ModelAdapter` 实现

mod adapter;
mod mock;
mod openai;
pub mod streaming;

pub use adapter::ModelAdapter;
pub use mock::{MockBehavior, MockModel};
pub use openai::create_model;
pub use streaming::{BoxedCompletionStream, StreamChunk, ToolCallChunkContent, ToolCallDelta};
