//! 数据初始化（Seed）
//!
//! 在空表中插入初始数据，通常用于配置表、字典表、注册表等。
//! 不是所有表都需要 seed，数量天花板很低（3~8 个）。
//! 文件命名：`s_{表名}.rs`。

pub mod s_agent_tool_registry;

use sea_orm_migration::MigrationTrait;

/// 迁移任务
pub fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![
        // 工具注册表：预置平台内建工具（list_dir / read_file / search_files / run_command）
        Box::new(s_agent_tool_registry::Migration),
    ]
}
