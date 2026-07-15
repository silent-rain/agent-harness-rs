//! 智能体服务模块
pub mod dto;
pub mod enums;

pub(crate) mod dao;
pub use dao::{
    a2a_service_registry::A2aServiceRegistryDao, a2a_service_rel::A2aServiceRelDao,
    agent_config::AgentConfigDao, agent_session::AgentSessionDao,
    command_registry::CommandRegistryDao, command_rel::CommandRelDao, llm_model::LlmModelDao,
    llm_model_rel::LlmModelRelDao, mcp_service_registry::McpServiceRegistryDao,
    mcp_service_rel::McpServiceRelDao, rule_registry::RuleRegistryDao, rule_rel::RuleRelDao,
    session_pending_input::SessionPendingInputDao, skill_package::SkillPackageDao,
    skill_rel::SkillRelDao, sub_agent_rel::SubAgentRelDao, tool_registry::ToolRegistryDao,
    tool_rel::ToolRelDao,
};

pub(crate) mod service;
pub use service::{
    a2a_service_registry::A2aServiceRegistryService, a2a_service_rel::A2aServiceRelService,
    agent_config::AgentConfigService, agent_run::AgentRunService,
    agent_session::AgentSessionService, command_registry::CommandRegistryService,
    command_rel::CommandRelService, llm_model::LlmModelService, llm_model_rel::LlmModelRelService,
    mcp_service_registry::McpServiceRegistryService, mcp_service_rel::McpServiceRelService,
    permission::PermissionService, rule_registry::RuleRegistryService, rule_rel::RuleRelService,
    skill_package::SkillPackageService, skill_rel::SkillRelService,
    sub_agent_rel::SubAgentRelService, tool_registry::ToolRegistryService,
    tool_rel::ToolRelService,
};

pub(crate) mod controller;
pub use controller::{
    a2a_service_registry::A2aServiceRegistryController, a2a_service_rel::A2aServiceRelController,
    agent_config::AgentConfigController, agent_run::AgentRunController,
    agent_session::AgentSessionController, command_registry::CommandRegistryController,
    command_rel::CommandRelController, llm_model::LlmModelController,
    llm_model_rel::LlmModelRelController, mcp_service_registry::McpServiceRegistryController,
    mcp_service_rel::McpServiceRelController, rule_registry::RuleRegistryController,
    rule_rel::RuleRelController, skill_package::SkillPackageController,
    skill_rel::SkillRelController, sub_agent_rel::SubAgentRelController,
    tool_registry::ToolRegistryController, tool_rel::ToolRelController,
};

pub(crate) mod router;
pub use router::{
    AgentRouter, a2a_service_registry::A2aServiceRegistryRouter,
    a2a_service_rel::A2aServiceRelRouter, agent_config::AgentConfigRouter,
    agent_run::AgentRunRouter, agent_session::AgentSessionRouter,
    command_registry::CommandRegistryRouter, command_rel::CommandRelRouter,
    llm_model::LlmModelRouter, llm_model_rel::LlmModelRelRouter,
    mcp_service_registry::McpServiceRegistryRouter, mcp_service_rel::McpServiceRelRouter,
    rule_registry::RuleRegistryRouter, rule_rel::RuleRelRouter, skill_package::SkillPackageRouter,
    skill_rel::SkillRelRouter, sub_agent_rel::SubAgentRelRouter, tool_registry::ToolRegistryRouter,
    tool_rel::ToolRelRouter,
};
