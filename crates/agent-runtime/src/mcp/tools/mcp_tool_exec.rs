//! MCP 远端工具执行器
//!
//! 把 MCP 服务端声明的单个工具包装为实现了 rig [`Tool`] 的类型，与
//! [`crate::a2a::A2aAgentTool`] 等远端工具保持同一套实现形态：
//!
//! - 元数据（名称 / 描述 / `input_schema`）直接取自 MCP 工具声明；
//! - 调用时通过 [`Peer::call_tool_once`] 发起 `tools/call`，内容块折叠为纯文本
//!   （非文本块以 JSON 文本透传，避免信息丢失）；
//! - 单次调用带超时保护，防止远端 hang 住把整个 Agent 轮次拖死；
//! - 工具名是运行期才确定的（服务端命名），因此通过 [`McpToolExecTool::into_dynamic_tool`]
//!   转成 [`DynamicTool`] 时覆盖 [`Tool::NAME`]。
//!
//! 提供的主要内容：
//! - [`McpToolExecTool`] - 单个 MCP 远端工具的执行器
//! - [`DEFAULT_MCP_TOOL_TIMEOUT`] - 单次调用默认超时

use std::time::Duration;

use rig_core::tool::{DynamicTool, Tool, ToolContext, ToolExecutionError};
use rmcp::{
    model::{CallToolRequestParams, CallToolResponse, ContentBlock},
    service::{Peer, RoleClient},
};

use crate::tool::dynamic_tool_named;

/// MCP 远端工具单次调用超时
pub const DEFAULT_MCP_TOOL_TIMEOUT: Duration = Duration::from_secs(300);

/// MCP 远端工具执行器
///
/// 与远端服务一一对应：一个 MCP 工具声明 → 一个 [`McpToolExecTool`] 实例。
///
/// ## 工作原理
///
/// ```text
/// LLM 调用 <service_tool_name>({...})
///   ↓
/// McpToolExecTool::call()
///   ↓
/// Peer::call_tool_once(tools/call)
///   ↓
/// 内容块折叠为文本返回给 LLM
/// ```
pub struct McpToolExecTool {
    /// 服务端声明的工具名（`tools/call` 使用）
    tool_name: String,
    /// 工具描述（LLM 可见）
    description: String,
    /// 入参 JSON Schema（服务端声明原样透传）
    parameters: serde_json::Value,
    /// 所属 MCP 服务的客户端连接句柄
    peer: Peer<RoleClient>,
}

impl McpToolExecTool {
    /// 从 MCP 工具声明与所属连接创建执行器
    ///
    /// # Parameters
    ///
    /// * `tool` - MCP 服务端声明的工具（名称 / 描述 / JSON Schema）
    /// * `peer` - 该工具所属服务的客户端连接句柄
    pub fn new(tool: rmcp::model::Tool, peer: Peer<RoleClient>) -> Self {
        Self {
            tool_name: tool.name.to_string(),
            description: tool.description.as_deref().unwrap_or("").to_string(),
            parameters: serde_json::Value::Object((*tool.input_schema).clone()),
            peer,
        }
    }

    /// 服务端声明的工具名
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    /// 转成带运行期名称（服务端工具名）的 [`DynamicTool`]。
    ///
    /// rig 的 [`Tool::NAME`] 是常量，运行期名称只能在这一层覆盖。
    pub fn into_dynamic_tool(self) -> DynamicTool {
        let name = self.tool_name.clone();
        dynamic_tool_named(&name, self)
    }
}

impl Tool for McpToolExecTool {
    /// 静态占位名；真正的逐实例名称（服务端工具名）在转成
    /// [`DynamicTool`] 时（[`Self::into_dynamic_tool`]）覆盖。
    const NAME: &'static str = "mcp_tool_exec";
    /// MCP 工具的入参由服务端 schema 决定，这里保留原始 JSON 对象
    type Args = serde_json::Value;
    /// 内容块已折叠为纯文本，`String` 经 [`IntoToolOutput`] 天然映射为文本输出
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        self.description.clone()
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let mut params = CallToolRequestParams::new(self.tool_name.clone());
        if let serde_json::Value::Object(map) = args {
            params = params.with_arguments(map);
        }

        let response =
            tokio::time::timeout(DEFAULT_MCP_TOOL_TIMEOUT, self.peer.call_tool_once(params))
                .await
                .map_err(|_| {
                    ToolExecutionError::other(format!(
                        "MCP tool `{}` timed out after {}s",
                        self.tool_name,
                        DEFAULT_MCP_TOOL_TIMEOUT.as_secs()
                    ))
                })?
                .map_err(|e| ToolExecutionError::other(format!("MCP call failed: {e}")))?;

        let result = match response {
            CallToolResponse::Complete(result) => result,
            other => {
                return Err(ToolExecutionError::other(format!(
                    "MCP tool `{}` returned an unsupported response: {other:?}",
                    self.tool_name
                )));
            }
        };

        let text = blocks_to_text(&result.content);
        if result.is_error.unwrap_or(false) {
            return Err(ToolExecutionError::other(text));
        }

        Ok(text)
    }
}

/// 把 MCP 内容块折叠为文本：文本块直接取文本，其余块序列化为 JSON。
fn blocks_to_text(blocks: &[ContentBlock]) -> String {
    let mut parts = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(text.text.clone()),
            other => parts.push(
                serde_json::to_string(other)
                    .unwrap_or_else(|_| String::from("<unserializable content block>")),
            ),
        }
    }
    parts.join("\n")
}
