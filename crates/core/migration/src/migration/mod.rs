//! 版本化迁移（Versioned Migration）
//!
//! 表结构变更、数据迁移等增量变更。
//! 按业务域子目录组织，文件只增不减。
//! 命名规则：`m{日期}_{序号}_{描述}.rs`

use sea_orm_migration::MigrationTrait;

/// 迁移任务
pub fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![]
}
