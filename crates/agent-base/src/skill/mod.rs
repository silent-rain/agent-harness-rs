//! 技能模块
//!
//! 提供本地技能加载器和技能安装能力。
//! 技能（Skill）是一系列指令、脚本和资源的集合，以本地目录包形式存在，
//! 运行时按需加载注入到 System Prompt 中。
//!
//! 提供的主要内容：
//! - [`LocalSkillLoader`] - 本地文件系统技能加载器（含 DB zip 惰性解压）
//! - [`SkillManifest`] - 技能清单，从 `SKILL.md` frontmatter 解析
//!
//! ## 设计说明
//!
//! 各类 Loader 负责从特定来源加载 [`SkillConfig`](crate::types::skill::SkillConfig)，
//! 由调用方将结果通过
//! [`AgentBuilder::add_skills`](crate::runtime::AgentBuilder::add_skills) 直接注入
//! 到 [`AgentBundle`](crate::types::runtime::AgentBundle) 中，
//! 在 build 阶段由 builder 统一按 `skill_code` 去重（先注入者优先，本地优先于 DB）。
//!
//! ```text
//!   ┌──────────────────────────┐
//!   │ LocalSkillLoader         │──load──┐
//!   ├──────────────────────────┤        │  add_skills
//!   │ DbSkillLoader (业务层)   │──load──┼──────────────▶ AgentBuilder ──build──▶ Vec<SkillConfig>
//!   ├──────────────────────────┤        │
//!   │ SessionSkillLoader ...   │──load──┘
//!   └──────────────────────────┘
//! ```
//!
//! ## 目录结构约定
//!
//! ```text
//! <skills_root>/
//! ├── service-hub-generator/     # skill_code = 目录名
//! │   ├── SKILL.md               # 入口清单（含 frontmatter）
//! │   ├── references/            # 参考资料
//! │   │   └── *.md
//! │   └── assets/                # 资源文件
//! │       └── *
//! └── migration-entity-generator/
//!     ├── SKILL.md
//!     ├── references/
//!     └── assets/
//! ```
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use agent_base::skill::LocalSkillLoader;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // 本地加载
//! let local_skills = LocalSkillLoader::new(".codebuddy/skills").load().await?;
//!
//! // 通过 AgentBuilder::add_skills 注入到 bundle
//! // builder.add_skills(local_skills);
//! # Ok(())
//! # }
//! ```
mod loader;
mod manifest;
mod tools;

pub mod installer;

pub use installer::{
    DbInstaller, InMemorySkillPackageStore, InstallOutcome, InstallRecord, LocalDirInstaller,
    SkillCheckResult, SkillPackage, SkillPackageMeta, SkillPackageRecord, SkillPackageStatus,
    SkillPackageStore, SkillRelRecord, SkillSource,
};
pub use loader::LocalSkillLoader;
pub use manifest::SkillManifest;
pub use tools::{
    SkillBackend, SkillExecTool, SkillListDocsTool, SkillLoadTool, SkillManageArgs,
    SkillManageOutput, SkillManagementTool, SkillSelectDocsTool,
};
