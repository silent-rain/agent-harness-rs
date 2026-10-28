//! `mcp_service_tools` 工具
//!
//! 列出指定 MCP 服务的可用工具，返回每个工具的名称和描述。

use std::collections::HashMap;

use rig_core::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};

use crate::{error::Error, types::mcp::McpToolInfo};

/// 列出指定 MCP 服务的可用工具
///
/// 根据服务名称查询该服务下所有可用工具的名称和描述。
/// 工具在 `McpRegistry::build_tools()` 时通过快照数据构建，
/// 无需持有 `McpRegistry` 引用。
pub struct McpServiceToolsTool {
    /// Snapshot: service_name → tool info list
    service_tools: HashMap<String, Vec<McpToolInfo>>,
}

impl McpServiceToolsTool {
    /// Create from a snapshot of service-to-tools mapping
    ///
    /// # Parameters
    ///
    /// * `service_tools` - Pre-collected mapping from service name to tool info list
    pub fn new(service_tools: HashMap<String, Vec<McpToolInfo>>) -> Self {
        Self { service_tools }
    }
}

/// `mcp_service_tools` tool arguments
#[derive(Debug, Deserialize)]
pub struct McpServiceToolsArgs {
    /// The MCP service name to query tools for
    pub service_name: String,
}

/// `mcp_service_tools` tool output
#[derive(Debug, Serialize)]
pub struct McpServiceToolsOutput {
    /// The queried service name
    pub service_name: String,
    /// Total number of tools
    pub total: usize,
    /// Tool info list
    pub tools: Vec<McpToolInfo>,
}

impl Tool for McpServiceToolsTool {
    const NAME: &'static str = "mcp_service_tools";

    type Error = Error;
    type Args = McpServiceToolsArgs;
    type Output = McpServiceToolsOutput;

    fn description(&self) -> String {
        let service_names: Vec<&str> = self.service_tools.keys().map(|s| s.as_str()).collect();
        format!(
            "List all available tools for a specified MCP service. Available services: [{}]",
            service_names.join(", ")
        )
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "service_name": {
                    "type": "string",
                    "description": "The MCP service name to query tools for"
                }
            },
            "required": ["service_name"]
        })
    }

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let tools = self
            .service_tools
            .get(&args.service_name)
            .cloned()
            .ok_or_else(|| Error::McpServiceNotFound {
                service_name: args.service_name.clone(),
            })?;

        Ok(McpServiceToolsOutput {
            service_name: args.service_name,
            total: tools.len(),
            tools,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_service_tools_found() {
        let mut service_tools = HashMap::new();
        service_tools.insert(
            "filesystem".to_string(),
            vec![
                McpToolInfo {
                    name: "read_file".to_string(),
                    description: "Read a file".to_string(),
                },
                McpToolInfo {
                    name: "write_file".to_string(),
                    description: "Write a file".to_string(),
                },
            ],
        );
        let tool = McpServiceToolsTool::new(service_tools);
        let result = tool
            .call(
                &mut ToolContext::new(),
                McpServiceToolsArgs {
                    service_name: "filesystem".to_string(),
                },
            )
            .await
            .unwrap();
        assert_eq!(result.service_name, "filesystem");
        assert_eq!(result.total, 2);
        assert_eq!(result.tools[0].name, "read_file");
    }

    #[tokio::test]
    async fn test_mcp_service_tools_not_found() {
        let tool = McpServiceToolsTool::new(HashMap::new());
        let result = tool
            .call(
                &mut ToolContext::new(),
                McpServiceToolsArgs {
                    service_name: "nonexistent".to_string(),
                },
            )
            .await;
        assert!(result.is_err());
    }
}
