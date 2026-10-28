//! 本地 MCP 配置加载器
//!
//! 从本地文件系统读取 `mcp.json` 配置文件，将其解析为 [`McpServerConfig`] 列表。
//!
//! 提供的主要内容：
//! - [`LocalMcpLoader`] - 本地 MCP 配置文件加载器
//! - [`McpServerConfig`] - 包装层（含 `service_name`、`allowed_tools`、`server_config`）
//! - [`McpServiceEntry`] - 原始连接参数（url、command、args、env、timeout、headers）

use std::path::{Path, PathBuf};

use tokio::fs;
use tracing::{debug, info, warn};

use crate::{
    constant::{DEFAULT_MCP_CONFIG_DIR, DEFAULT_MCP_CONFIG_FILE, DEFAULT_WORKSPACE_DIR},
    error::{AgentResult, Error},
    types::mcp::{McpServerConfig, McpServersWrapper},
    utils::mcp::infer_transport_type,
};

/// 本地 MCP 配置加载器
///
/// 读取指定路径的 `mcp.json` 文件，将其解析为 [`McpServerConfig`] 列表。
/// 每个条目包含服务名称、允许工具列表（本地配置固定为 `["*"]`，即全部启用）及连接参数。
///
/// 设计上保持"被动"：仅在 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
/// 加载结果由调用方注入到 [`crate::mcp::McpRegistry`] 中统一建立连接。
///
/// ## 使用示例
///
/// ```rust,no_run
/// use agent_runtime::mcp::{LocalMcpLoader, McpRegistry};
///
/// # async fn example() -> agent_runtime::error::AgentResult<()> {
/// let loader = LocalMcpLoader::default();
/// let configs = loader.load().await?;
///
/// let mut registry = McpRegistry::default();
/// registry.add_configs(configs);
/// registry.connect_all().await?;
/// # Ok(())
/// # }
/// ```
pub struct LocalMcpLoader {
    /// MCP 配置文件路径（如 `.codebuddy/mcp/mcp.json`）
    config_path: PathBuf,
}

impl Default for LocalMcpLoader {
    fn default() -> Self {
        Self {
            config_path: Path::new(DEFAULT_WORKSPACE_DIR)
                .join(DEFAULT_MCP_CONFIG_DIR)
                .join(DEFAULT_MCP_CONFIG_FILE),
        }
    }
}

impl LocalMcpLoader {
    /// 创建本地 MCP 配置加载器
    ///
    /// # Parameters
    ///
    /// * `config_path` - MCP 配置文件路径（如 `.codebuddy/mcp/mcp.json`）
    pub fn new(config_path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: config_path.into(),
        }
    }

    /// 切换工作空间根目录
    ///
    /// 将 `config_path` 重新设置为 `workspace_dir/mcp/mcp.json`。
    ///
    /// # Parameters
    ///
    /// * `workspace_dir` - 工作空间根目录（如 `.codebuddy`）
    ///
    /// # Returns
    ///
    /// 返回 `self` 以支持链式调用
    pub fn with_workspace_dir(mut self, workspace_dir: impl Into<PathBuf>) -> Self {
        self.config_path = workspace_dir
            .into()
            .join(DEFAULT_MCP_CONFIG_DIR)
            .join(DEFAULT_MCP_CONFIG_FILE);
        self
    }

    /// 获取 MCP 配置文件路径
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// 读取并解析本地 MCP 配置文件
    ///
    /// 读取 `config_path` 指向的 JSON 文件，解析为 [`McpServerConfig`] 列表，
    /// 按服务名字典序排列以保证确定性。
    /// 本地配置不提供工具级开关，统一为 `["*"]`（即 [`ToolPolicy::All`]）。
    ///
    /// [`ToolPolicy::All`]: crate::mcp::ToolPolicy::All
    ///
    /// 配置文件不存在时返回空列表（视作"无本地 MCP 配置"，不视为错误）。
    ///
    /// # Returns
    ///
    /// 解析成功的 [`McpServerConfig`] 列表，按服务名字典序排列
    ///
    /// # Errors
    ///
    /// 配置文件存在但读取或解析失败时返回错误
    pub async fn load(&self) -> AgentResult<Vec<McpServerConfig>> {
        if !self.config_path.exists() {
            debug!(
                config_path = %self.config_path.display(),
                "Local MCP config file does not exist, skipping"
            );
            return Ok(Vec::new());
        }

        let content =
            fs::read_to_string(&self.config_path)
                .await
                .map_err(|e| Error::McpConfig {
                    service_name: "local".to_string(),
                    message: format!(
                        "Failed to read local MCP config file '{}': {e}",
                        self.config_path.display()
                    ),
                })?;

        let local_file: McpServersWrapper =
            serde_json::from_str(&content).map_err(|e| Error::McpConfig {
                service_name: "local".to_string(),
                message: format!(
                    "Failed to parse local MCP config file '{}': {e}",
                    self.config_path.display()
                ),
            })?;

        let mut entries: Vec<McpServerConfig> = local_file
            .mcp_servers
            .into_iter()
            .map(|(name, mut entry)| {
                // Normalize transport_type in-place (e.g. "streamable-http" → "streamable_http"),
                // inferring from command/url when the field is absent.
                entry.transport_type = Some(infer_transport_type(&entry));
                McpServerConfig {
                    service_name: name,
                    // 本地配置无工具级开关：空列表/`["*"]` 均由 ToolPolicy 归一为「全部启用」
                    allowed_tools: vec!["*".to_string()],
                    server_config: entry,
                }
            })
            .collect();
        entries.sort_by(|a, b| a.service_name.cmp(&b.service_name));

        let count = entries.len();
        if count > 0 {
            info!(
                config_path = %self.config_path.display(),
                services_found = count,
                "Local MCP config file loaded"
            );
        } else {
            warn!(
                config_path = %self.config_path.display(),
                "Local MCP config file contains no server entries"
            );
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::TempDir;

    fn write_mcp_json(dir: &Path, content: &str) -> PathBuf {
        let path = dir.join("mcp.json");
        std_fs::write(&path, content).unwrap();
        path
    }

    // =========================================================================
    // LocalMcpLoader tests
    // =========================================================================

    #[tokio::test]
    async fn test_load_nonexistent_file_returns_empty() {
        let loader = LocalMcpLoader::new("/nonexistent/path/mcp.json");
        let configs = loader.load().await.unwrap();
        assert!(configs.is_empty());
    }

    #[tokio::test]
    async fn test_load_empty_servers_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = write_mcp_json(tmp.path(), r#"{"mcpServers": {}}"#);
        let loader = LocalMcpLoader::new(path);
        let configs = loader.load().await.unwrap();
        assert!(configs.is_empty());
    }

    #[tokio::test]
    async fn test_load_http_service() {
        let tmp = TempDir::new().unwrap();
        let path = write_mcp_json(
            tmp.path(),
            r#"{
                "mcpServers": {
                    "my-service": {
                        "url": "http://127.0.0.1:8080/sse",
                        "transportType": "sse"
                    }
                }
            }"#,
        );
        let loader = LocalMcpLoader::new(path);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let entry = &configs[0];
        assert_eq!(entry.service_name, "my-service");
        assert_eq!(entry.allowed_tools, vec!["*"]);
        assert_eq!(entry.server_config.transport_type.as_deref(), Some("sse"));
        assert_eq!(
            entry.server_config.url.as_deref(),
            Some("http://127.0.0.1:8080/sse")
        );
    }

    #[tokio::test]
    async fn test_load_multiple_services_sorted() {
        let tmp = TempDir::new().unwrap();
        let path = write_mcp_json(
            tmp.path(),
            r#"{
                "mcpServers": {
                    "z-service": { "url": "http://z.example.com", "transportType": "http" },
                    "a-service": { "url": "http://a.example.com", "transportType": "http" }
                }
            }"#,
        );
        let loader = LocalMcpLoader::new(path);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].service_name, "a-service");
        assert_eq!(configs[1].service_name, "z-service");
    }

    #[tokio::test]
    async fn test_load_invalid_json_returns_error() {
        let tmp = TempDir::new().unwrap();
        let path = write_mcp_json(tmp.path(), "not valid json");
        let loader = LocalMcpLoader::new(path);
        let result = loader.load().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_all_transport_types() {
        let tmp = TempDir::new().unwrap();
        let path = write_mcp_json(
            tmp.path(),
            r#"{
                "mcpServers": {
                    "simple-tool": {
                        "url": "http://127.0.0.1:8081/sse",
                        "transportType": "sse",
                        "timeout": 30000,
                        "headers": { "X-User-Custom": "custom-value" }
                    },
                    "demo-mcp": {
                        "url": "http://127.0.0.1:8081/mcp",
                        "transportType": "streamable-http",
                        "timeout": 30000
                    },
                    "dash-mcp": {
                        "command": "npx",
                        "args": ["@tencent/dash-mcp@latest"],
                        "transportType": "stdio",
                        "env": { "API_KEY": "xxx" }
                    },
                    "url_fetch": {
                        "command": "uvx",
                        "args": ["mcp-server-fetch"]
                    }
                }
            }"#,
        );
        let loader = LocalMcpLoader::new(path);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 4);
        // Sorted by name: dash-mcp, demo-mcp, simple-tool, url_fetch
        let entry = &configs[0];
        assert_eq!(entry.service_name, "dash-mcp");
        assert_eq!(entry.server_config.transport_type.as_deref(), Some("stdio"));
        assert!(entry.server_config.url.is_none());

        let entry = &configs[1];
        assert_eq!(entry.service_name, "demo-mcp");
        assert_eq!(
            entry.server_config.transport_type.as_deref(),
            Some("streamable_http")
        );

        let entry = &configs[2];
        assert_eq!(entry.service_name, "simple-tool");
        assert_eq!(entry.server_config.transport_type.as_deref(), Some("sse"));

        let entry = &configs[3];
        assert_eq!(entry.service_name, "url_fetch");
        assert_eq!(entry.server_config.transport_type.as_deref(), Some("stdio"));
    }

    #[test]
    fn test_with_workspace_dir() {
        let loader = LocalMcpLoader::default().with_workspace_dir("/tmp/my-workspace");
        assert_eq!(
            loader.config_path(),
            Path::new("/tmp/my-workspace/mcp/mcp.json")
        );
    }
}
