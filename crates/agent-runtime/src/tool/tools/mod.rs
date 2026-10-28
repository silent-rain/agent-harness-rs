//! 工具模块
//!
//! 提供共享工具池与内置工具实现。
//!
//! 工具契约完全采用 rig-core 原生形态：类型化工具实现
//! [`rig_core::tool::Tool`]，运行期统一以
//! [`DynamicTool`](rig_core::tool::DynamicTool) 持有，由
//! [`ToolPool::call`](crate::tool::ToolPool::call) 派发。
//!
//! 需要运行时上下文的工具（子 Agent 委派 / A2A / skill 管理）从
//! [`ToolContext`](rig_core::tool::ToolContext) 读取运行身份与事件通道，
//! 与无状态工具共用同一条派发路径。
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
//! - 内部持有 `Arc<DynamicTool>`，可通过 `Arc<ToolPool>` 在多个 Agent 间共享，
//!   避免重复构造工具实例；
//! - 派发（`ToolPool::call`）与 LLM 工具定义（`ToolPool::tool_definitions`）
//!   均由池直接提供，不派生第二份注册表；
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
//! // 1. 直接装填内置工具
//! let mut pool = ToolPool::new();
//! pool.register_builtin(ReadFileTool).register_builtin(ListDirTool);
//!
//! // 2. 一次性注入内置工具
//! let mut pool = ToolPool::new();
//! pool.register_many(all_builtin_tools(), ToolSource::Builtin);
//!
//! // 3. 多 Agent 共享同一份池
//! let pool = Arc::new(pool);
//! let definitions = pool.tool_definitions();
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
