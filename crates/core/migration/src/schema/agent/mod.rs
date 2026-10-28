//! Agent 域数据库建表（DDL）
//!
//! 包含智能体基座所有核心表的建表迁移，所有表统一使用 `t_agent_` 前缀。

pub mod agent_a2a_service_registry;
pub mod agent_a2a_service_rel;
pub mod agent_command_registry;
pub mod agent_command_rel;
pub mod agent_config;
pub mod agent_llm_model;
pub mod agent_llm_model_rel;
pub mod agent_mcp_service_registry;
pub mod agent_mcp_service_rel;
pub mod agent_rule_registry;
pub mod agent_rule_rel;
pub mod agent_session;
pub mod agent_session_agent_state;
pub mod agent_session_event;
pub mod agent_session_pending_input;
pub mod agent_session_state;
pub mod agent_session_user_state;
pub mod agent_skill_package;
pub mod agent_skill_rel;
pub mod agent_sub_agent_rel;
pub mod agent_tool_registry;
pub mod agent_tool_rel;
