//! 命令加载器
//!
//! 提供从不同来源加载命令的加载器。每个加载器返回 [`crate::types::command::CommandConfig`] 列表，
//! 由调用方通过
//! [`AgentBuilder::add_commands`](crate::runtime::AgentBuilder::add_commands) 注入到 bundle 中统一合并。
//!
//! 提供的主要内容：
//! - [`LocalCommandLoader`] - 从本地文件系统扫描命令文件

mod local;

pub use local::LocalCommandLoader;
