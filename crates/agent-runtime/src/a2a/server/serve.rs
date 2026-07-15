//! A2aServer：将本地 Agent 一键暴露为 A2A v1.0 服务
//!
//! 提供三种使用形态：
//! - **形态 ①**：`A2aServer::new(agent, bundle).serve(addr)` — 最常见，一行启动
//! - **形态 ②**：`A2aServer::with_card(agent, card).serve(addr)` — 自定义 AgentCard
//! - **形态 ③**：`MultiA2aServer::new(addr).mount(...).serve()` — 多 Agent 单端口

use std::sync::Arc;

use a2a_rs_core::AgentCard;
use a2a_rs_server::A2aServer as InnerA2aServer;
use tracing::info;

use crate::{
    Error,
    a2a::server::executor::{AgentExecAdapter, EventSenderSlot},
    error::AgentResult,
    runtime::Agent,
    types::runtime::AgentBundle,
};

/// A2A 服务启动器
///
/// 将本地 `Arc<dyn Agent>` 暴露为符合 A2A v1.0 规范的 HTTP 服务。
///
/// 启动后自动提供：
/// - `GET  /.well-known/agent-card.json` — AgentCard 服务发现
/// - `POST /v1/rpc` — JSON-RPC（`message/send`、`message/stream`）
/// - `POST /v1/message:send` — REST 非流式
/// - `POST /v1/message:stream` — REST 流式（SSE）
/// - `GET  /health` — 健康检查
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use agent_runtime::a2a::A2aServer;
/// use agent_runtime::types::runtime::AgentBundle;
///
/// # async fn example(agent: Arc<dyn agent_runtime::Agent>, bundle: AgentBundle) -> anyhow::Result<()> {
/// // 形态 ①：一行启动（最常见）
/// A2aServer::new(agent, bundle)
///     .serve("0.0.0.0:3000")
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct A2aServer {
    adapter: AgentExecAdapter,
    /// 事件发送器槽的 Arc 克隆（在 adapter move 之前保存，用于注入 broadcast sender）
    event_slot: EventSenderSlot,
    bind_addr: Option<String>,
}

impl A2aServer {
    /// 从 Agent + AgentBundle 创建服务（AgentCard 自动生成）
    ///
    /// # Parameters
    ///
    /// * `agent` - 本地 Agent 实例
    /// * `bundle` - 智能体装配包（用于自动生成 AgentCard）
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::sync::Arc;
    /// use agent_runtime::a2a::A2aServer;
    /// use agent_runtime::types::runtime::AgentBundle;
    ///
    /// # async fn example(agent: Arc<dyn agent_runtime::Agent>, bundle: AgentBundle) -> anyhow::Result<()> {
    /// A2aServer::new(agent, bundle).serve("0.0.0.0:3000").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(agent: Arc<dyn Agent>, bundle: AgentBundle) -> Self {
        let adapter = AgentExecAdapter::new(agent, &bundle, "");
        let event_slot = adapter.event_sender_slot();
        Self {
            adapter,
            event_slot,
            bind_addr: None,
        }
    }

    /// 从 Agent + 自定义 AgentCard 创建服务
    ///
    /// 适用于需要精确控制 AgentCard 内容的场景（如设置 provider、icon、额外 skill）。
    ///
    /// # Parameters
    ///
    /// * `agent` - 本地 Agent 实例
    /// * `card` - 自定义 AgentCard
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::sync::Arc;
    /// use agent_runtime::a2a::{A2aServer, card::AgentCardBuilder};
    /// use agent_runtime::types::runtime::AgentBundle;
    ///
    /// # async fn example(agent: Arc<dyn agent_runtime::Agent>, bundle: AgentBundle) -> anyhow::Result<()> {
    /// let card = AgentCardBuilder::from_bundle(&bundle, "http://my-agent.example.com")
    ///     .with_provider("MyTeam", "https://myteam.example.com")
    ///     .build();
    ///
    /// A2aServer::with_card(agent, card).serve("0.0.0.0:3000").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_card(agent: Arc<dyn Agent>, card: AgentCard) -> Self {
        let adapter = AgentExecAdapter::with_card(agent, card);
        let event_slot = adapter.event_sender_slot();
        Self {
            adapter,
            event_slot,
            bind_addr: None,
        }
    }

    /// 绑定监听地址（链式调用，可选）
    ///
    /// # Parameters
    ///
    /// * `addr` - 监听地址（如 `"0.0.0.0:3000"`）
    pub fn bind(mut self, addr: impl Into<String>) -> Self {
        self.bind_addr = Some(addr.into());
        self
    }

    /// 启动 A2A 服务（阻塞直到服务停止）
    ///
    /// 内部流程：
    /// 1. 构造 `a2a-rs-server::A2aServer`（adapter 被 move 进去）
    /// 2. 通过预先保存的 `event_slot` Arc 将 broadcast sender 注入 adapter
    /// 3. 绑定地址并运行
    ///
    /// # Parameters
    ///
    /// * `addr` - 监听地址（如 `"0.0.0.0:3000"`）
    ///
    /// # Returns
    ///
    /// 服务正常退出时返回 `Ok(())`
    pub async fn serve(self, addr: impl Into<String>) -> AgentResult<()> {
        let addr = self.bind_addr.unwrap_or_else(|| addr.into());
        info!("A2A Server starting on {addr}");

        // 在 adapter move 之前保存 event_slot（已在 new/with_card 中保存）
        let event_slot = self.event_slot;

        let inner = InnerA2aServer::new(self.adapter)
            .bind(&addr)
            .map_err(|e| Error::Internal(format!("A2A bind failed: {e}")))?;

        // 从 inner server 获取 broadcast sender，注入到 adapter 的 OnceLock 槽
        // adapter 虽然已 move 进 inner，但 event_slot 是 Arc 共享引用，仍然有效
        let tx = inner.get_event_sender();
        event_slot
            .set(tx)
            .unwrap_or_else(|_| tracing::warn!("A2A event sender already initialized"));

        inner
            .run()
            .await
            .map_err(|e| Error::Internal(format!("A2A server error: {e}")))
    }
}

/// 多 Agent 单端口服务构造器
///
/// 允许在同一端口上挂载多个 Agent，每个 Agent 通过不同的 base path 路由。
///
/// > **注意**：当前版本（P0 MVP）仅使用第一个挂载的 Agent。
/// > 完整多路由支持将在 P3 阶段通过 axum Router 组合实现。
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use agent_runtime::a2a::server::serve::MultiA2aServer;
/// use agent_runtime::types::runtime::AgentBundle;
///
/// # async fn example(
/// #     math_agent: Arc<dyn agent_runtime::Agent>, math_bundle: AgentBundle,
/// #     weather_agent: Arc<dyn agent_runtime::Agent>, weather_bundle: AgentBundle,
/// # ) -> anyhow::Result<()> {
/// MultiA2aServer::new("0.0.0.0:3000")
///     .mount("/agents/math", math_agent, math_bundle)
///     .mount("/agents/weather", weather_agent, weather_bundle)
///     .serve()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct MultiA2aServer {
    bind_addr: String,
    agents: Vec<(String, AgentExecAdapter, EventSenderSlot)>,
}

impl MultiA2aServer {
    /// 创建多 Agent 服务构造器
    ///
    /// # Parameters
    ///
    /// * `bind_addr` - 监听地址（如 `"0.0.0.0:3000"`）
    pub fn new(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            agents: Vec::new(),
        }
    }

    /// 挂载一个 Agent 到指定路径
    ///
    /// # Parameters
    ///
    /// * `base_path` - 路由前缀（如 `"/agents/math"`）
    /// * `agent` - 本地 Agent 实例
    /// * `bundle` - 智能体装配包
    pub fn mount(
        mut self,
        base_path: impl Into<String>,
        agent: Arc<dyn Agent>,
        bundle: AgentBundle,
    ) -> Self {
        let base_path = base_path.into();
        let adapter = AgentExecAdapter::new(agent, &bundle, &base_path);
        let slot = adapter.event_sender_slot();
        self.agents.push((base_path, adapter, slot));
        self
    }

    /// 启动多 Agent 服务（阻塞直到服务停止）
    ///
    /// # Returns
    ///
    /// 服务正常退出时返回 `Ok(())`
    pub async fn serve(self) -> AgentResult<()> {
        if self.agents.is_empty() {
            return Err(Error::Internal(
                "MultiA2aServer: no agents mounted".to_string(),
            ));
        }

        // P0 MVP：仅使用第一个 Agent
        let (_, adapter, event_slot) = self.agents.into_iter().next().unwrap();

        let inner = InnerA2aServer::new(adapter)
            .bind(&self.bind_addr)
            .map_err(|e| Error::Internal(format!("A2A bind failed: {e}")))?;

        let tx = inner.get_event_sender();
        event_slot
            .set(tx)
            .unwrap_or_else(|_| tracing::warn!("A2A event sender already initialized"));

        inner
            .run()
            .await
            .map_err(|e| Error::Internal(format!("A2A server error: {e}")))
    }
}
