//! 命令加载器（装配层）
//!
//! 提供基于外部存储的命令加载实现。与 `agent_base::command::loader` 互为镜像，
//! 后者负责本地文件系统加载，本模块负责数据库等装配层数据源。
//!
//! 提供的主要内容：
//! - [`DbCommandLoader`]：从数据库装配命令列表

mod db;

pub use db::DbCommandLoader;
