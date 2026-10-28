//! MCP 连接注册表与工具
//!
//! 独立管理所有 MCP 服务的连接生命周期，并提供 LLM 可调用的远端工具与内省工具。
//!
//! ## 子模块
//!
//! - [`loader`] - MCP 配置加载器（`LocalMcpLoader`）
//! - [`registry`] - MCP 连接注册表（`McpRegistry`）
//! - [`tools`] - MCP 工具（远端执行 `McpToolExecTool`、内省 `McpServiceListTool` / `McpServiceToolsTool`）

mod loader;
mod registry;
mod tools;

pub use loader::LocalMcpLoader;
pub use registry::{McpConnection, McpRegistry, ToolPolicy};
pub use tools::{
    DEFAULT_MCP_TOOL_TIMEOUT, McpServiceListTool, McpServiceToolsTool, McpToolExecTool,
};
