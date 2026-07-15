//! A2A Server 端模块
//!
//! 提供的主要内容：
//! - [`executor`] - `AgentExecAdapter`：将 `Arc<dyn Agent>` 适配为 A2A `MessageHandler`
//! - [`serve`] - `A2aServer`：一行启动器，将 Agent 暴露为 A2A 服务

pub mod executor;
pub mod serve;
