//! MCP 连接注册表
//!
//! 独立管理所有 MCP 服务的连接生命周期。
//!
//! ## 架构
//!
//! ```text
//! McpRegistry
//!   ├── connections: HashMap<service_name, McpConnection>
//!   │     ├── peer    (用于调用远端工具)
//!   │     ├── policy  (工具启用策略)
//!   │     └── running (持有连接生命周期)
//!   ├── pending_configs: Vec<McpServerConfig>  → 待连接的服务配置
//!   └── Methods:
//!     ├── add_config / add_configs / with_configs  → 注册待连接配置
//!     ├── connect_all()                            → 连接所有 pending 配置
//!     ├── service_names()                          → 已连接服务名称
//!     ├── service_description(service_name)        → 服务握手元数据
//!     ├── tools_of(service_name)                   → 单服务可用工具（已过策略）
//!     └── build_tools()                            → 转 rig DynamicTool（含内省工具）
//! ```
//!
//! ## 设计说明
//!
//! - 创建实例时仅注册配置，不执行 I/O 连接（延迟连接）
//! - 通过 `connect_all()` 批量连接；单服务失败记录警告并跳过，不影响其他服务
//! - 本地配置文件扫描由 [`crate::mcp::LocalMcpLoader`] 负责，Registry 只接收 `Vec<McpServerConfig>`
//! - `McpRegistry` 只负责连接管理和工具查询，`rmcp::model::Tool` → rig 工具由本模块内部完成
//! - 连接生命周期由 `McpRegistry` 内部持有，对外透明（无需 `McpConnectionGuard`）
//! - 工具启用语义**只有一份**：[`ToolPolicy`]。空列表或含 `"*"` = 全部启用；
//!   需要「一个工具都不给」时不要绑定该服务（`enabled = false`），而不是给空列表
//! - `build_tools()` 自动注入内省工具（`McpServiceListTool`、`McpServiceToolsTool`），
//!   内省工具看到的是**过滤后**的工具，与 LLM 实际可调用范围一致
use std::collections::{HashMap, HashSet};

use rig_core::tool::DynamicTool;
use rmcp::{
    model::{ClientCapabilities, ClientConfig, Implementation},
    service::{Peer, RoleClient},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tracing::{debug, info, warn};

use crate::{
    error::{AgentResult, Error},
    mcp::{McpServiceListTool, McpServiceToolsTool, McpToolExecTool},
    tool::dynamic_tool,
    types::mcp::{
        McpServerConfig, McpServiceDescription, McpServiceEntry, McpServiceInfo, McpToolInfo,
    },
};
/// MCP 工具启用策略
///
/// 由 [`McpServerConfig::allowed_tools`] 归一而来，是「哪些工具对 LLM 可见」的**唯一**解释点：
///
/// - 空列表或缺省 → [`ToolPolicy::All`]
/// - 列表中含 `"*"` → [`ToolPolicy::All`]
/// - 其余 → [`ToolPolicy::Only`]
///
/// 「一个工具都不给」不在工具粒度表达：应直接把服务置于未启用状态（不绑定 / `enabled = false`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToolPolicy {
    /// 不限制：服务声明的全部工具均可用
    #[default]
    All,
    /// 仅允许列出的工具
    Only(HashSet<String>),
}

impl ToolPolicy {
    /// 从配置的允许列表归一为策略（唯一入口）
    ///
    /// # Parameters
    ///
    /// * `allowed_tools` - 配置中的工具列表，空或含 `"*"` 表示全部启用
    pub fn from_config(allowed_tools: &[String]) -> Self {
        if allowed_tools.is_empty() || allowed_tools.iter().any(|name| name == "*") {
            return Self::All;
        }
        Self::Only(allowed_tools.iter().cloned().collect())
    }

    /// 判断单个工具是否允许调用
    ///
    /// # Parameters
    ///
    /// * `tool_name` - MCP 服务端声明的工具名
    pub fn allows(&self, tool_name: &str) -> bool {
        match self {
            Self::All => true,
            Self::Only(names) => names.contains(tool_name),
        }
    }

    /// 应用策略过滤工具列表
    ///
    /// `All` 时原样返回，不做任何复制。
    ///
    /// # Parameters
    ///
    /// * `tools` - MCP 服务端声明的完整工具列表
    pub fn apply(&self, tools: Vec<rmcp::model::Tool>) -> Vec<rmcp::model::Tool> {
        match self {
            Self::All => tools,
            Self::Only(_) => tools
                .into_iter()
                .filter(|tool| self.allows(tool.name.as_ref()))
                .collect(),
        }
    }
}

/// 单个 MCP 连接
pub struct McpConnection {
    /// MCP 服务名称
    service_name: String,
    /// 传输类型
    transport_type: String,
    /// MCP 客户端对端句柄（用于调用远端工具 / 拉取工具列表）
    peer: Peer<RoleClient>,
    /// 连接建立时握手得到服务端元数据快照
    description: McpServiceDescription,
    /// 工具启用策略
    policy: ToolPolicy,
    /// 持有连接生命周期（类型擦除）
    #[allow(dead_code)]
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
/// use agent_runtime::mcp::{LocalMcpLoader, McpRegistry};
///
/// # async fn example() -> agent_runtime::error::AgentResult<()> {
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
    client: ClientConfig,
}

impl Default for McpRegistry {
    fn default() -> Self {
        let client = ClientConfig::new(
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
    pub fn with_client(mut self, client: ClientConfig) -> Self {
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

        for config in &configs {
            match self.connect(config).await {
                Ok(()) => {
                    info!(
                        service_name = %config.service_name,
                        transport = %config.server_config.transport_type.as_deref().unwrap_or("unknown"),
                        "MCP service connected"
                    );
                }
                Err(e) => {
                    warn!(
                        service_name = %config.service_name,
                        transport = %config.server_config.transport_type.as_deref().unwrap_or("unknown"),
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
    ///
    /// 工具启用范围由 [`McpServerConfig::allowed_tools`] 归一为 [`ToolPolicy`] 后随连接保存。
    ///
    /// # Parameters
    ///
    /// * `server` - 单个服务的完整绑定配置（服务名 + 允许工具 + 连接参数）
    pub async fn connect(&mut self, server: &McpServerConfig) -> AgentResult<()> {
        let service_name = server.service_name.as_str();
        let config = &server.server_config;
        let transport_type = config
            .transport_type
            .as_deref()
            .unwrap_or("stdio")
            .to_string();
        let (peer, running) = match transport_type.as_str() {
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
                description: Self::snapshot_description(service_name, &peer),
                peer,
                policy: ToolPolicy::from_config(&server.allowed_tools),
                running,
            },
        );

        Ok(())
    }

    /// 从握手结果生成服务端元数据快照
    ///
    /// rmcp 3.8 起服务端身份是可选的（discovery 响应不强制提供），
    /// 缺失时各字段退化为空值而非报错。
    ///
    /// # Parameters
    ///
    /// * `service_name` - MCP 服务名称
    /// * `peer` - 已完成握手的客户端句柄
    fn snapshot_description(service_name: &str, peer: &Peer<RoleClient>) -> McpServiceDescription {
        match peer.peer_info() {
            Some(info) => {
                let server = info.server_info.clone().unwrap_or_default();
                McpServiceDescription {
                    service_name: service_name.to_string(),
                    server_name: server.name.clone(),
                    server_version: server.version.clone(),
                    server_title: server.title.clone(),
                    server_description: server.description.clone(),
                    instructions: info.instructions.clone(),
                    protocol_version: info.protocol_version.as_str().to_string(),
                }
            }
            None => McpServiceDescription {
                service_name: service_name.to_string(),
                server_name: String::new(),
                server_version: String::new(),
                server_title: None,
                server_description: None,
                instructions: None,
                protocol_version: String::new(),
            },
        }
    }

    /// 建立 stdio 连接
    ///
    /// 直接从 [`McpServiceEntry`] 提取 `command`、`args`、`env` 字段，
    /// 构建子进程并建立 MCP 连接。
    async fn connect_stdio(
        &self,
        service_name: &str,
        config: &McpServiceEntry,
    ) -> AgentResult<(Peer<RoleClient>, Box<dyn std::any::Any + Send + Sync>)> {
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
    ) -> AgentResult<(Peer<RoleClient>, Box<dyn std::any::Any + Send + Sync>)> {
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

        // rmcp 3.8 起自定义请求头通过 config 字段直接设置（不再有 builder 方法）
        let mut transport_config = StreamableHttpClientTransportConfig::with_uri(url);
        transport_config.custom_headers = custom_headers;

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
    /// 获取指定服务的可用工具列表（已应用 [`ToolPolicy`]）
    ///
    /// 返回 `rmcp::model::Tool`，不做 LLM tool 转换。
    ///
    /// # Parameters
    ///
    /// * `service_name` - MCP 服务名称
    ///
    /// # Errors
    ///
    /// 服务未连接（[`Error::McpServiceNotFound`]）或拉取工具失败（[`Error::McpToolList`]）
    pub async fn tools_of(&self, service_name: &str) -> AgentResult<Vec<rmcp::model::Tool>> {
        let conn = self.connection(service_name)?;
        let tools = self.list_tools(conn).await?;
        Ok(conn.policy.apply(tools))
    }

    /// 获取指定 MCP 服务的描述信息
    ///
    /// 通过 MCP 协议初始化握手获取的服务端元数据，
    /// 包含服务实现信息（名称、版本、描述）和可选的使用说明（instructions）。
    ///
    /// # Parameters
    ///
    /// * `service_name` - MCP 服务名称
    pub fn service_description(&self, service_name: &str) -> AgentResult<McpServiceDescription> {
        Ok(self.connection(service_name)?.description.clone())
    }

    /// 按服务名取连接，不存在时报 [`Error::McpServiceNotFound`]
    fn connection(&self, service_name: &str) -> AgentResult<&McpConnection> {
        self.connections
            .get(service_name)
            .ok_or_else(|| Error::McpServiceNotFound {
                service_name: service_name.to_string(),
            })
    }

    /// 从连接中拉取服务端声明的完整工具列表（未过滤）
    ///
    /// 仅本模块内部使用：对外一律通过 [`Self::tools_of`] 暴露过滤后的视图。
    async fn list_tools(&self, conn: &McpConnection) -> AgentResult<Vec<rmcp::model::Tool>> {
        conn.peer
            .list_all_tools()
            .await
            .map_err(|e| Error::McpToolList {
                service_name: conn.service_name.clone(),
                message: format!("{e}"),
            })
    }
}

/// Rig Tools
impl McpRegistry {
    /// 构建可注入 rig 的工具列表：远端 MCP 工具 + 内省工具
    ///
    /// 远端工具已按 [`ToolPolicy`] 过滤；末尾追加 `mcp_service_list` 与
    /// `mcp_service_tools` 两个内省工具（其快照同样只含已启用工具）。
    /// 单个服务拉取工具失败时记录警告并跳过，不影响其他服务。
    ///
    /// 返回的扁平 `Vec<DynamicTool>` 可直接交给
    /// [`crate::tool::ToolPool::register_many`] / `register_rig_tool_boxed`。
    pub async fn build_tools(&self) -> Vec<DynamicTool> {
        let mut tools: Vec<DynamicTool> = Vec::new();
        let mut services_info = Vec::with_capacity(self.connections.len());
        let mut service_tools_map = HashMap::with_capacity(self.connections.len());

        for conn in self.connections.values() {
            let remote_tools = match self.list_tools(conn).await {
                Ok(remote_tools) => conn.policy.apply(remote_tools),
                Err(e) => {
                    warn!(
                        service_name = %conn.service_name,
                        error = %e,
                        "Failed to list tools from MCP service, skipped"
                    );
                    continue;
                }
            };

            debug!(
                service_name = %conn.service_name,
                tools_count = remote_tools.len(),
                "MCP tools listed"
            );

            let tool_infos: Vec<McpToolInfo> = remote_tools
                .iter()
                .map(|tool| McpToolInfo {
                    name: tool.name.to_string(),
                    description: tool.description.as_deref().unwrap_or("").to_string(),
                })
                .collect();

            services_info.push(McpServiceInfo {
                name: conn.service_name.clone(),
                transport_type: conn.transport_type.clone(),
                tool_count: remote_tools.len(),
                description: conn.description.server_description.clone(),
            });
            service_tools_map.insert(conn.service_name.clone(), tool_infos);

            for tool in remote_tools {
                tools.push(McpToolExecTool::new(tool, conn.peer.clone()).into_dynamic_tool());
            }
        }

        // Introspection tools
        tools.push(dynamic_tool(McpServiceListTool::new(services_info)));
        tools.push(dynamic_tool(McpServiceToolsTool::new(service_tools_map)));

        tools
    }
}
