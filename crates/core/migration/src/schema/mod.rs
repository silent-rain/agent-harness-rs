//! Schema 模块：建表（DDL）
//!
//! 表定义按业务域分组，文件名与表名一致（去掉 `t_` 前缀）。
//! 全部采用 entity-first 写法：`db.get_schema_builder().register(Entity).apply(db)`。

use sea_orm_migration::MigrationTrait;

pub mod agent;

/// 迁移任务（按表依赖顺序注册）
pub fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![
        // Agent 基座
        Box::new(agent::agent_config::Migration),
        Box::new(agent::agent_llm_model::Migration),
        Box::new(agent::agent_llm_model_rel::Migration),
        // 工具
        Box::new(agent::agent_tool_registry::Migration),
        Box::new(agent::agent_tool_rel::Migration),
        // 规则
        Box::new(agent::agent_rule_registry::Migration),
        Box::new(agent::agent_rule_rel::Migration),
        // 技能
        Box::new(agent::agent_skill_package::Migration),
        Box::new(agent::agent_skill_rel::Migration),
        // 命令
        Box::new(agent::agent_command_registry::Migration),
        Box::new(agent::agent_command_rel::Migration),
        // MCP 服务
        Box::new(agent::agent_mcp_service_registry::Migration),
        Box::new(agent::agent_mcp_service_rel::Migration),
        // A2A 服务
        Box::new(agent::agent_a2a_service_registry::Migration),
        Box::new(agent::agent_a2a_service_rel::Migration),
        // 子智能体
        Box::new(agent::agent_sub_agent_rel::Migration),
        // 会话运行时
        Box::new(agent::agent_session::Migration),
        Box::new(agent::agent_session_event::Migration),
        Box::new(agent::agent_session_state::Migration),
        Box::new(agent::agent_session_pending_input::Migration),
        Box::new(agent::agent_session_user_state::Migration),
        Box::new(agent::agent_session_agent_state::Migration),
    ]
}
