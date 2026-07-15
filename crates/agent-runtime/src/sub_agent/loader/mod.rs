//! 子智能体加载器
//!
//! 提供从不同来源加载子智能体的加载器。每个加载器返回
//! [`crate::types::runtime::AgentBundle`] 列表，由调用方通过
//! [`AgentBuilder::add_sub_agents`](crate::runtime::AgentBuilder::add_sub_agents)
//! 注入到 bundle 中统一合并。
//!
//! 提供的主要内容：
//! - [`LocalSubAgentLoader`] - 从本地文件系统扫描子智能体目录（`.md` 文件）

mod local;

pub use local::LocalSubAgentLoader;
