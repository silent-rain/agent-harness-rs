//! 子智能体加载器（装配层）
//!
//! 提供基于外部存储的子智能体加载实现，与其它资源域
//! （rule / command / skill / mcp / tool）的 loader 结构保持一致。
//!
//! 提供的主要内容：
//! - [`DbSubAgentLoader`]：从数据库装配子智能体完整快照

pub mod db;

pub use db::DbSubAgentLoader;
