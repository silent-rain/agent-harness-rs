//! A2A（Agent-to-Agent）协议支持
//!
//! 提供将本地 Agent 暴露为 A2A 服务（Server 端）以及将远程 A2A 服务
//! 透明代理为本地 Agent（Client 端）的完整双向能力。
//!
//! 提供的主要内容：
//! - [`card`] - AgentBundle → A2A AgentCard 的自动生成
//! - [`server`] - A2A Server 端（`A2aServer`、`AgentExecAdapter`）
//! - [`client`] - A2A Client 端（`A2AAgent`，实现 `Agent` trait）
//!
//! ## 快速开始
//!
//! ### 暴露本地 Agent 为 A2A 服务
//!
//! ```rust,no_run
//! use agent_runtime::a2a::A2aServer;
//!
//! # async fn example(agent: std::sync::Arc<dyn agent_runtime::Agent>, bundle: agent_runtime::types::runtime::AgentBundle) -> anyhow::Result<()> {
//! A2aServer::new(agent, bundle)
//!     .serve("0.0.0.0:3000")
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### 调用远程 A2A Agent
//!
//! ```rust,no_run
//! use agent_runtime::a2a::A2AAgent;
//! use agent_runtime::Agent;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let remote = A2AAgent::connect("http://remote-agent:3000").await?;
//! let outcome = remote.run("session-1", "你好").await?;
//! println!("{}", outcome.final_answer);
//! # Ok(())
//! # }
//! ```

pub mod card;
pub mod client;
pub mod loader;
pub mod registry;
pub mod server;
mod tools;

// Server 端公共类型
pub use server::serve::A2aServer;
// Client 端公共类型
pub use client::A2AAgent;
// Registry
pub use registry::A2aRegistry;

pub use loader::LocalA2aLoader;
pub use tools::A2aAgentTool;
