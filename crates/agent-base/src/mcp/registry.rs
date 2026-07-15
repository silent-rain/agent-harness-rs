//! MCP 连接注册表
//!
//! 独立管理所有 MCP 服务的连接生命周期。
//!
//! ## 架构
//!
//! ```text
//! McpRegistry
//!   ├── connections: HashMap<service_name, McpConnection>
//!   │     ├── server_sink (用于调用远端工具)
//!   │     └── running (持有连接生命周期)
//!   ├── pending_configs: Vec<PendingMcpConfig>  → 待连接的服务配置
//!   └── Methods:
//!     ├── connects(configs)                     → 批量连接服务配置
//!     ├── connect(service)                      → 直接连接单个服务
//!     ├── connect_all()                         → 连接所有 pending 配置
//!     ├── get_enabled_tools(service_name)        → 返回已过白名单过滤的原始工具列表
//!     ├── list_all_enabled_tools()               → 返回所有服务的原始工具列表
//!     ├── list_all_service_names()               → 返回所有已连接服务名称列表（owned）
//!     ├── build_toolset()                        → 构建 ToolSet（含远端工具 + 内省工具）
//!     └── get_service_description(service_name)  → 获取指定服务的描述信息
//! ```
//!
//! ## 设计说明
//!
//! - 创建实例时仅注册配置，不执行 I/O 连接（延迟连接）
//! - 通过 `connects()` 或 `connect()` 执行连接
//! - 本地配置文件扫描由 [`crate::mcp::LocalMcpLoader`] 负责，Registry 只接收 `Vec<(String, McpServerConfig)>`
//! - `McpRegistry` 只负责连接管理和工具列表查询，不做 LLM tool 转换
//! - 将 `rmcp::model::Tool` 转换为 LLM 可调用的 `McpTool` 由调用方负责
//! - 连接生命周期由 `McpRegistry` 内部持有，对外透明（无需 `McpConnectionGuard`）
//! - 每个 MCP 服务独立建立连接，连接失败时记录警告并跳过
//! - `allowed_tools` 白名单：空列表=无可用工具，`["*"]`=所有工具可用
//! - `build_toolset()` 自动注入内省工具（`McpServiceListTool`、`McpServiceToolsTool`）
use std::collections::HashMap;

use rig_core::tool::{ToolDyn, rmcp::McpTool};
use rmcp::service::ServerSink;
use rmcp::{
    model::{ClientCapabilities, ClientInfo, Implementation},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tracing::{debug, info, warn};

use crate::{
    error::{AgentResult, Error},
    mcp::{McpServiceListTool, McpServiceToolsTool},
    types::mcp::{
        McpServerConfig, McpServiceDescription, McpServiceEntry, McpServiceInfo, McpToolInfo,
    },
};
/// 单个 MCP 连接
#[allow(dead_code)]
pub struct McpConnection {
    /// MCP 服务名称
    service_name: String,
    /// 传输类型
    transport_type: String,
    /// MCP 客户端连接句柄（用于调用远端工具）
    server_sink: ServerSink,
    /// 白名单工具列表（空=无可用工具）
    allowed_tools: Vec<String>,
    /// 持有连接生命周期（类型擦除）
    running: Box<dyn std::any::Any + Send + Sync>,
}

/// MCP 连接注册表
///
/// 独立管理所有 MCP 服务的连接生命周期。
/// 采用「配置注册 → 延迟连接」两阶段模式：
///
/// 1. **配置阶段**（同步）：通过 `add_config` / `add_configs` 注册服务配置
/// 2. **连接阶段**（异步）：通过 `connect_all` 执行实际连接
///
/// 连接在 `connect_all()` 时建立，在 Registry drop 时自动关闭。
///
/// 只负责连接管理和工具列表查询，不做 LLM tool 转换。
/// 本地配置文件的读取由 [`crate::mcp::LocalMcpLoader`] 负责，Registry 只接收已解析的配置列表。
///
/// `AgentBuilder::build()` 内部会自动使用本注册表完成 MCP 工具注入，
/// 一般情况下调用方无需直接操作 `McpRegistry`。
///
/// 若需自定义 MCP 构建流程，可通过手动构建后
/// 再通过 `AgentBuilder::with_mcp_registry` 注入。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_base::mcp::{LocalMcpLoader, McpRegistry};
///
/// # async fn example() -> agent_base::error::AgentResult<()> {
/// // Load configs from local file
/// let configs = LocalMcpLoader::default().load().await?;
///
/// // Two-phase: register configs → connect all
/// let mut registry = McpRegistry::default();
/// registry.add_configs(configs);
/// registry.connect_all().await?;
///
/// // Build toolset for runtime injection
/// let toolset = registry.build_tools().await;
/// # Ok(())
/// # }
/// ```
pub struct McpRegistry {
    /// Active MCP connections keyed by service name
    connections: HashMap<String, McpConnection>,
    /// Pending service configs waiting to be connected
    pending_configs: Vec<McpServerConfig>,
    /// MCP 客户端身份信息（agent 名称、版本等）
    client: ClientInfo,
}

impl Default for McpRegistry {
    fn default() -> Self {
        let client = ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("nucleus-agent", "0.1.0"),
        );

        Self {
            connections: HashMap::new(),
            pending_configs: Vec::new(),
            client,
        }
    }
}

impl McpRegistry {
    /// 创建空的注册表，使用默认客户端信息
    pub fn new() -> Self {
        McpRegistry::default()
    }

    /// 设置自定义客户端信息
    ///
    /// 用于需要自定义 agent 名称或版本的场景。
    ///
    /// # Parameters
    ///
    /// * `client` - 自定义 MCP 客户端身份信息
    pub fn with_client(mut self, client: ClientInfo) -> Self {
        self.client = client;
        self
    }

    /// 批量设置 MCP 服务配置
    ///
    /// 将 pending 队列中的旧配置**全部替换**为提供的新配置列表。
    /// 适用于构建阶段一次性设置服务配置，支持链式调用。
    ///
    /// 与 `add_configs` 不同，本方法会清空已有的 pending 配置后再写入，
    /// 确保最终 pending 队列中只包含本次传入的配置。
    ///
    /// # Parameters
    ///
    /// * `configs` - MCP 服务配置列表
    pub fn with_configs(mut self, configs: Vec<McpServerConfig>) -> Self {
        for c in &configs {
            info!(
                service_name = %c.service_name,
                transport = %c.server_config.transport_type.as_deref().unwrap_or("unknown"),
                "MCP service config set (builder, replaces previous pending)"
            );
        }
        self.pending_configs = configs;
        self
    }

    /// 注册单个 MCP 服务配置（不立即连接）
    ///
    /// 配置会被暂存到 pending 队列中，等待 `connect_all()` 时统一建立连接。
    ///
    /// # Parameters
    ///
    /// * `config` - MCP 服务配置
    pub fn add_config(&mut self, config: McpServerConfig) {
        info!(
            service_name = %config.service_name,
            transport = %config.server_config.transport_type.as_deref().unwrap_or("unknown"),
            "MCP service config registered (pending connection)"
        );
        self.pending_configs.push(config);
    }

    /// 批量注册 MCP 服务配置（不立即连接）
    ///
    /// 配置会被暂存到 pending 队列中，等待 `connect_all()` 时统一建立连接。
    ///
    /// # Parameters
    ///
    /// * `configs` - MCP 服务配置列表
    pub fn add_configs(&mut self, configs: Vec<McpServerConfig>) {
        for config in configs {
            self.add_config(config);
        }
    }

    /// 连接所有 pending 配置
    ///
    /// 取出所有待连接的服务配置并逐一建立连接。
    /// 连接失败的服务会记录警告并跳过，不影响其他服务。
    /// 调用后 pending 队列会被清空。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`
    pub async fn connect_all(&mut self) -> AgentResult<()> {
        let configs = std::mem::take(&mut self.pending_configs);

        if configs.is_empty() {
            return Ok(());
        }

        for pending in &configs {
            match self
                .connect(
                    &pending.service_name,
                    &pending.server_config,
                    &pending.allowed_tools,
                )
                .await
            {
                Ok(()) => {
                    info!(
                        service_name = %pending.service_name,
                        transport = %pending.server_config.transport_type.as_deref().unwrap_or("unknown"),
                        "MCP service connected"
                    );
                }
                Err(e) => {
                    warn!(
                        service_name = %pending.service_name,
                        transport = %pending.server_config.transport_type.as_deref().unwrap_or("unknown"),
                        error = %e,
                        "MCP service connection failed, skipped"
                    );
                }
            }
        }

        Ok(())
    }

    /// 获取已连接的服务数量
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// 获取待连接的配置数量
    pub fn pending_count(&self) -> usize {
        self.pending_configs.len()
    }

    /// 获取所有已连接的服务名称
    pub fn service_names(&self) -> Vec<&str> {
        self.connections.keys().map(|s| s.as_str()).collect()
    }

    /// 连接单个 MCP 服务
    pub async fn connect(
        &mut self,
        service_name: &str,
        config: &McpServiceEntry,
        allowed_tools: &[String],
    ) -> AgentResult<()> {
        let transport_type = config
            .transport_type
            .as_deref()
            .unwrap_or("stdio")
            .to_string();
        let (server_sink, running) = match transport_type.as_str() {
            "stdio" => self.connect_stdio(service_name, config).await?,
            "http" | "streamable_http" | "sse" => self.connect_http(service_name, config).await?,
            other => {
                return Err(Error::McpUnsupportedTransport {
                    transport_type: other.to_string(),
                });
            }
        };

        self.connections.insert(
            service_name.to_string(),
            McpConnection {
                service_name: service_name.to_string(),
                transport_type,
                server_sink,
                allowed_tools: allowed_tools.to_vec(),
                running,
            },
        );

        Ok(())
    }

    /// 建立 stdio 连接
    ///
    /// 直接从 [`McpServiceEntry`] 提取 `command`、`args`、`env` 字段，
    /// 构建子进程并建立 MCP 连接。
    async fn connect_stdio(
        &self,
        service_name: &str,
        config: &McpServiceEntry,
    ) -> AgentResult<(ServerSink, Box<dyn std::any::Any + Send + Sync>)> {
        let command = config.command.as_deref().ok_or_else(|| Error::McpConfig {
            service_name: service_name.to_string(),
            message: "stdio config missing 'command' field".to_string(),
        })?;

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(&config.args);

        if !config.env.is_empty() {
            cmd.envs(&config.env);
        }

        let (child_process, _stderr) = rmcp::transport::TokioChildProcess::builder(cmd)
            .spawn()
            .map_err(|e| Error::McpConnection {
                service_name: service_name.to_string(),
                message: format!("Failed to spawn child process: {e}"),
            })?;

        let running = rmcp::serve_client(self.client.clone(), child_process)
            .await
            .map_err(|e| Error::McpConnection {
                service_name: service_name.to_string(),
                message: format!("stdio connection failed: {e}"),
            })?;

        let sink = running.peer().clone();
        Ok((sink, Box::new(running)))
    }

    /// 建立 HTTP/SSE 连接
    ///
    /// 直接从 [`McpServiceEntry`] 提取 `url`、`headers`、`timeout` 字段，
    /// 构建带自定义请求头的 HTTP 传输层并建立 MCP 连接。
    async fn connect_http(
        &self,
        service_name: &str,
        config: &McpServiceEntry,
    ) -> AgentResult<(ServerSink, Box<dyn std::any::Any + Send + Sync>)> {
        let url = config.url.as_deref().ok_or_else(|| Error::McpConfig {
            service_name: service_name.to_string(),
            message: "missing url".to_string(),
        })?;

        // Build custom headers from config
        let mut custom_headers = HashMap::new();
        for (name, value) in &config.headers {
            let header_name =
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|e| {
                    Error::McpConfig {
                        service_name: service_name.to_string(),
                        message: format!("Invalid header name '{name}': {e}"),
                    }
                })?;
            let header_value =
                reqwest::header::HeaderValue::from_str(value).map_err(|e| Error::McpConfig {
                    service_name: service_name.to_string(),
                    message: format!("Invalid header value for '{name}': {e}"),
                })?;
            custom_headers.insert(header_name, header_value);
        }

        let transport_config =
            StreamableHttpClientTransportConfig::with_uri(url).custom_headers(custom_headers);

        // Build reqwest client: apply timeout if configured, otherwise use default
        let http_client = {
            let mut builder = reqwest::Client::builder().pool_max_idle_per_host(0);
            if let Some(timeout_ms) = config.timeout {
                builder = builder.timeout(std::time::Duration::from_millis(timeout_ms));
            }
            builder.build().map_err(|e| Error::McpConnection {
                service_name: service_name.to_string(),
                message: format!("Failed to build HTTP client: {e}"),
            })?
        };
        let transport = StreamableHttpClientTransport::with_client(http_client, transport_config);

        let running = rmcp::serve_client(self.client.clone(), transport)
            .await
            .map_err(|e| Error::McpConnection {
                service_name: service_name.to_string(),
                message: format!("HTTP connection failed: {e}"),
            })?;

        let sink = running.peer().clone();
        Ok((sink, Box::new(running)))
    }
}

impl McpRegistry {
    /// 获取指定服务已过白名单过滤的原始工具列表
    ///
    /// 返回 `rmcp::model::Tool`，不做 LLM tool 转换。
    /// 调用方可通过 `McpTool::from_mcp_server(tool, sink)` 自行转换。
    ///
    /// # Parameters
    ///
    /// * `service_name` - MCP 服务名称
    ///
    /// # Returns
    ///
    /// `(ServerSink, Vec<rmcp::model::Tool>)` — 连接句柄与工具列表
    pub async fn get_enabled_tools(
        &self,
        service_name: &str,
    ) -> AgentResult<(ServerSink, Vec<rmcp::model::Tool>)> {
        let conn = self
            .connections
            .get(service_name)
            .ok_or_else(|| Error::McpServiceNotFound {
                service_name: service_name.to_string(),
            })?;
        let tools = self.filter_tools(conn).await?;
        Ok((conn.server_sink.clone(), tools))
    }

    /// 获取所有服务已过白名单过滤的原始工具列表
    ///
    /// 返回 `(ServerSink, rmcp::model::Tool)` 对，调用方自行转换为 `McpTool`。
    /// 单个服务获取失败时记录警告并跳过，不影响其他服务。
    ///
    /// # Returns
    ///
    /// `Vec<(ServerSink, rmcp::model::Tool)>` — 所有服务的工具列表
    pub async fn list_all_enabled_tools(
        &self,
    ) -> AgentResult<Vec<(ServerSink, rmcp::model::Tool)>> {
        let mut result = Vec::new();

        for conn in self.connections.values() {
            match self.filter_tools(conn).await {
                Ok(tools) => {
                    debug!(
                        service_name = %conn.service_name,
                        tools_count = tools.len(),
                        "MCP tools listed"
                    );
                    for tool in tools {
                        result.push((conn.server_sink.clone(), tool));
                    }
                }
                Err(e) => {
                    warn!(
                        service_name = %conn.service_name,
                        error = %e,
                        "Failed to list tools from MCP service, skipped"
                    );
                }
            }
        }

        Ok(result)
    }

    /// 获取指定服务的所有原始工具列表（不经过白名单过滤）
    ///
    /// 返回该服务注册的全部 `rmcp::model::Tool`，忽略 `allowed_tools` 白名单。
    /// 适用于需要查看服务完整工具能力的场景。
    ///
    /// # Parameters
    ///
    /// * `service_name` - MCP 服务名称
    ///
    /// # Returns
    ///
    /// `(ServerSink, Vec<rmcp::model::Tool>)` — 连接句柄与完整工具列表
    pub async fn get_service_all_tools(
        &self,
        service_name: &str,
    ) -> AgentResult<(ServerSink, Vec<rmcp::model::Tool>)> {
        let conn = self
            .connections
            .get(service_name)
            .ok_or_else(|| Error::McpServiceNotFound {
                service_name: service_name.to_string(),
            })?;
        let tools = self.fetch_tools(conn).await?;
        Ok((conn.server_sink.clone(), tools))
    }

    /// 获取所有已连接的服务名称列表（owned）
    ///
    /// 与 `service_names()` 不同，返回 `Vec<String>` 而非借用引用，
    /// 适用于需要跨异步边界传递的场景。
    ///
    /// # Returns
    ///
    /// `Vec<String>` — 所有已连接服务的名称列表
    pub fn list_all_service_names(&self) -> Vec<String> {
        self.connections.keys().cloned().collect()
    }

    /// 获取指定 MCP 服务的描述信息
    ///
    /// 通过 MCP 协议初始化握手获取的服务端元数据，
    /// 包含服务实现信息（名称、版本、描述）和可选的使用说明（instructions）。
    ///
    /// # Parameters
    ///
    /// * `service_name` - MCP 服务名称
    ///
    /// # Returns
    ///
    /// `McpServiceDescription` — 服务描述信息
    pub fn get_service_description(
        &self,
        service_name: &str,
    ) -> AgentResult<McpServiceDescription> {
        let conn = self
            .connections
            .get(service_name)
            .ok_or_else(|| Error::McpServiceNotFound {
                service_name: service_name.to_string(),
            })?;
        Ok(Self::build_description(conn))
    }

    /// 从连接中构建服务描述信息
    fn build_description(conn: &McpConnection) -> McpServiceDescription {
        let peer_info = conn.server_sink.peer_info();

        match peer_info {
            Some(info) => McpServiceDescription {
                service_name: conn.service_name.clone(),
                server_name: info.server_info.name.clone(),
                server_version: info.server_info.version.clone(),
                server_title: info.server_info.title.clone(),
                server_description: info.server_info.description.clone(),
                instructions: info.instructions.clone(),
                protocol_version: info.protocol_version.as_str().to_string(),
            },
            None => McpServiceDescription {
                service_name: conn.service_name.clone(),
                server_name: String::new(),
                server_version: String::new(),
                server_title: None,
                server_description: None,
                instructions: None,
                protocol_version: String::new(),
            },
        }
    }

    /// 从连接中获取全部原始工具列表（不过滤）
    async fn fetch_tools(&self, conn: &McpConnection) -> AgentResult<Vec<rmcp::model::Tool>> {
        conn.server_sink
            .list_all_tools()
            .await
            .map_err(|e| Error::McpToolList {
                service_name: conn.service_name.clone(),
                message: format!("{e}"),
            })
    }

    /// 从连接中获取已过白名单过滤的原始工具列表
    ///
    /// 白名单语义：
    /// - 空列表 → 无可用工具，返回空列表
    /// - 包含 `"*"` → 所有工具可用（本地配置默认行为）
    /// - 其他 → 仅返回白名单中列出的工具
    async fn filter_tools(&self, conn: &McpConnection) -> AgentResult<Vec<rmcp::model::Tool>> {
        if conn.allowed_tools.is_empty() {
            return Ok(vec![]);
        }

        let tools = self.fetch_tools(conn).await?;

        // Wildcard: all tools allowed
        if conn.allowed_tools.contains(&"*".to_string()) {
            return Ok(tools);
        }

        let result = tools
            .into_iter()
            .filter(|tool| conn.allowed_tools.contains(&tool.name.to_string()))
            .collect();

        Ok(result)
    }
}

/// Rig Tools
impl McpRegistry {
    /// Build MCP tools as a flat list of boxed tools (alternative to
    /// [`Self::build_toolset`]).
    ///
    /// Returns every remote MCP tool plus the introspection tools
    /// (`mcp_service_list`, `mcp_service_tools`) as a single
    /// `Vec<Box<dyn ToolDyn>>`. This shape is friendlier for callers that
    /// pipe tools into [`crate::tool::ToolPool`] via
    /// [`crate::tool::ToolPool::register_many`] / `register_mcp_boxed`,
    /// avoiding any intermediate `ToolSet` whose internals are not iterable
    /// from outside `rig_core`.
    pub async fn build_tools(&self) -> Vec<Box<dyn ToolDyn>> {
        let mut tools: Vec<Box<dyn ToolDyn>> = Vec::new();

        // Collect per-service tool info for introspection tools
        let mut services_info = Vec::new();
        let mut service_tools_map = HashMap::new();

        for conn in self.connections.values() {
            match self.filter_tools(conn).await {
                Ok(remote_tools) => {
                    debug!(
                        service_name = %conn.service_name,
                        tools_count = remote_tools.len(),
                        "MCP tools listed"
                    );

                    let tool_infos: Vec<McpToolInfo> = remote_tools
                        .iter()
                        .map(|t| McpToolInfo {
                            name: t.name.to_string(),
                            description: t.description.as_deref().unwrap_or("").to_string(),
                        })
                        .collect();

                    services_info.push(McpServiceInfo {
                        name: conn.service_name.clone(),
                        transport_type: conn.transport_type.clone(),
                        tool_count: remote_tools.len(),
                    });
                    service_tools_map.insert(conn.service_name.clone(), tool_infos);

                    for tool in remote_tools {
                        tools.push(Box::new(McpTool::from_mcp_server(
                            tool,
                            conn.server_sink.clone(),
                        )));
                    }
                }
                Err(e) => {
                    warn!(
                        service_name = %conn.service_name,
                        error = %e,
                        "Failed to list tools from MCP service, skipped"
                    );
                }
            }
        }

        // Introspection tools
        tools.push(Box::new(McpServiceListTool::new(services_info)));
        tools.push(Box::new(McpServiceToolsTool::new(service_tools_map)));

        tools
    }
}
