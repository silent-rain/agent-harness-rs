//! 工具加载器（装配层）
//!
//! 提供基于外部存储的工具加载实现，与其它资源域（rule / command / skill / mcp）
//! 的 loader 结构保持一致：
//!
//! - 装配层（本模块）：依赖 SeaORM，从数据库读取注册表
//! - 核心运行时（`agent-runtime`）：零外部存储依赖
//!
//! 提供的主要内容：
//! - [`DbToolLoader`]：从数据库装配工具列表

pub mod db;

pub use db::DbToolLoader;
