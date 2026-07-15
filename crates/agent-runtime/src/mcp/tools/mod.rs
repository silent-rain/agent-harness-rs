//! MCP 内省工具
//!
//! 提供 LLM 可调用的 MCP 服务内省能力：
//! - [`McpServiceListTool`] - 列出当前所有已连接的 MCP 服务
//! - [`McpServiceToolsTool`] - 列出指定 MCP 服务的可用工具

mod mcp_service_list;
mod mcp_service_tools;

pub use mcp_service_list::McpServiceListTool;
pub use mcp_service_tools::McpServiceToolsTool;
