//! A2A Client 端模块
//!
//! 提供的主要内容：
//! - [`a2a_agent`] - `A2AAgent`：实现 `Agent` trait，将远程 A2A 服务透明代理为本地 Agent
//! - [`tool`] - `A2aAgentTool`：将 `A2AAgent` 包装为动态工具，供 LLM 直接调用
//! - [`loader`] - A2A 配置加载器（`LocalA2aLoader`）

pub mod a2a_agent;

pub use a2a_agent::A2AAgent;
