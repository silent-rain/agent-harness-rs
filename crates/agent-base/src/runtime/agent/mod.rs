//! 智能体抽象与默认实现
//!
//! 提供的主要内容：
//! - [`Agent`] / [`AgentOpenAI`] - 顶层智能体 trait（[`mod@traits`]）
//! - [`LlmAgent`] - LLM 推理智能体（默认实现）
//! - [`ChainAgent`] - 链式串联智能体
//! - [`ParallelAgent`] - 并行智能体
//!
//! ## 抽象层次
//!
//! ```text
//!   trait Agent
//!        ├── LlmAgent       — 单 LLM 循环（ReAct）
//!        ├── ChainAgent     — 顺序串联多个 Agent
//!        ├── ParallelAgent  — 并发执行多个 Agent
//!        └── (未来) CycleAgent / RalphAgent / GraphAgent
//! ```

mod builder;
mod chain_agent;
mod llm_agent;
mod local_source;
mod parallel_agent;
mod traits;

pub use builder::AgentBuilder;
pub use chain_agent::ChainAgent;
pub use llm_agent::LlmAgent;
pub use local_source::LocalAgentSource;
pub use parallel_agent::ParallelAgent;
pub use traits::{Agent, AgentOpenAI, AgentSource, OpenAIChunkStream};
