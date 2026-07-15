//! A2A 连接注册表
//!
//! 独立管理所有远程 A2A 服务的连接生命周期，与 [`crate::mcp::McpRegistry`] 对称。
//!
//! ## 架构
//!
//! ```text
//! A2aRegistry
//!   ├── connections: HashMap<service_name, A2aConnection>
//!   │     ├── agent  (已连接的 A2AAgent)
//!   │     └── config (原始服务配置)
//!   ├── pending_configs: Vec<A2aServiceConfig>  → 待连接的服务配置
//!   └── Methods:
//!     ├── add_config(config)      → 注册单个配置（不立即连接）
//!     ├── add_configs(configs)    → 批量注册配置
//!     ├── with_configs(configs)   → 替换式批量设置（Builder 风格）
//!     ├── connect_all()           → 连接所有 pending 配置
//!     ├── build_tools()           → 构建 A2A 工具列表（Vec<Box<dyn ContextualTool>>）
//!     ├── connection_count()      → 已连接服务数量
//!     └── pending_count()         → 待连接配置数量
//! ```
//!
//! ## 设计说明
//!
//! - 创建实例时仅注册配置，不执行 I/O 连接（延迟连接）
//! - 通过 `connect_all()` 统一建立连接，单个服务失败时记录警告并跳过
//! - 连接生命周期由 `A2aRegistry` 内部持有，对外透明
//! - `AgentBuilder::build()` 内部会自动使用本注册表完成 A2A 工具注入，
//!   一般情况下调用方无需直接操作 `A2aRegistry`
//! - 若需自定义构建流程，可通过手动构建后
//!   再通过 `AgentBuilder::with_a2a_registry` 注入

use std::collections::HashMap;

use tracing::{info, warn};

use crate::{
    a2a::{A2AAgent, tools::A2aAgentTool},
    error::AgentResult,
    types::a2a::A2aServiceConfig,
};

/// 单个 A2A 连接
pub struct A2aConnection {
    /// 已连接的远程 A2A Agent
    pub agent: A2AAgent,
    /// 原始服务配置（用于工具名称、描述等）
    pub config: A2aServiceConfig,
}

/// A2A 连接注册表
///
/// 独立管理所有远程 A2A 服务的连接生命周期，与 [`crate::mcp::McpRegistry`] 对称。
/// 采用「配置注册 → 延迟连接」两阶段模式：
///
/// 1. **配置阶段**（同步）：通过 `add_config` / `add_configs` 注册服务配置
/// 2. **连接阶段**（异步）：通过 `connect_all` 执行实际连接
///
/// `AgentBuilder::build()` 内部会自动使用本注册表完成 A2A 工具注入，
/// 一般情况下调用方无需直接操作 `A2aRegistry`。
///
/// 若需自定义 A2A 构建流程，可通过手动构建后
/// 再通过 `AgentBuilder::with_a2a_registry` 注入。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_runtime::a2a::A2aRegistry;
/// use agent_runtime::types::a2a::A2aServiceConfig;
///
/// # async fn example() -> agent_runtime::error::AgentResult<()> {
/// let config = A2aServiceConfig {
///     service_name: "weather_agent".to_string(),
///     description: "Query weather information".to_string(),
///     base_url: "http://weather-agent:3000".to_string(),
///     timeout_secs: Some(30),
///     headers: Default::default(),
/// };
///
/// // Two-phase: register configs → connect all
/// let mut registry = A2aRegistry::default();
/// registry.add_config(config);
/// registry.connect_all().await?;
///
/// // Build tools for runtime injection
/// let tools = registry.build_tools();
/// # Ok(())
/// # }
/// ```
#[derive(Default)]
pub struct A2aRegistry {
    /// Active A2A connections keyed by service name
    connections: HashMap<String, A2aConnection>,
    /// Pending service configs waiting to be connected
    pending_configs: Vec<A2aServiceConfig>,
}

impl A2aRegistry {
    /// 创建空的注册表
    pub fn new() -> Self {
        A2aRegistry::default()
    }

    /// 批量设置 A2A 服务配置（替换式）
    ///
    /// 将 pending 队列中的旧配置**全部替换**为提供的新配置列表。
    /// 适用于构建阶段一次性设置服务配置，支持链式调用。
    ///
    /// 与 `add_configs` 不同，本方法会清空已有的 pending 配置后再写入。
    ///
    /// # Parameters
    ///
    /// * `configs` - A2A 服务配置列表
    pub fn with_configs(mut self, configs: Vec<A2aServiceConfig>) -> Self {
        for c in &configs {
            info!(
                service_name = %c.service_name,
                base_url = %c.base_url,
                "A2A service config set (builder, replaces previous pending)"
            );
        }
        self.pending_configs = configs;
        self
    }

    /// 注册单个 A2A 服务配置（不立即连接）
    ///
    /// 配置会被暂存到 pending 队列中，等待 `connect_all()` 时统一建立连接。
    ///
    /// # Parameters
    ///
    /// * `config` - A2A 服务配置
    pub fn add_config(&mut self, config: A2aServiceConfig) {
        info!(
            service_name = %config.service_name,
            base_url = %config.base_url,
            "A2A service config registered (pending connection)"
        );
        self.pending_configs.push(config);
    }

    /// 批量注册 A2A 服务配置（不立即连接）
    ///
    /// 配置会被暂存到 pending 队列中，等待 `connect_all()` 时统一建立连接。
    ///
    /// # Parameters
    ///
    /// * `configs` - A2A 服务配置列表
    pub fn add_configs(&mut self, configs: Vec<A2aServiceConfig>) {
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

        for config in configs {
            match A2AAgent::connect(&config.base_url).await {
                Ok(agent) => {
                    info!(
                        service_name = %config.service_name,
                        base_url = %config.base_url,
                        "A2A service connected"
                    );
                    self.connections
                        .insert(config.service_name.clone(), A2aConnection { agent, config });
                }
                Err(e) => {
                    warn!(
                        service_name = %config.service_name,
                        base_url = %config.base_url,
                        error = %e,
                        "A2A service connection failed, skipped"
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

    /// 构建 A2A 工具列表
    ///
    /// 将所有已连接的 A2A Agent 包装为 [`A2aAgentTool`]，
    /// 返回 `Vec<A2aAgentTool>` 供 [`crate::tool::ToolPool`] 通过
    /// `register_contextual` 注册。
    ///
    /// # Returns
    ///
    /// 每个已连接服务对应一个 `A2aAgentTool`
    pub fn build_tools(&self) -> Vec<A2aAgentTool> {
        self.connections
            .values()
            .map(|conn| A2aAgentTool::new(conn.agent.clone(), &conn.config))
            .collect()
    }
}
