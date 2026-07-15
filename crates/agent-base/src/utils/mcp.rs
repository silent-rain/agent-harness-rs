//! MCP 工具集模块

use crate::types::mcp::McpServiceEntry;

/// Infer and normalize the transport type from a [`McpServiceEntry`].
///
/// Priority:
/// 1. Explicit `transportType` field — normalized (`-` → `_`)
/// 2. `command` present → `"stdio"`
/// 3. `url` present → `"sse"`
/// 4. Fallback → `"stdio"`
pub fn infer_transport_type(config: &McpServiceEntry) -> String {
    if let Some(t) = &config.transport_type {
        return t.replace('-', "_");
    }
    if config.command.is_some() {
        return "stdio".to_string();
    }
    if config.url.is_some() {
        return "sse".to_string();
    }
    "stdio".to_string()
}
