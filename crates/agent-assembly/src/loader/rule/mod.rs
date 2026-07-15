//! 规则加载器（装配层）
//!
//! 提供基于外部存储的规则加载实现。与 `agent_base::rule::loader` 互为镜像，
//! 后者负责本地文件系统加载，本模块负责数据库等装配层数据源。
//!
//! 提供的主要内容：
//! - [`DbRuleLoader`]：从数据库装配规则列表

pub mod db;

pub use db::DbRuleLoader;
