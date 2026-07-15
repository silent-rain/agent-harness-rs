//! 智能体运行时基座库
//!
//! `agent-runtime` 是智能体平台的核心运行时基座，以 `rig-core` 为核心提供：
//!
//! - [`types`] - 核心领域模型（`AgentDefinitionSnapshot` 等）
//! - [`hook`] - 智能体生命周期钩子（`AgentHook`、`HookRegistry`）
//! - [`event`] - 统一流式事件协议（`AgentStreamEvent`）
//! - [`model`] - LLM 模型适配层（`ModelAdapter`、`StreamChunk`、`MockModel`）
//! - [`message`] - 对话消息中立载体（`ConversationMessage`、`MessageRole`）
//! - [`context`] - 对话消息上下文（`MessageContext`）
//! - [`prompt`] - 系统提示词构建（`SystemPrompt`）
//! - [`knowledge`] - 知识检索抽象（`AgentKnowledgeRetriever`）
//! - [`runtime`] - 核心运行循环与事件发布器（`Agent`、`AgentBuilder`、`RuntimeServices`）
//! - [`error`] - 统一错误类型（`AgentError`）
//! - [`skill`] - 本地目录扫描的技能加载器与技能工具（`LocalSkillLoader`、`SkillManifest`）
//! - [`tool`] - 工具池与内置工具实现（`ToolPool`、`rig::Tool` trait）
//! - [`mcp`] - MCP 连接注册表（`McpRegistry`，`feature = "mcp"` 时启用）
//! - [`openai`] - OpenAI Chat Completions 协议产物与流式适配器
//! - [`a2a`] - A2A（Agent-to-Agent）协议支持（`feature = "a2a"` 时启用）
//!   - `A2aServer`：将本地 Agent 一键暴露为 A2A v1.0 服务
//!   - `A2AAgent`：将远程 A2A 服务透明代理为本地 Agent（实现 `Agent` trait）
//! - [`prelude`] - SDK 常用类型的一键导入（`use agent_runtime::prelude::*;`）
//!
//! ## 设计原则
//!
//! - `agent-runtime` 以 `rig-core` 为核心，工具系统直接使用 `rig::ToolSet`
//! - 用户只需传入 rig `CompletionModel` 实例，无需感知内部推理逻辑
//! - 所有可执行动作统一通过 `function calling` 或受 `JSON Schema` 约束的结构化协议触发
//! - 禁止依赖解析自由文本来驱动工具、RAG、记忆或子智能体调用
//! - **动作层结构化，展示层自然语言**
#[cfg(feature = "a2a")]
pub mod a2a;
#[cfg(feature = "command")]
pub mod command;
#[cfg(feature = "mcp")]
pub mod mcp;
#[cfg(feature = "rule")]
pub mod rule;
#[cfg(feature = "skill")]
pub mod skill;
#[cfg(feature = "sub_agent")]
pub mod sub_agent;

pub mod constant;
pub mod context;
pub mod error;
pub mod hook;
pub mod knowledge;
pub mod message;
pub mod model;
pub mod openai;
pub mod prelude;
pub mod prompt;
pub mod runtime;
pub mod session;
pub mod tool;
pub mod types;
pub mod utils;

// 常用类型重导出
pub use context::MessageContext;
pub use error::{AgentResult, Error};
pub use hook::{AgentHook, HookRegistry};
pub use knowledge::{AgentKnowledgeRetriever, KnowledgeQueryResult, NoopKnowledgeRetriever};
pub use message::{ConversationMessage, DistilledMemory, MessageRole};
pub use model::ModelAdapter;
pub use prompt::SystemPrompt;
pub use runtime::{
    Agent, AgentBuilder, AgentOpenAI, ChainAgent, LlmAgent, ModelTurn, OpenAIChunkStream,
    ParallelAgent, RuntimeServices,
};
pub use session::{AgentStreamEvent, InMemorySessionService, SessionService, SharedSessionService};
