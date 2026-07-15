//! 数据库驱动的智能体配置来源
//!
//! 提供的主要内容：
//! - [`DbAgentSource`]：实现 [`agent_base::runtime::AgentSource`]，
//!   从数据库一次性装配完整的智能体配置 [`AgentBundle`](agent_base::types::runtime::AgentBundle)
//! - [`AgentSummary`]：装配产物的能力摘要

pub mod source;

pub use source::{AgentSummary, DbAgentSource};
