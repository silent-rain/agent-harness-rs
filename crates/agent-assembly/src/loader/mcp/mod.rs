//! MCP 服务加载器（装配层）
//!
//! 提供基于外部存储的 MCP 服务加载实现。与 `agent_base::mcp::loader` 互为镜像，
//! 后者负责本地文件系统加载，本模块负责数据库等装配层数据源。
//!
//! 提供的主要内容：
//! - [`DbMcpLoader`]：从数据库装配 MCP 服务配置列表

mod db;

pub use db::DbMcpLoader;
