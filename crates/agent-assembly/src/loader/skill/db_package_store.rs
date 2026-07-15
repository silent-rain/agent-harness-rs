//! 基于 SeaORM 的 [`SkillPackageStore`] 实现
//!
//! 操作 `t_agent_skill_package` 与 `t_agent_skill_rel` 两张表，
//! 提供线上模式下的 Skill 安装持久化能力。
//!
//! ## 设计要点
//!
//! - `check`：一次查包 + 一次查关系，合并为 [`SkillCheckResult`] 返回
//! - `install`：单事务内完成"upsert 包 + upsert 关联"，`update=true` 时强制覆盖包内容
//! - `uninstall`：`unlink_only=false` 时若包无其他引用则级联删包
//! - `list`：join 关系表与包表，仅返回已启用的绑定
//! - `load_zip`：按包 ID 加载完整 zip 字节流（运行时专用）
//! - 所有写入路径使用事务保证原子性
//! - 数据库错误通过 [`SkillError::Database`] 直接传播，
//!   无需手写 `map_err`
//!
//! 提供的主要内容：
//! - [`DbSkillPackageStore`] - SeaORM 实现

use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set, TransactionTrait,
};

use agent_runtime::{
    error::SkillError,
    skill::installer::{
        SkillCheckResult, SkillPackage, SkillPackageMeta, SkillPackageRecord, SkillPackageStore,
        SkillRelRecord, store::InstallRecord,
    },
};
use entity::agent::{skill_package, skill_rel};

/// SeaORM 版 [`SkillPackageStore`] 实现
///
/// 线上部署时通过 [`DbSkillPackageStore::new`] 构造一个共享实例，
/// 注入给 [`agent_runtime::skill::installer::DbInstaller`] 使用。
pub struct DbSkillPackageStore {
    db: DatabaseConnection,
}

impl DbSkillPackageStore {
    /// 创建 store 实例
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl SkillPackageStore for DbSkillPackageStore {
    async fn check(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
    ) -> Result<SkillCheckResult, SkillError> {
        // 查包
        let pkg_model = skill_package::Entity::find()
            .filter(skill_package::Column::Deleted.eq(false))
            .filter(skill_package::Column::UserId.eq(user_id))
            .filter(skill_package::Column::SkillCode.eq(skill_code))
            .one(&self.db)
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;

        let package = pkg_model.as_ref().map(package_record_from_model);

        // 查关联（仅当包存在且 agent_id > 0 时才有意义）
        let rel = match &pkg_model {
            Some(pkg) if agent_id > 0 => skill_rel::Entity::find()
                .filter(skill_rel::Column::UserId.eq(user_id))
                .filter(skill_rel::Column::AgentId.eq(agent_id))
                .filter(skill_rel::Column::SkillPackageId.eq(pkg.id))
                .one(&self.db)
                .await
                .map_err(|e| SkillError::Database(e.to_string()))?
                .map(|m| rel_record_from_model(m, skill_code)),
            _ => None,
        };

        Ok(SkillCheckResult { package, rel })
    }

    async fn install(
        &self,
        user_id: i64,
        agent_id: i64,
        pkg: SkillPackage,
        update: bool,
    ) -> Result<InstallRecord, SkillError> {
        let txn = self
            .db
            .begin()
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;

        // ── 1. upsert 包 ──────────────────────────────────────────────────────
        let existing_pkg = skill_package::Entity::find()
            .filter(skill_package::Column::Deleted.eq(false))
            .filter(skill_package::Column::UserId.eq(user_id))
            .filter(skill_package::Column::SkillCode.eq(&pkg.skill_code))
            .one(&txn)
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;

        let pkg_model = match existing_pkg {
            Some(model) if update => {
                // 强制覆盖包内容
                let mut active: skill_package::ActiveModel = model.into();
                active.name = Set(pkg.name.clone());
                active.description = Set(pkg.description.clone());
                active.version = Set(pkg.version.clone());
                active.source_type = Set(pkg.source_type);
                active.source_url = Set(pkg.source_url.clone());
                active.content_zip = Set(pkg.content_zip.clone());
                active.content_sha256 = Set(pkg.content_sha256.clone());
                active.size_bytes = Set(pkg.size_bytes);
                active.installed_by = Set(pkg.installed_by);
                active.status = Set(pkg.status);
                active
                    .update(&txn)
                    .await
                    .map_err(|e| SkillError::Database(e.to_string()))?
            }
            Some(model) => model,
            None => {
                // 首次插入
                let active = skill_package::ActiveModel {
                    user_id: Set(user_id),
                    skill_code: Set(pkg.skill_code.clone()),
                    name: Set(pkg.name.clone()),
                    description: Set(pkg.description.clone()),
                    version: Set(pkg.version.clone()),
                    source_type: Set(pkg.source_type),
                    source_url: Set(pkg.source_url.clone()),
                    content_zip: Set(pkg.content_zip.clone()),
                    content_sha256: Set(pkg.content_sha256.clone()),
                    size_bytes: Set(pkg.size_bytes),
                    installed_by: Set(pkg.installed_by),
                    status: Set(pkg.status),
                    ..Default::default()
                };
                active
                    .insert(&txn)
                    .await
                    .map_err(|e| SkillError::Database(e.to_string()))?
            }
        };

        let record = package_record_from_model(&pkg_model);

        // ── 2. upsert 关联 ────────────────────────────────────────────────────
        let existing_rel = skill_rel::Entity::find()
            .filter(skill_rel::Column::UserId.eq(user_id))
            .filter(skill_rel::Column::AgentId.eq(agent_id))
            .filter(skill_rel::Column::SkillPackageId.eq(pkg_model.id))
            .one(&txn)
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;

        let outcome = if existing_rel.is_some() && !update {
            // 已关联且不强制更新 → Skipped
            txn.commit()
                .await
                .map_err(|e| SkillError::Database(e.to_string()))?;
            return Ok(InstallRecord::Skipped(record));
        } else if existing_rel.is_none() {
            // 新建关联
            let active = skill_rel::ActiveModel {
                user_id: Set(user_id),
                agent_id: Set(agent_id),
                skill_package_id: Set(pkg_model.id),
                enabled: Set(true),
                ..Default::default()
            };
            active
                .insert(&txn)
                .await
                .map_err(|e| SkillError::Database(e.to_string()))?;
            InstallRecord::Installed(record)
        } else {
            // update=true 且关联已存在 → Updated
            InstallRecord::Updated(record)
        };

        txn.commit()
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;
        Ok(outcome)
    }

    async fn uninstall(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
        unlink_only: bool,
    ) -> Result<bool, SkillError> {
        // 查包
        let Some(pkg) = skill_package::Entity::find()
            .filter(skill_package::Column::Deleted.eq(false))
            .filter(skill_package::Column::UserId.eq(user_id))
            .filter(skill_package::Column::SkillCode.eq(skill_code))
            .one(&self.db)
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?
        else {
            return Ok(false);
        };

        // 删除关联
        let result = skill_rel::Entity::delete_many()
            .filter(skill_rel::Column::UserId.eq(user_id))
            .filter(skill_rel::Column::AgentId.eq(agent_id))
            .filter(skill_rel::Column::SkillPackageId.eq(pkg.id))
            .exec(&self.db)
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;

        let removed = result.rows_affected > 0;

        // 若不仅取消关联，且包不再被任何 Agent 引用，则删除包记录
        if removed && !unlink_only {
            let ref_count = skill_rel::Entity::find()
                .filter(skill_rel::Column::SkillPackageId.eq(pkg.id))
                .count(&self.db)
                .await
                .map_err(|e| SkillError::Database(e.to_string()))?;

            if ref_count == 0 {
                skill_package::Entity::delete_by_id(pkg.id)
                    .filter(skill_package::Column::Deleted.eq(false))
                    .exec(&self.db)
                    .await
                    .map_err(|e| SkillError::Database(e.to_string()))?;
            }
        }

        Ok(removed)
    }

    async fn list(&self, user_id: i64, agent_id: i64) -> Result<Vec<SkillPackageMeta>, SkillError> {
        let rels = skill_rel::Entity::find()
            .filter(skill_rel::Column::UserId.eq(user_id))
            .filter(skill_rel::Column::AgentId.eq(agent_id))
            .filter(skill_rel::Column::Enabled.eq(true))
            .all(&self.db)
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;

        if rels.is_empty() {
            return Ok(Vec::new());
        }

        let package_ids: Vec<i64> = rels.iter().map(|r| r.skill_package_id).collect();
        let packages = skill_package::Entity::find()
            .filter(skill_package::Column::Deleted.eq(false))
            .filter(skill_package::Column::Id.is_in(package_ids))
            .all(&self.db)
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;

        Ok(packages.iter().map(package_meta_from_model).collect())
    }

    async fn load_zip(&self, package_id: i64) -> Result<Option<SkillPackage>, SkillError> {
        let pkg = skill_package::Entity::find_by_id(package_id)
            .filter(skill_package::Column::Deleted.eq(false))
            .one(&self.db)
            .await
            .map_err(|e| SkillError::Database(e.to_string()))?;
        Ok(pkg.map(package_from_model))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 转换辅助函数
// ─────────────────────────────────────────────────────────────────────────────

/// `t_agent_skill_package` Model → `SkillPackageRecord`
fn package_record_from_model(model: &skill_package::Model) -> SkillPackageRecord {
    SkillPackageRecord {
        id: model.id,
        user_id: model.user_id,
        meta: package_meta_from_model(model),
    }
}

/// `t_agent_skill_package` Model → `SkillPackageMeta`
fn package_meta_from_model(model: &skill_package::Model) -> SkillPackageMeta {
    SkillPackageMeta {
        skill_code: model.skill_code.clone(),
        name: model.name.clone(),
        description: model.description.clone(),
        version: model.version.clone(),
        source_type: model.source_type,
        source_url: model.source_url.clone(),
        content_sha256: model.content_sha256.clone(),
        size_bytes: model.size_bytes,
        status: model.status,
    }
}

/// `t_agent_skill_package` Model → 完整 `SkillPackage`（含 zip 字节流）
fn package_from_model(model: skill_package::Model) -> SkillPackage {
    SkillPackage {
        skill_code: model.skill_code,
        name: model.name,
        description: model.description,
        version: model.version,
        source_type: model.source_type,
        source_url: model.source_url,
        content_zip: model.content_zip,
        content_sha256: model.content_sha256,
        size_bytes: model.size_bytes,
        installed_by: model.installed_by,
        status: model.status,
    }
}

/// `t_agent_skill_rel` Model → `SkillRelRecord`
fn rel_record_from_model(model: skill_rel::Model, skill_code: &str) -> SkillRelRecord {
    SkillRelRecord {
        id: model.id,
        user_id: model.user_id,
        agent_id: model.agent_id,
        skill_package_id: model.skill_package_id,
        skill_code: skill_code.to_string(),
        enabled: model.enabled,
    }
}
