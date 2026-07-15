//! 智能体平台装配层
//!
//! 负责从数据库装配 `AgentBundle`，
//! 并构建可运行的 `Agent`。
//!
//! # 架构说明
//!
//! ```text
//! agent-service
//!   └── agent-assembly          ← 本 crate
//!         ├── agent             ← 数据库驱动的 AgentSource
//!         └── agent-base        ← 运行时基座（不含 DB 依赖）
//! ```
//!
//! # 快速开始
//!
//! ```rust,no_run
//! use agent_assembly::DbAgentSource;
//! use agent_base::runtime::AgentSource;
//!
//! // 1. 构建数据库源
//! // let source = DbAgentSource::new(db, agent_id, user_id);
//!
//! // 2. 一次性加载完整装配包
//! // let bundle = source.load().await?;
//!
//! // 3. 运行
//! // let answer = runtime.run("session-1", "你好", 12345).await?;
//! ```

pub mod agent;
pub mod enums;
pub mod error;
pub mod loader;
pub mod session;

pub use agent::{AgentSummary, DbAgentSource};
pub use error::{Error, PlatformResult};
pub use session::{
    DbSessionService, NewSession, NewSessionEvent, SessionDao, SessionEventDao, SessionStateDao,
    SessionStateEntry, SessionUpdate,
};

// 重导出 agent-base 的核心类型，方便上层直接使用
pub use agent_base::{
    Agent, ModelAdapter, RuntimeServices,
    types::runtime::{AgentBundle, AgentConfig},
};
