//! Agent 域数据库迁移模块
//!
//! 分三层组织，注册顺序固定为 **schema → seed → migration**：
//!
//! - [`schema`]：建表（DDL），按业务域分子目录，文件名与表名一致（去掉 `t_` 前缀）
//! - [`seed`]：初始数据（DML），平铺，文件名 `s_{表名}.rs`
//! - [`migration`]：版本化迁移，按业务域分子目录，文件名 `m{日期}_{序号}_{描述}.rs`
//!
//! 所有表统一使用 `t_agent_` 前缀。
//! 注意：本 Migrator 同时包含 core/migration 的所有迁移，
//! 作为整个项目的统一迁移入口，避免多个 Migrator 共用同一数据库时的冲突。

pub use sea_orm_migration::migrator::MigratorTrait;
use sea_orm_migration::{MigrationTrait, async_trait};

pub mod utils;

pub mod migration;
pub mod schema;
pub mod seed;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        let mut migrations: Vec<Box<dyn MigrationTrait>> = vec![];

        // 1. schema：建表
        migrations.extend(schema::migrations());
        // 2. seed：初始数据
        migrations.extend(seed::migrations());
        // 3. migration：版本化增量变更
        migrations.extend(migration::migrations());

        migrations
    }
}
