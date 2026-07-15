//! SDK 常用类型 Prelude
//!
//! 统一导出 `agent-runtime` 中最常用的类型，方便使用者一次性导入：
//!
//! ```rust,no_run
//! use agent_runtime::prelude::*;
//! ```
//!
//! 包含：
//! - 核心运行时：[`Agent`]、[`AgentBuilder`]、[`RuntimeServices`]
//! - 事件：[`AgentStreamEvent`]（流由 [`Agent::run_stream`] 返回）
//! - 错误：[`AgentResult`]、[`Error`]
//! - 模型与消息：[`ConversationMessage`]、[`ModelTurn`]
//! - 工具池：[`ToolPool`]
//! - 内置工具入口：[`all_builtin_tools`](crate::tool::simple_tools::all_builtin_tools)
//!
//! `feature = "mcp"` 启用时，另行导出 [`McpRegistry`](crate::mcp::McpRegistry)。

pub use crate::context::MessageContext;
pub use crate::error::{AgentResult, Error};
pub use crate::message::ConversationMessage;
pub use crate::runtime::{Agent, AgentBuilder, ModelTurn, RuntimeServices};
pub use crate::session::AgentStreamEvent;
pub use crate::tool::ToolPool;
