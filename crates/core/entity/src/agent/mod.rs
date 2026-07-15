//! Agent 域实体模块
//!
//! 包含智能体基座所有核心表的 SeaORM 实体定义。
//!
//! - `t_agent_a2a_service_registry`：A2A 服务注册表
//! - `t_agent_a2a_service_rel`：智能体与 A2A 服务的绑定关系
//! - `t_agent_config`：智能体核心元数据
//! - `t_agent_llm_model`：LLM 模型配置
//! - `t_agent_llm_model_rel`：智能体与模型的绑定关系
//! - `t_agent_tool_registry`：工具注册表
//! - `t_agent_tool_rel`：智能体与工具的绑定关系
//! - `t_agent_rule_registry`：规则注册表
//! - `t_agent_rule_rel`：智能体与规则的绑定关系
//! - `t_agent_skill_rel`：智能体与技能的绑定关系（已安装 skill 索引表）
//! - `t_agent_skill_package`：技能安装包（zip 二进制存储，内置安装通道）
//! - `t_agent_command_registry`：命令注册表
//! - `t_agent_command_rel`：智能体与命令的绑定关系
//! - `t_agent_mcp_service_registry`：MCP 服务注册表
//! - `t_agent_mcp_service_rel`：智能体与 MCP 服务的绑定关系
//! - `t_agent_sub_agent_rel`：智能体与子智能体的绑定关系
//! - `t_agent_session`：会话主表（Phase 1 新增）
//! - `t_agent_session_event`：会话事件流（append-only，Phase 1 新增）
//! - `t_agent_session_state`：会话级 KV 状态（Phase 1 新增）
//! - `t_agent_session_pending_input`：会话注入消息队列（接口层写入，ReAct 循环消费）
//! - `t_agent_user_state`：用户级 KV 状态（跨 agent，Phase 1 新增）
//! - `t_agent_global_state`：智能体全局级 KV 状态（agent 私有，Phase 1 新增）

pub mod a2a_service_registry;
pub mod a2a_service_rel;
pub mod agent_config;
pub mod command_registry;
pub mod command_rel;
pub mod llm_model;
pub mod llm_model_rel;
pub mod mcp_service_registry;
pub mod mcp_service_rel;
pub mod rule_registry;
pub mod rule_rel;
pub mod session;
pub mod session_agent_state;
pub mod session_event;
pub mod session_pending_input;
pub mod session_state;
pub mod session_user_state;
pub mod skill_package;
pub mod skill_rel;
pub mod sub_agent_rel;
pub mod tool_registry;
pub mod tool_rel;

pub use a2a_service_registry::Entity as A2aServiceRegistry;
pub use a2a_service_rel::Entity as A2aServiceRel;
pub use agent_config::Entity as AgentConfig;
pub use command_registry::Entity as CommandRegistry;
pub use command_rel::Entity as CommandRel;
pub use llm_model::Entity as LlmModel;
pub use llm_model_rel::Entity as LlmModelRel;
pub use mcp_service_registry::Entity as McpServiceRegistry;
pub use mcp_service_rel::Entity as McpServiceRel;
pub use rule_registry::Entity as RuleRegistry;
pub use rule_rel::Entity as RuleRel;
pub use session::Entity as Session;
pub use session_agent_state::Entity as GlobalState;
pub use session_event::Entity as SessionEvent;
pub use session_pending_input::Entity as SessionPendingInput;
pub use session_state::Entity as SessionState;
pub use session_user_state::Entity as UserState;
pub use skill_package::Entity as SkillPackage;
pub use skill_rel::Entity as SkillRel;
pub use sub_agent_rel::Entity as SubAgentRel;
pub use tool_registry::Entity as ToolRegistry;
pub use tool_rel::Entity as ToolRel;
