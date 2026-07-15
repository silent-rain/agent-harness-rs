//! 工具模块
//!
//! 提供共享工具池与内置工具实现。
//!
//! 工具调用由 [`ContextualToolSet`] 统一派发：
//! - 实现 [`rig::Tool`] 的无状态工具会通过 [`StatelessAsContextual`] 适配为
//!   [`ContextualToolDyn`]，与有状态工具共用同一份注册表；
//! - 需要运行时上下文的工具（如子 Agent 委派）直接实现结构化的
//!   [`ContextualTool`] trait（携带 `Args` / `Output` / `Error` 关联类型），
//!   由 blanket impl 自动转换为 [`ContextualToolDyn`]。
//!
//! 提供的工具：
//! - [`ReadFileTool`] - 读取文件内容
//! - [`SearchFilesTool`] - 在目录中搜索文件名
//! - [`ListDirTool`] - 列出目录内容
//! - [`RunCommandTool`] - 受控终端命令执行
//! - [`KnowledgeQueryTool`] - 查询知识库
//!
//! ## 设计说明
//!
//! [`ToolPool`] 是工具子系统的唯一容器：
//! - 内部以 `Arc<dyn ContextualToolDyn>` 持有候选工具，可通过 `Arc<ToolPool>`
//!   在多个 Agent 间共享，避免重复构造工具实例；
//! - [`ToolPool::into_contextual_toolset`] 派生 [`ContextualToolSet`]
//!   供 `ToolExecutor` 调用，派生过程仅克隆 `Arc`，不复制底层实例；
//! - 池可变（`register_*` 接受 `&mut self`），但通过 `Arc<ToolPool>`
//!   共享后，所有 Agent 看到的就是同一份只读快照。
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use agent_runtime::tool::{ToolPool, ReadFileTool, ListDirTool};
//! use agent_runtime::tool::simple_tools::all_builtin_tools;
//! use agent_runtime::types::tool::ToolSource;
//!
//! // 1. 直接装填后派生 ContextualToolSet
//! let mut pool = ToolPool::new();
//! pool.register_builtin(ReadFileTool).register_builtin(ListDirTool);
//! let toolset = pool.into_contextual_toolset();
//!
//! // 2. 一次性注入内置工具
//! let mut pool = ToolPool::new();
//! pool.register_many(all_builtin_tools(), ToolSource::Builtin);
//!
//! // 3. 多 Agent 共享同一份池
//! let pool = Arc::new(pool);
//! let toolset = pool.into_contextual_toolset();
//! ```

// 通用工具
mod duckduckgo;
mod knowledge_query;
mod list_dir;
mod read_file;
mod run_command;
mod search_files;

pub use duckduckgo::DuckDuckGoTool;
pub use knowledge_query::KnowledgeQueryTool;
pub use list_dir::ListDirTool;
pub use read_file::ReadFileTool;
pub use run_command::RunCommandTool;
pub use search_files::SearchFilesTool;
