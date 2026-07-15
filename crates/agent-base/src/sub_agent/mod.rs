//! 子智能体模块
//!
//! 提供本地子智能体加载器与委派工具。子智能体最终被包装为工具（Tool），
//! 注入到父智能体的 ToolSet 中，由 LLM 通过 function calling 触发委派。
//!
//! 提供的主要内容：
//! - [`LocalSubAgentLoader`] - 本地文件系统子智能体加载器
//! - [`SubAgentManifest`] - 子智能体清单，从 `.md` 文件 frontmatter 解析
//! - [`DelegateSubAgentTool`] / [`FlatSubAgentTool`] - 委派工具
//!
//! ## 设计说明
//!
//! Loader 负责从特定来源加载 [`AgentBundle`](crate::types::runtime::AgentBundle)，
//! 由调用方将结果通过
//! [`AgentBuilder::add_sub_agents`](crate::runtime::AgentBuilder::add_sub_agents)
//! 直接注入到 bundle 中，在 build 阶段由 builder 统一按 `name` 去重（先注入者优先），
//! 并根据父级 `max_delegation_depth` 递归截断子智能体树深度。
//!
//! ## 目录结构约定（本地加载）
//!
//! ```text
//! .codebuddy/agents/
//! ├── code-reviewer.md    # name = "code-reviewer"
//! ├── doc-writer.md       # name = "doc-writer"
//! └── 测试3.md            # name = "测试3"
//! ```
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use agent_base::sub_agent::LocalSubAgentLoader;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Local scan via loader (returns Vec<AgentBundle> directly)
//! let local_bundles = LocalSubAgentLoader::new(".codebuddy/agents").load().await?;
//!
//! // 通过 AgentBuilder::add_sub_agents 注入到 bundle
//! // builder.add_sub_agents(local_bundles);
//! # Ok(())
//! # }
//! ```

pub mod loader;
mod manifest;
mod tools;

pub use loader::LocalSubAgentLoader;
pub use manifest::SubAgentManifest;
pub use tools::DelegateSubAgentTool;
pub use tools::FlatSubAgentTool;
