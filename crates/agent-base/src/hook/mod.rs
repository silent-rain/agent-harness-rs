//! 生命周期钩子模块
//!
//! 提供智能体运行时的生命周期钩子，支持在关键节点插入自定义逻辑。
//!
//! 三层钩子体系：
//! - **Agent 级**：Agent 运行前后的拦截
//! - **Model 级**：LLM 调用前后的拦截
//! - **Tool 级**：工具调用前后的拦截
//!
//! 提供的主要内容：
//! - [`AgentHook`] - 统一的生命周期钩子 trait（6 个方法均带默认实现）
//! - [`HookRegistry`] - 钩子注册表，管理所有钩子的注册和执行
//!
//! 数据结构（`Args` / `Result`）位于 [`crate::types::hook`]。

mod registry;
mod traits;

pub use registry::HookRegistry;
pub use traits::AgentHook;
