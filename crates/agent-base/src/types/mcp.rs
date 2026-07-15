//! MCP types
//!
//! Provides:
//! - [`McpServerConfig`]: MCP service binding config (from DB or local)
//! - [`McpServiceEntry`]: Unified MCP server config (used in both `config_json` and local `mcp.json`)
//! - [`McpServiceInfo`]: Service summary for introspection
//! - [`McpServiceDescription`]: Service metadata from MCP handshake
//! - [`McpToolInfo`]: Tool summary for introspection

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// =============================================================================
// Core config types
// =============================================================================

/// MCP 配置加载器
///
/// 读取指定路径的 `mcp.json` 文件，将其解析为 [`McpServerConfig`] 列表。
///
/// 设计上保持"被动"：仅在 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
/// 加载结果由调用方注入到 [`crate::mcp::McpRegistry`] 中统一建立连接。
///
/// Local `.codebuddy/mcp/mcp.json` file schema
///
/// Represents the top-level structure of a local MCP configuration file.
/// Each key in `mcpServers` is the service name, and the value is a
/// [`McpServerConfig`] describing the server connection parameters.
///
/// # File format
///
/// ```json
/// {
///   "mcpServers": {
///     "simple-tool": {
///       "url": "http://127.0.0.1:8081/sse",
///       "transportType": "sse"
///     },
///     "dash-mcp": {
///       "command": "npx",
///       "args": ["@tencent/dash-mcp@latest"],
///       "transportType": "stdio"
///     }
///   }
/// }
/// ```
#[derive(Debug, serde::Deserialize)]
pub struct McpServersWrapper {
    /// MCP server entries keyed by service name
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: HashMap<String, McpServiceEntry>,
}

/// MCP 服务配置
///
/// 描述智能体绑定的 MCP 服务及其允许调用的工具白名单。
/// 白名单来源于 `t_agent_mcp_service_rel.enabled_tools` 字段，仅列表中的工具可被调用；
/// 列表为空时该服务无可用工具。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// MCP 服务名称
    pub service_name: String,
    /// 已启用的工具白名单（空=无可用工具）
    pub allowed_tools: Vec<String>,
    /// MCP 服务配置
    pub server_config: McpServiceEntry,
}

/// 统一的 MCP 服务器配置
///
/// 同时用于：
/// 1. `t_agent_mcp_service_rel.config_json` 字段的结构化解析（线上配置）
/// 2. 本地 `.codebuddy/mcp/mcp.json` 中单个服务条目的解析
///
/// 线上与本地存储结构一致，`config_json` 直接存储 `McpServiceEntry` 的 JSON 序列化：
///
/// ```json
/// {"url": "http://127.0.0.1:8081/sse", "transportType": "sse", "timeout": 30000}
/// ```
///
/// ## stdio 类型必填字段
///
/// - `command`: 可执行命令（如 `npx`、`uvx`）
/// - `args`: 命令参数列表
///
/// ## sse / http 类型必填字段
///
/// - `url`: 服务地址
///
/// ## 通用可选字段
///
/// - `transport_type`: 传输类型（`stdio`、`sse`、`streamable-http`），可自动推断
/// - `env`: 环境变量（仅 stdio 类型生效）
/// - `timeout`: 连接/请求超时（毫秒）
/// - `headers`: 自定义 HTTP 请求头（仅 sse / http 类型生效）
///
/// # Examples
///
/// ```rust
/// use agent_base::types::mcp::McpServiceEntry;
///
/// // stdio config
/// let json = r#"{"command": "npx", "args": ["--registry", "https://example.com", "@pkg/mcp"]}"#;
/// let config: McpServiceEntry = serde_json::from_str(json).unwrap();
/// assert_eq!(config.command.as_deref(), Some("npx"));
/// assert_eq!(config.args.len(), 3);
///
/// // http config with headers and timeout
/// let json = r#"{"url": "http://127.0.0.1:8081/sse", "transportType": "sse", "timeout": 30000, "headers": {"X-Custom": "value"}}"#;
/// let config: McpServiceEntry = serde_json::from_str(json).unwrap();
/// assert_eq!(config.url.as_deref(), Some("http://127.0.0.1:8081/sse"));
/// assert_eq!(config.timeout, Some(30000));
/// assert_eq!(config.headers.get("X-Custom").map(|s| s.as_str()), Some("value"));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServiceEntry {
    /// Server URL (required for sse/http, absent for stdio)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Transport type: `stdio`, `sse`, `streamable-http`
    ///
    /// Can be omitted — will be inferred from other fields:
    /// - `command` present → `stdio`
    /// - `url` present → `sse`
    /// - fallback → `stdio`
    #[serde(
        rename = "transportType",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub transport_type: Option<String>,

    /// Executable command for stdio transport (e.g. `npx`, `uvx`, `node`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Command arguments for stdio transport
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables for stdio transport
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Connection/request timeout in milliseconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,

    /// Custom HTTP headers for sse/http transport
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
}

// =============================================================================
// Introspection types
// =============================================================================

/// MCP 服务摘要信息
#[derive(Debug, Clone, Serialize)]
pub struct McpServiceInfo {
    /// Service name
    pub name: String,
    /// Transport type (stdio, http, sse, etc.)
    pub transport_type: String,
    /// Number of available tools
    pub tool_count: usize,
}

/// MCP 服务描述信息
///
/// 通过 MCP 协议初始化握手获取的服务端元数据，
/// 包含服务实现信息和可选的使用说明。
#[derive(Debug, Clone, Serialize)]
pub struct McpServiceDescription {
    /// Service name (from registry connection key)
    pub service_name: String,
    /// Server implementation name (from MCP initialize handshake)
    pub server_name: String,
    /// Server implementation version
    pub server_version: String,
    /// Server title (human-readable, optional)
    pub server_title: Option<String>,
    /// Server description (optional)
    pub server_description: Option<String>,
    /// Human-readable instructions for using this server (optional)
    pub instructions: Option<String>,
    /// MCP protocol version supported by the server
    pub protocol_version: String,
}

/// MCP 工具摘要信息
#[derive(Debug, Clone, Serialize)]
pub struct McpToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
}
