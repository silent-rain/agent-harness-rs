//! 工具模块
//!
//! 提供共享工具池与内置工具实现，工具契约**完全采用 rig-core 原生形态**：
//!
//! - 工具实现 rig 的 [`Tool`](rig_core::tool::Tool) 或直接构造
//!   [`DynamicTool`](rig_core::tool::DynamicTool)；
//! - 运行期统一持有 `DynamicTool`（对象安全形态），一次调用由
//!   [`ToolPool::call`] 完成派发；
//! - 上下文用 rig 的 [`ToolContext`](rig_core::tool::ToolContext)：
//!   运行身份走 inbound（[`AgentRun`] / [`StepIndex`]），父运行事件通道走
//!   scope（[`AgentRunScope`]）。
//!
//! 本仓库不再提供第二套工具抽象与上下文类型，二者之间的适配全部集中在
//! [`rig_bridge`]。
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
//! - 派发（[`ToolPool::call`]）与 LLM 工具定义（[`ToolPool::tool_definitions`]）
//!   都直接由池提供，不再派生第二份注册表；
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
//! // 1. 逐个注册
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
mod pool;
pub mod rig_bridge;
pub mod simple_tools;
mod tools;

pub use pool::{ToolCallError, ToolPool};
pub use rig_bridge::{
    AgentRun, AgentRunScope, StepIndex, ToolContextExt, call_context, dynamic_tool,
    dynamic_tool_named, tool_name_of, tool_output_to_string,
};
pub use tools::*;
