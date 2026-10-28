//! MCP 工具
//!
//! 两类工具共用同一套 rig [`Tool`](rig_core::tool::Tool) 实现形态：
//! - 远端工具执行：[`McpToolExecTool`]（由 `build_tools()` 注入给 LLM）
//! - 服务内省：[`McpServiceListTool`]、[`McpServiceToolsTool`]

mod mcp_service_list;
mod mcp_service_tools;
mod mcp_tool_exec;

pub use mcp_service_list::McpServiceListTool;
pub use mcp_service_tools::McpServiceToolsTool;
pub use mcp_tool_exec::{DEFAULT_MCP_TOOL_TIMEOUT, McpToolExecTool};
