//! [`LlmAgent`](crate::runtime::LlmAgent) 内部组件模块
//!
//! 将原 `default_runner.rs` 中近 1700 行代码按职责拆分为以下组件，
//! 每个组件保持单一职责，可独立测试与替换：
//!
//! | 组件 | 职责 |
//! |------|------|
//! | [`ModelCaller`] | LLM 流式调用 + 聚合 + before/after_model 钩子 + 工具执行 + 终止判定 |
//! | [`ToolExecutor`] | 单工具调用 + 重试 + 事件 + 钩子 + 审计 + 记忆 |
//!
//! 注：原 `RunJournal` 已合并至 [`RuntimeServices`](crate::runtime::RuntimeServices)
//! 的便捷方法（`try_append_event` / `try_append_context` 等）；
//! 原 `DecisionDispatcher` / `HookPipeline.call_model` 已并入 [`ModelCaller`]。

mod json_repair;
mod model_caller;
mod tool_executor;
pub mod types;

pub use json_repair::repair_json_arguments;
pub(crate) use model_caller::ModelCaller;
pub(crate) use tool_executor::ToolExecutor;
