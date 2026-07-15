//! 会话与事件模块
//!
//! 聚合会话管理与流式事件协议，统一管理会话的生命周期、事件流和状态。
//!
//! 提供的主要内容：
//! - [`event`][]: 流式事件模型（`AgentStreamEvent`、`SessionEventRecord` 等）
//! - [`in_memory`][]: 内存会话服务实现（`InMemorySessionService`、`Session`）
//! - [`traits`][]: 会话服务抽象（`SessionService`、`SharedSessionService`）
//! - [`agent_ref`][]: 智能体/会话上下文值对象（`SessionRef`）
//! - [`context`][]: 三层上下文寻址键（`ContextScope`）
//! - [`outcome`][]: 运行结果与流类型（`RunOutcome`、`AgentEventStream`、`BoxStream`）

pub mod event;
pub mod in_memory;
pub mod outcome;
pub mod scoped_contexts;
pub mod traits;

// Event types
pub use event::{AgentEventStream, AgentStreamEvent, BoxStream, FinishReason, PlannedToolCall};

// Outcome types
pub use outcome::RunOutcome;

// Scoped context types
pub use scoped_contexts::ScopedContexts;

// Service types
pub use in_memory::InMemorySessionService;
pub use traits::{SessionService, SharedSessionService};
