//! OpenAI Chat Completions 协议兼容层
//!
//! 提供 `agent-runtime` 内部事件流到 OpenAI Chat Completions API 的协议产物，
//! 上层（HTTP / RPC / SDK 调用方）可直接复用 OpenAI 官方 SDK 消费。
//!
//! 提供的主要内容：
//! - 标准类型重导出（来自 `async-openai` crate）
//! - [`error`]: OpenAI 错误响应 [`OpenAIError`]
//! - [`stream_adapter`]: 内部事件 → OpenAI 协议产物的单向适配器
//!   [`OpenAIStreamAdapter`]
//! - [`raw_stream_adapter`]: 原始单轮 LLM 调用事件 → OpenAI 协议产物的适配器
//!   [`OpenAIRawStreamAdapter`]
//!
//! ## 设计原则
//!
//! - 只做"翻译"，不持有任何业务状态（除适配器内部的 tool_calls 索引映射）
//! - 字段命名严格对齐 OpenAI Chat Completions API（2024-11 规范）
//! - 扩展事件（step / tool_call / rag）不混入 OpenAI 主流，单独以
//!   [`AdapterOutput::Extension`](stream_adapter::AdapterOutput) 通道输出，
//!   由调用方决定如何承载（SSE event name / 自定义字段 / 丢弃）

pub mod error;
pub mod raw_stream_adapter;
pub mod stream_adapter;

// 重导出 async-openai 标准类型，保持向后兼容的别名
pub use async_openai::types::chat::{
    ChatChoice, ChatChoiceStream, ChatCompletionMessageToolCallChunk,
    ChatCompletionMessageToolCalls, ChatCompletionResponseMessage,
    ChatCompletionStreamResponseDelta, CompletionUsage, CreateChatCompletionResponse,
    CreateChatCompletionStreamResponse, FinishReason, FunctionCallStream, Role,
};

// 向后兼容的类型别名
pub type ChatCompletion = CreateChatCompletionResponse;
pub type ChatCompletionChunk = CreateChatCompletionStreamResponse;
pub type CompletionChoice = ChatChoice;
pub type ChatCompletionMessage = ChatCompletionResponseMessage;
pub type ChunkChoice = ChatChoiceStream;
pub type ChoiceDelta = ChatCompletionStreamResponseDelta;
pub type ChunkToolCallDelta = ChatCompletionMessageToolCallChunk;
pub type ChunkFunctionDelta = FunctionCallStream;
pub type Usage = CompletionUsage;

pub use error::{ErrorBody, OpenAIError};
pub use raw_stream_adapter::{OpenAIRawStreamAdapter, RawAdapterOutput};
pub use stream_adapter::{AdapterOutput, OpenAIStreamAdapter};
