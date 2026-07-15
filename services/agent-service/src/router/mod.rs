//! 路由层
pub mod a2a_service_registry;
pub mod a2a_service_rel;
pub mod agent_config;
pub mod agent_run;
pub mod agent_session;
pub mod command_registry;
pub mod command_rel;
pub mod llm_model;
pub mod llm_model_rel;
pub mod mcp_service_registry;
pub mod mcp_service_rel;
pub mod rule_registry;
pub mod rule_rel;
pub mod skill_package;
pub mod skill_rel;
pub mod sub_agent_rel;
pub mod tool_registry;
pub mod tool_rel;

use axum::Router;

/// 路由器
pub struct AgentRouter;

impl AgentRouter {
    /// 注册`智能体服务`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/agent",
            Router::new()
                .merge(agent_config::AgentConfigRouter::register()) // 智能体配置管理
                .merge(agent_run::AgentRunRouter::register()) // 智能体运行
                .merge(agent_session::AgentSessionRouter::register()) // 智能体会话管理
                .merge(command_registry::CommandRegistryRouter::register()) // 命令注册表管理
                .merge(command_rel::CommandRelRouter::register()) // 命令绑定关系管理
                .merge(llm_model::LlmModelRouter::register()) // LLM 模型配置管理
                .merge(rule_registry::RuleRegistryRouter::register()) // 规则注册表管理
                .merge(rule_rel::RuleRelRouter::register()) // 规则绑定关系管理
                .merge(skill_package::SkillPackageRouter::register()) // 技能安装包管理
                .merge(skill_rel::SkillRelRouter::register()) // 技能绑定关系管理
                .merge(tool_registry::ToolRegistryRouter::register()) // 工具注册表管理
                .merge(tool_rel::ToolRelRouter::register()) // 工具绑定关系管理
                .merge(mcp_service_rel::McpServiceRelRouter::register()) // MCP 服务绑定关系管理
                .merge(mcp_service_registry::McpServiceRegistryRouter::register()) // MCP 服务注册表管理
                .merge(a2a_service_rel::A2aServiceRelRouter::register()) // A2A 服务绑定关系管理
                .merge(a2a_service_registry::A2aServiceRegistryRouter::register()) // A2A 服务注册表管理
                .merge(llm_model_rel::LlmModelRelRouter::register()) // LLM 模型绑定关系管理
                .merge(sub_agent_rel::SubAgentRelRouter::register()), // 子智能体绑定关系管理
        )
    }
}
