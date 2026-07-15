//! Agent 域数据库迁移模块
//!
//! 包含智能体基座所有核心表的迁移定义。
//! 所有表统一使用 `t_agent_` 前缀。
//!
//! 提供的主要内容：
//! - `agent`: Agent 域所有表的迁移定义子模块
//! - `utils`: 迁移工具函数（索引创建等）
//!
//! 注意：本 Migrator 同时包含 core/migration 的所有迁移，
//! 作为整个项目的统一迁移入口，避免多个 Migrator 共用同一数据库时的冲突。

pub use sea_orm_migration::migrator::MigratorTrait;
use sea_orm_migration::{MigrationTrait, async_trait};

pub mod utils;

pub mod agent;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        let mut migrations: Vec<Box<dyn MigrationTrait>> = vec![];

        // Agent 域（按依赖顺序）
        migrations.extend(vec![
            Box::new(agent::agent_config::Migration) as Box<dyn MigrationTrait>,
            Box::new(agent::agent_llm_model::Migration),
            Box::new(agent::agent_llm_model_rel::Migration),
            Box::new(agent::agent_tool_registry::Migration),
            Box::new(agent::agent_tool_rel::Migration),
            Box::new(agent::agent_rule_registry::Migration),
            Box::new(agent::agent_rule_rel::Migration),
            Box::new(agent::agent_skill_package::Migration),
            Box::new(agent::agent_skill_rel::Migration),
            Box::new(agent::agent_command_registry::Migration),
            Box::new(agent::agent_command_rel::Migration),
            Box::new(agent::agent_mcp_service_registry::Migration),
            Box::new(agent::agent_mcp_service_rel::Migration),
            Box::new(agent::agent_a2a_service_registry::Migration),
            Box::new(agent::agent_a2a_service_rel::Migration),
            Box::new(agent::agent_sub_agent_rel::Migration),
            Box::new(agent::agent_session::Migration),
            Box::new(agent::agent_session_event::Migration),
            Box::new(agent::agent_session_state::Migration),
            Box::new(agent::agent_session_pending_input::Migration),
            Box::new(agent::agent_session_user_state::Migration),
            Box::new(agent::agent_session_agent_state::Migration),
        ]);

        migrations
    }
}
