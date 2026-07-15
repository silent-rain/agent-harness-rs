//! MCP 配置加载器
//!
//! 提供从不同来源加载 MCP 服务配置的加载器。
//!
//! 提供的主要内容：
//! - [`LocalMcpLoader`] - 从本地 `mcp.json` 文件加载 MCP 服务配置

mod local;

pub use local::LocalMcpLoader;
