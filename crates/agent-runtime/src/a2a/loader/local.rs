//! 本地 A2A 配置加载器
//!
//! 从本地文件系统读取 `a2a.json` 配置文件，将其解析为 [`A2aServiceConfig`] 列表。
//!
//! 提供的主要内容：
//! - [`LocalA2aLoader`] - 本地 A2A 配置文件加载器

use std::path::{Path, PathBuf};

use tokio::fs;
use tracing::{debug, info, warn};

use crate::{
    constant::{DEFAULT_A2A_CONFIG_DIR, DEFAULT_A2A_CONFIG_FILE, DEFAULT_WORKSPACE_DIR},
    error::{AgentResult, Error},
    types::a2a::{A2aServersWrapper, A2aServiceConfig},
};

/// 本地 A2A 配置加载器
///
/// 读取指定路径的 `a2a.json` 文件，将其解析为 [`A2aServiceConfig`] 列表。
///
/// 设计上保持"被动"：仅在 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
/// 加载结果由调用方注入到 [`crate::types::runtime::AgentBundle`] 的 `a2a_configs` 字段中。
///
/// ## 配置文件格式
///
/// `a2a.json` 与 `mcp.json` 格式对称，使用 `a2aServers` 顶层字段包装：
///
/// ```json
/// {
///   "a2aServers": {
///     "weather_agent": {
///       "base_url": "http://weather-agent:3000",
///       "description": "Query weather information",
///       "timeout_secs": 60,
///       "headers": {
///         "Authorization": "Bearer your-token"
///       }
///     },
///     "math_agent": {
///       "base_url": "http://math-agent:3000"
///     }
///   }
/// }
/// ```
///
/// ## 使用示例
///
/// ```rust,no_run
/// use agent_runtime::a2a::LocalA2aLoader;
///
/// # async fn example() -> agent_runtime::error::AgentResult<()> {
/// let loader = LocalA2aLoader::default();
/// let configs = loader.load().await?;
/// println!("Loaded {} A2A services", configs.len());
/// # Ok(())
/// # }
/// ```
pub struct LocalA2aLoader {
    /// A2A 配置文件路径（如 `.codebuddy/a2a/a2a.json`）
    config_path: PathBuf,
}

impl Default for LocalA2aLoader {
    fn default() -> Self {
        Self {
            config_path: Path::new(DEFAULT_WORKSPACE_DIR)
                .join(DEFAULT_A2A_CONFIG_DIR)
                .join(DEFAULT_A2A_CONFIG_FILE),
        }
    }
}

impl LocalA2aLoader {
    /// 创建本地 A2A 配置加载器
    ///
    /// # Parameters
    ///
    /// * `config_path` - A2A 配置文件路径（如 `.codebuddy/a2a/a2a.json`）
    pub fn new(config_path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: config_path.into(),
        }
    }

    /// 切换工作空间根目录
    ///
    /// 将 `config_path` 重新设置为 `workspace_dir/a2a/a2a.json`。
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
            .join(DEFAULT_A2A_CONFIG_DIR)
            .join(DEFAULT_A2A_CONFIG_FILE);
        self
    }

    /// 获取 A2A 配置文件路径
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// 读取并解析本地 A2A 配置文件
    ///
    /// 读取 `config_path` 指向的 JSON 文件，解析为 [`LocalA2aFile`]，
    /// 然后将所有服务配置转换为 [`A2aServiceConfig`] 列表返回。
    ///
    /// 配置文件不存在时返回空列表（视作"无本地 A2A 配置"，不视为错误）。
    ///
    /// # Returns
    ///
    /// 解析成功的服务配置列表，按服务名字典序排列以保证确定性
    ///
    /// # Errors
    ///
    /// 配置文件存在但读取或解析失败时返回错误
    pub async fn load(&self) -> AgentResult<Vec<A2aServiceConfig>> {
        if !self.config_path.exists() {
            debug!(
                config_path = %self.config_path.display(),
                "Local A2A config file does not exist, skipping"
            );
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.config_path).await.map_err(|e| {
            Error::Internal(format!(
                "Failed to read local A2A config file '{}': {e}",
                self.config_path.display()
            ))
        })?;

        let wrapper: A2aServersWrapper = serde_json::from_str(&content).map_err(|e| {
            Error::Internal(format!(
                "Failed to parse local A2A config file '{}': {e}",
                self.config_path.display()
            ))
        })?;

        let mut configs: Vec<A2aServiceConfig> = wrapper
            .a2a_servers
            .into_iter()
            .map(|(name, mut entry)| {
                entry.service_name = name;
                entry
            })
            .collect();
        // 按服务名字典序排列，保证确定性
        configs.sort_by(|a, b| a.service_name.cmp(&b.service_name));
        let count = configs.len();

        if count > 0 {
            info!(
                config_path = %self.config_path.display(),
                services_found = count,
                "Local A2A config file loaded"
            );
        } else {
            warn!(
                config_path = %self.config_path.display(),
                "Local A2A config file contains no service entries"
            );
        }

        Ok(configs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::TempDir;

    fn write_a2a_json(dir: &Path, content: &str) -> PathBuf {
        let path = dir.join("a2a.json");
        std_fs::write(&path, content).unwrap();
        path
    }

    #[tokio::test]
    async fn test_load_nonexistent_file_returns_empty() {
        let loader = LocalA2aLoader::new("/nonexistent/path/a2a.json");
        let configs = loader.load().await.unwrap();
        assert!(configs.is_empty());
    }

    #[tokio::test]
    async fn test_load_empty_a2a_servers_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = write_a2a_json(tmp.path(), r#"{"a2aServers": {}}"#);
        let loader = LocalA2aLoader::new(path);
        let configs = loader.load().await.unwrap();
        assert!(configs.is_empty());
    }

    #[tokio::test]
    async fn test_load_single_service() {
        let tmp = TempDir::new().unwrap();
        let path = write_a2a_json(
            tmp.path(),
            r#"{
                "a2aServers": {
                    "weather_agent": {
                        "base_url": "http://weather-agent:3000",
                        "description": "Query weather information",
                        "timeout_secs": 30
                    }
                }
            }"#,
        );
        let loader = LocalA2aLoader::new(path);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].service_name, "weather_agent");
        assert_eq!(configs[0].base_url, "http://weather-agent:3000");
        assert_eq!(configs[0].description, "Query weather information");
        assert_eq!(configs[0].timeout_secs, Some(30));
        assert!(configs[0].headers.is_empty());
    }

    #[tokio::test]
    async fn test_load_service_with_headers() {
        let tmp = TempDir::new().unwrap();
        let path = write_a2a_json(
            tmp.path(),
            r#"{
                "a2aServers": {
                    "secure_agent": {
                        "base_url": "http://secure-agent:3000",
                        "headers": {
                            "Authorization": "Bearer token123",
                            "X-Api-Key": "my-key"
                        }
                    }
                }
            }"#,
        );
        let loader = LocalA2aLoader::new(path);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        assert_eq!(
            configs[0].headers.get("Authorization").map(|s| s.as_str()),
            Some("Bearer token123")
        );
        assert_eq!(
            configs[0].headers.get("X-Api-Key").map(|s| s.as_str()),
            Some("my-key")
        );
    }

    #[tokio::test]
    async fn test_load_multiple_services_sorted() {
        let tmp = TempDir::new().unwrap();
        let path = write_a2a_json(
            tmp.path(),
            r#"{
                "a2aServers": {
                    "z_agent": { "base_url": "http://z-agent:3000" },
                    "a_agent": { "base_url": "http://a-agent:3000" }
                }
            }"#,
        );
        let loader = LocalA2aLoader::new(path);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].service_name, "a_agent");
        assert_eq!(configs[1].service_name, "z_agent");
    }

    #[tokio::test]
    async fn test_load_invalid_json_returns_error() {
        let tmp = TempDir::new().unwrap();
        let path = write_a2a_json(tmp.path(), "not valid json");
        let loader = LocalA2aLoader::new(path);
        let result = loader.load().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_with_workspace_dir() {
        let loader = LocalA2aLoader::default().with_workspace_dir("/tmp/my-workspace");
        assert_eq!(
            loader.config_path(),
            Path::new("/tmp/my-workspace/a2a/a2a.json")
        );
    }
}
