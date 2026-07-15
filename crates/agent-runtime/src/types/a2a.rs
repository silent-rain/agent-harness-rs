//! A2A 服务配置类型
//!
//! 提供的主要内容：
//! - [`A2aServiceConfig`] - A2A 远程 Agent 服务配置
//! - [`A2aServersWrapper`] - `a2aServers` 顶层包裹结构（用于 JSON 解析）

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A2A 远程 Agent 服务配置
///
/// 描述一个远程 A2A Agent 服务的连接参数，与 [`crate::types::mcp::McpServerConfig`] 对称。
/// 由 `LocalA2aLoader`（本地 `a2a.json`）或 `DbA2aLoader`（数据库）加载产出，
/// 注入到 [`crate::types::runtime::AgentBundle`] 的 `a2a_configs` 字段中。
///
/// ## 使用示例
///
/// ```rust,no_run
/// use agent_base::types::a2a::A2aServiceConfig;
///
/// let config = A2aServiceConfig {
///     service_name: "weather_agent".to_string(),
///     description: "Query weather information from remote agent".to_string(),
///     base_url: "http://weather-agent:3000".to_string(),
///     timeout_secs: Some(60),
///     headers: Default::default(),
/// };
/// ```
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct A2aServiceConfig {
    /// 工具名称（LLM 可见，如 `"weather_agent"`）
    ///
    /// 在 ReAct 循环中作为工具名暴露给 LLM，应使用 snake_case 命名。
    #[serde(default)]
    pub service_name: String,
    /// 工具描述（LLM 可见）
    ///
    /// 优先使用此字段，若为空则从远程 AgentCard 自动发现。
    #[serde(default)]
    pub description: String,
    /// 远程 A2A 服务基础 URL（如 `"http://remote-agent:3000"`）
    pub base_url: String,
    /// 连接超时（秒），`None` 使用默认值 60 秒
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// 自定义 HTTP 请求头（如 `Authorization`、`X-Api-Key` 等）
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
}

/// `a2a.json` / `config_json` 顶层包裹结构
///
/// 与 `McpServersWrapper` 对称，用于解析 `a2aServers` 包裹格式的 JSON：
///
/// ```json
/// {
///   "a2aServers": {
///     "weather_agent": {
///       "base_url": "http://weather-agent:3000",
///       "headers": { "Authorization": "Bearer token" }
///     }
///   }
/// }
/// ```
///
/// 主要使用场景：
/// - `LocalA2aLoader` 解析本地 `a2a.json` 文件
/// - `DbA2aLoader` 解析数据库 `config_json` 字段
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct A2aServersWrapper {
    /// 服务配置映射（键为服务名称）
    #[serde(rename = "a2aServers", default)]
    pub a2a_servers: HashMap<String, A2aServiceConfig>,
}
