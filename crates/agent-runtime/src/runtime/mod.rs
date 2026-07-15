//! 运行时模块
//!
//! 提供的主要内容：
//! - `agent`: 智能体抽象与默认实现（`Agent` trait + `LlmAgent` / `ChainAgent` / `ParallelAgent`）
//! - `services`: 运行时服务聚合（`RuntimeServices`，按领域分组）
//! - `llm`: `LlmAgent` 内部组件（`ModelCaller` / `ToolExecutor`）
//!
//! ## 架构总览
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │  trait Agent (顶层抽象)                                    │
//! │     ├── LlmAgent     — 单 LLM 循环（ReAct）                │
//! │     │      ├── ModelCaller         （单步推理 + 工具执行）   │
//! │     │      ├── ToolExecutor        （单工具调用 + 钩子）     │
//! │     │      └── HookRegistry        （生命周期钩子）          │
//! │     ├── ChainAgent   — 顺序串联                            │
//! │     ├── ParallelAgent— 并行分支                            │
//! │     └── (未来) CycleAgent / RalphAgent / GraphAgent        │
//! └──────────────────────────────────────────────────────────┘
//! ```

pub mod agent;
pub mod llm;
pub mod services;

pub use crate::session::{
    AgentEventStream, BoxStream, InMemorySessionService, RunOutcome, ScopedContexts,
    SessionService, SharedSessionService,
};
pub use agent::{
    Agent, AgentBuilder, AgentOpenAI, AgentSource, ChainAgent, LlmAgent, LocalAgentSource,
    OpenAIChunkStream, ParallelAgent,
};
pub use llm::types::ModelTurn;
pub use services::RuntimeServices;
