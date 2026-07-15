//! A2A 配置加载器
//!
//! 提供从不同来源加载 A2A 服务配置的加载器。
//!
//! 提供的主要内容：
//! - [`LocalA2aLoader`] - 从本地 `a2a.json` 文件加载 A2A 服务配置

mod local;

pub use local::LocalA2aLoader;
