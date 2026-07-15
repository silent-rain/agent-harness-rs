//! 技能加载器
//!
//! 提供从不同来源加载技能的加载器。每个加载器返回 [`crate::types::skill::SkillConfig`] 列表，
//! 由调用方通过
//! [`AgentBuilder::add_skills`](crate::runtime::AgentBuilder::add_skills) 注入到 bundle 中统一合并。
//!
//! 提供的主要内容：
//! - [`LocalSkillLoader`] - 从本地文件系统扫描技能目录（含 DB zip 惰性解压）

mod local;

pub use local::LocalSkillLoader;
