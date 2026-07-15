//! 命令模块
//!
//! 提供本地命令加载器与命令相关工具。命令（Command）是一种面向开发场景的指令式交互能力，
//! 用于将常见、高频、结构化的开发任务封装为可复用的命令。
//!
//! 提供的主要内容：
//! - [`LocalCommandLoader`] - 本地文件系统命令加载器
//! - [`CommandListTool`] / [`CommandLoadTool`] - 内置工具，供 LLM 按需加载命令
//! - [`CommandManifest`] - 命令清单，从命令 `.md` 文件 frontmatter 解析（可选工具）
//!
//! ## 与 Skill 的区别
//!
//! - **Skill**：模型自主调用，注入 System Prompt，LLM 按需触发
//! - **Command**：用户 / 宿主显式触发，一条明确指令执行特定任务
//!
//! ## 设计说明
//!
//! 各类 Loader 负责从特定来源加载 [`CommandConfig`](crate::types::command::CommandConfig)，
//! 由调用方将结果通过
//! [`AgentBuilder::add_commands`](crate::runtime::AgentBuilder::add_commands) 直接注入
//! 到 [`AgentBundle`](crate::types::runtime::AgentBundle) 中，
//! 在 build 阶段由 builder 统一按 `name` 去重（先注入者位置优先 + 后写覆盖）。
//!
//! ```text
//!   ┌──────────────────────────┐
//!   │ LocalCommandLoader       │──load──┐
//!   ├──────────────────────────┤        │  add_commands
//!   │ DbCommandLoader (业务层) │──load──┼──────────────▶ AgentBuilder ──build──▶ Vec<CommandConfig>
//!   ├──────────────────────────┤        │
//!   │ HttpCommandLoader ...    │──load──┘
//!   └──────────────────────────┘
//! ```
//!
//! ## 目录结构约定（本地加载）
//!
//! ```text
//! <commands_root>/
//! ├── gen-doc.md         # name = "gen-doc"，content = 文件完整内容
//! ├── gen-test.md        # name = "gen-test"
//! └── refactor.md        # name = "refactor"
//! ```
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use agent_base::command::LocalCommandLoader;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // 本地加载
//! let local_commands = LocalCommandLoader::new(".codebuddy/commands").load().await?;
//!
//! // 通过 AgentBuilder::add_commands 注入到 bundle
//! // builder.add_commands(local_commands);
//! # Ok(())
//! # }
//! ```
mod loader;
mod manifest;
mod tools;

// NOTE: CommandManifest is currently unused — reserved for future frontmatter parsing needs.
pub use loader::LocalCommandLoader;
pub use manifest::CommandManifest;
pub use tools::{CommandListTool, CommandLoadTool};
