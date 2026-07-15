//! `mcp_service_list` 工具
//!
//! 列出当前所有已连接的 MCP 服务，返回每个服务的名称、传输类型和可用工具数量。

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::{error::Error, types::mcp::McpServiceInfo};

/// 列出所有已连接的 MCP 服务
///
/// 返回每个服务的名称、传输类型和可用工具数量。
/// 工具在 `McpRegistry::build_toolset()` 时通过快照数据构建，
/// 无需持有 `McpRegistry` 引用。
pub struct McpServiceListTool {
    /// Snapshot of connected services
    services: Vec<McpServiceInfo>,
}

impl McpServiceListTool {
    /// Create from a snapshot of service information
    ///
    /// # Parameters
    ///
    /// * `services` - Pre-collected service info list
    pub fn new(services: Vec<McpServiceInfo>) -> Self {
        Self { services }
    }
}

/// `mcp_service_list` tool arguments (no parameters required)
#[derive(Debug, Deserialize)]
pub struct McpServiceListArgs {}

/// `mcp_service_list` tool output
#[derive(Debug, Serialize)]
pub struct McpServiceListOutput {
    /// Total number of connected services
    pub total: usize,
    /// Service info list
    pub services: Vec<McpServiceInfo>,
}

impl Tool for McpServiceListTool {
    const NAME: &'static str = "mcp_service_list";

    type Error = Error;
    type Args = McpServiceListArgs;
    type Output = McpServiceListOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "mcp_service_list".to_string(),
            description: "List all currently connected MCP services, including their names, transport types, and available tool counts.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(McpServiceListOutput {
            total: self.services.len(),
            services: self.services.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_service_list_empty() {
        let tool = McpServiceListTool::new(vec![]);
        let result = tool.call(McpServiceListArgs {}).await.unwrap();
        assert_eq!(result.total, 0);
        assert!(result.services.is_empty());
    }

    #[tokio::test]
    async fn test_mcp_service_list_with_data() {
        let services = vec![
            McpServiceInfo {
                name: "filesystem".to_string(),
                transport_type: "stdio".to_string(),
                tool_count: 3,
            },
            McpServiceInfo {
                name: "web-search".to_string(),
                transport_type: "http".to_string(),
                tool_count: 1,
            },
        ];
        let tool = McpServiceListTool::new(services);
        let result = tool.call(McpServiceListArgs {}).await.unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.services[0].name, "filesystem");
        assert_eq!(result.services[1].tool_count, 1);
    }
}
