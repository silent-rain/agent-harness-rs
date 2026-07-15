//! Skill 安装通道
//!
//! 提供两种独立的安装能力：
//! - **本地模式**（[`LocalDirInstaller`]）：直接把 Skill 写入 `<skills_root>/<skill_code>/`
//! - **线上模式**（[`DbInstaller`]）：通过 [`SkillPackageStore`] 写入数据库
//!
//! 两种模式互不依赖，调用方按部署形态二选一。
//!
//! 提供的主要内容：
//! - [`LocalDirInstaller`] - 本地目录安装器
//! - [`DbInstaller`] - 数据库安装器（线上）
//! - [`SkillSource`] - 安装来源协议（本地路径 / inline zip）
//! - [`SkillPackage`] / [`SkillPackageMeta`] - 安装包数据契约
//! - [`SkillPackageStatus`] - 包状态枚举
//! - [`SkillPackageStore`] - 持久化抽象 trait（线上模式专用）
//! - [`InMemorySkillPackageStore`] - 内存版 Store（测试与离线场景）
//! - [`InstallRecord`] - store.install 操作的结果语义
//! - [`InstallOutcome`] - DbInstaller install/update 操作的结果语义
//! - [`SkillCheckResult`] - 注册与关联状态查询结果
//! - [`SkillPackageRecord`] / [`SkillRelRecord`] - DB 行级记录

pub mod db;
pub mod local_dir;
pub mod source;
pub mod store;
pub mod types;
pub mod zip_codec;

pub use db::DbInstaller;
pub use local_dir::LocalDirInstaller;
pub use source::SkillSource;
pub use store::{InMemorySkillPackageStore, InstallRecord, SkillPackageStore};
pub use types::{
    InstallOutcome, SkillCheckResult, SkillPackage, SkillPackageMeta, SkillPackageRecord,
    SkillPackageStatus, SkillRelRecord,
};
pub use zip_codec::{
    pack_dir_to_zip, read_skill_md_from_zip, sha256_hex, sha256_short, unpack_zip_to_dir,
    validate_skill_zip,
};
