//! 对话上下文模块
//!
//! 管理 LLM 对话上下文窗口的构建、累加与转换。
//!
//! 提供的主要内容：
//! - [`MessageContext`][]: 对话消息上下文管理器

mod message_context;

pub use message_context::MessageContext;
