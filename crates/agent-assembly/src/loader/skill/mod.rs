//! 技能加载器（装配层）
//!
//! 提供基于外部存储的技能加载实现。与 `agent_runtime::skill::loader` 互为镜像，
//! 后者负责本地文件系统加载，本模块负责数据库等装配层数据源。
//!
//! 提供的主要内容：
//! - [`DbSkillLoader`] - 从数据库装配启用的技能列表（用于运行时）
//! - [`DbSkillPackageStore`] - SeaORM 实现的 [`agent_runtime::skill::installer::SkillPackageStore`]，
//!   供 `DbInstaller` / `SkillManagementTool` 写入安装包

pub mod db;
pub mod db_package_store;

pub use db::DbSkillLoader;
pub use db_package_store::DbSkillPackageStore;
