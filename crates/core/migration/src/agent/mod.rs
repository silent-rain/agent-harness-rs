//! Agent 域数据库迁移模块
//!
//! 包含智能体基座所有核心表的迁移定义。
//! 所有表统一使用 `t_agent_` 前缀。
//!
//! 表依赖顺序：
//! - `t_agent_llm_model`（无依赖）
//! - `t_agent_config`（无依赖）
//! - `t_agent_llm_model_rel`（依赖 `t_agent_config`、`t_agent_llm_model`）
//! - `t_agent_tool_registry`（无依赖）
//! - `t_agent_tool_rel`（依赖 `t_agent_config`、`t_agent_tool_registry`）
//! - `t_agent_rule_registry`（无依赖）
//! - `t_agent_rule_rel`（依赖 `t_agent_config`、`t_agent_rule_registry`）
//! - `t_agent_skill_rel`（依赖 `t_agent_config`）
//! - `t_agent_skill_package`（无依赖，zip 包存储表）
//! - `t_agent_command_registry`（无依赖）
//! - `t_agent_command_rel`（依赖 `t_agent_config`、`t_agent_command_registry`）
//! - `t_agent_mcp_service_registry`（无依赖）
//! - `t_agent_mcp_service_rel`（依赖 `t_agent_config`、`t_agent_mcp_service_registry`）
//! - `t_agent_a2a_service_registry`（无依赖）
//! - `t_agent_a2a_service_rel`（依赖 `t_agent_config`、`t_agent_a2a_service_registry`）
//! - `t_agent_sub_agent_rel`（依赖 `t_agent_config`）
//! - `t_agent_session`（依赖 `t_agent_config`，Phase 1 新增）
//! - `t_agent_session_event`（逻辑依赖 `t_agent_session`，Phase 1 新增）
//! - `t_agent_session_state`（逻辑依赖 `t_agent_session`，Phase 1 新增）
//! - `t_agent_session_pending_input`（逻辑依赖 `t_agent_session`，注入消息队列）
//! - `t_agent_user_state`（逻辑依赖 user_base，跨 agent，Phase 1 新增）
//! - `t_agent_global_state`（逻辑依赖 `t_agent_config`，agent 私有，Phase 1 新增）

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
