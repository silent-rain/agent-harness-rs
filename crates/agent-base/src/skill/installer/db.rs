//! 线上数据库安装器
//!
//! 通过 [`SkillPackageStore`] 将 Skill 安装包写入数据库的两张表：
//! - `t_agent_skill_package`：物理包（按 `(user_id, skill_code)` 去重）
//! - `t_agent_skill_rel`：Agent 与 skill 的绑定关系
//!
//! ## 业务流程
//!
//! - **install**: 已关联 → `Skipped`；未关联 → 写包 + link → `Installed`
//! - **update**: 强制覆盖包内容，并确保 link 存在 → `Updated`
//! - **uninstall**: 仅解除 (user, agent, skill_code) 的绑定，不删除包本身
//! - **list / query**: 直通 store 的 `list` / `check`
//!
//! 安装期不做 zip 解压，运行时由 `LocalSkillLoader` 或下游加载器按需取 zip。

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, info};

use super::local_dir::LocalDirInstaller;
use super::source::SkillSource;
use super::store::{InstallRecord, SkillPackageStore};
use super::types::{InstallOutcome, SkillCheckResult, SkillPackageMeta, SkillPackageRecord};
use crate::constant::DEFAULT_SKILL_PACKAGE_MAX_BYTES;
use crate::error::SkillError;

/// 线上安装器
///
/// 持有 [`SkillPackageStore`] 引用与一个仅用于打包的 [`LocalDirInstaller`]
/// （仅复用 zip 校验/打包流程，不写文件系统）。
pub struct DbInstaller {
    store: Arc<dyn SkillPackageStore>,
    /// 仅用于复用 prepare 流程（zip 打包 + manifest 解析），不会落到本地磁盘
    packer: LocalDirInstaller,
}

impl DbInstaller {
    /// 创建线上安装器
    ///
    /// # Parameters
    ///
    /// * `store` - 持久化 Store 实现
    pub fn new(store: Arc<dyn SkillPackageStore>) -> Self {
        // packer 的 skills_root 此处仅作占位，不会真正写入
        let packer = LocalDirInstaller::new(PathBuf::from("."))
            .with_max_bytes(DEFAULT_SKILL_PACKAGE_MAX_BYTES);
        Self { store, packer }
    }

    /// 自定义包大小上限
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.packer = LocalDirInstaller::new(PathBuf::from(".")).with_max_bytes(max_bytes);
        self
    }

    /// 暴露底层 Store（供下游加载器按需取 zip 字节流）
    pub fn store(&self) -> Arc<dyn SkillPackageStore> {
        self.store.clone()
    }

    /// 安装：已关联则跳过，否则写包 + link
    ///
    /// # Parameters
    ///
    /// * `user_id`    - 所属用户 ID
    /// * `agent_id`   - 智能体 ID
    /// * `skill_code` - 期望的技能编码（None 时从 SKILL.md 推断）
    /// * `source`     - 安装来源
    /// * `installed_by` - 安装者用户 ID（系统传 0）
    pub async fn install(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: Option<String>,
        source: SkillSource,
        installed_by: i64,
    ) -> Result<InstallOutcome, SkillError> {
        let prepared = self
            .packer
            .prepare(skill_code, source)
            .await
            .map_err(|e| SkillError::Internal(e.to_string()))?;
        let resolved_code = prepared.skill_code.clone();
        let pkg = prepared.into_package(installed_by);

        let record = self.store.install(user_id, agent_id, pkg, false).await?;
        match record {
            InstallRecord::Skipped(rec) => {
                debug!(
                    user_id, agent_id, skill_code = %resolved_code,
                    "Skill already linked, skipping install"
                );
                Ok(InstallOutcome::Skipped(rec.meta))
            }
            InstallRecord::Installed(rec) => {
                info!(
                    user_id, agent_id, skill_code = %resolved_code,
                    package_id = rec.id, "Skill package installed (db)"
                );
                Ok(InstallOutcome::Installed(rec.meta))
            }
            InstallRecord::Updated(rec) => Ok(InstallOutcome::Updated(rec.meta)),
        }
    }

    /// 更新：覆盖该 user 名下的 skill_code 对应的包内容
    ///
    /// 语义：
    /// - 直接强制覆盖包内容（zip 字节流、sha、version 等）
    /// - 同时确保该 (user, agent) 已 link 到该包，未 link 时补建
    /// - **影响所有引用该包的其他 Agent**（按设计如此）
    pub async fn update(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: Option<String>,
        source: SkillSource,
        installed_by: i64,
    ) -> Result<InstallOutcome, SkillError> {
        let prepared = self
            .packer
            .prepare(skill_code, source)
            .await
            .map_err(|e| SkillError::Internal(e.to_string()))?;
        let resolved_code = prepared.skill_code.clone();
        let pkg = prepared.into_package(installed_by);

        let record = self.store.install(user_id, agent_id, pkg, true).await?;
        info!(
            user_id, agent_id, skill_code = %resolved_code,
            package_id = record.record().id, "Skill package updated (db)"
        );
        Ok(InstallOutcome::Updated(record.record().meta.clone()))
    }

    /// 卸载：仅解除当前 (user, agent, skill_code) 的绑定，不删除包本身
    pub async fn uninstall(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
    ) -> Result<bool, SkillError> {
        let removed = self
            .store
            .uninstall(user_id, agent_id, skill_code, true)
            .await?;
        if removed {
            info!(user_id, agent_id, skill_code, "Skill rel unlinked (db)");
        } else {
            debug!(
                user_id,
                agent_id, skill_code, "Skill rel not found, nothing to unlink"
            );
        }
        Ok(removed)
    }

    /// 列出 (user, agent) 已启用的全部技能元数据
    pub async fn list(
        &self,
        user_id: i64,
        agent_id: i64,
    ) -> Result<Vec<SkillPackageMeta>, SkillError> {
        self.store.list(user_id, agent_id).await
    }

    /// 查询单个技能的注册与关联状态
    pub async fn check(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
    ) -> Result<SkillCheckResult, SkillError> {
        self.store.check(user_id, agent_id, skill_code).await
    }

    /// 查询单个技能的注册信息（不区分是否已绑定到当前 agent）
    pub async fn query(
        &self,
        user_id: i64,
        skill_code: &str,
    ) -> Result<Option<SkillPackageRecord>, SkillError> {
        // 使用 agent_id=0 仅查包注册状态，rel 部分忽略
        let result = self.store.check(user_id, 0, skill_code).await?;
        Ok(result.package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constant::SKILL_ENTRY_FILE;
    use crate::skill::installer::store::InMemorySkillPackageStore;
    use std::path::Path;
    use tempfile::TempDir;

    fn write_skill_dir(root: &Path, code: &str) {
        let dir = root.join(code);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(SKILL_ENTRY_FILE),
            "---\nname: My Skill\nversion: \"1.2.3\"\ndescription: hi\n---\n\nbody",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_install_creates_package_and_link() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "demo");

        let store: Arc<dyn SkillPackageStore> = Arc::new(InMemorySkillPackageStore::new());
        let installer = DbInstaller::new(store.clone());

        let outcome = installer
            .install(
                1,
                100,
                None,
                SkillSource::LocalDir(staging.path().join("demo")),
                1,
            )
            .await
            .unwrap();
        assert!(matches!(outcome, InstallOutcome::Installed(_)));
        assert_eq!(outcome.meta().skill_code, "demo");

        // 关系已建立
        let result = store.check(1, 100, "demo").await.unwrap();
        assert!(result.is_linked());
    }

    #[tokio::test]
    async fn test_install_returns_skipped_when_already_linked() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "demo");

        let store: Arc<dyn SkillPackageStore> = Arc::new(InMemorySkillPackageStore::new());
        let installer = DbInstaller::new(store);

        installer
            .install(
                1,
                100,
                None,
                SkillSource::LocalDir(staging.path().join("demo")),
                1,
            )
            .await
            .unwrap();
        let outcome = installer
            .install(
                1,
                100,
                None,
                SkillSource::LocalDir(staging.path().join("demo")),
                1,
            )
            .await
            .unwrap();
        assert!(matches!(outcome, InstallOutcome::Skipped(_)));
        assert!(!outcome.is_mutated());
    }

    #[tokio::test]
    async fn test_update_overrides_package_content() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "demo");

        let store: Arc<dyn SkillPackageStore> = Arc::new(InMemorySkillPackageStore::new());
        let installer = DbInstaller::new(store.clone());

        installer
            .install(
                1,
                100,
                None,
                SkillSource::LocalDir(staging.path().join("demo")),
                1,
            )
            .await
            .unwrap();

        // 修改源内容
        std::fs::write(
            staging.path().join("demo").join(SKILL_ENTRY_FILE),
            "---\nname: My Skill\nversion: \"2.0.0\"\ndescription: new\n---\n\nbody",
        )
        .unwrap();

        let outcome = installer
            .update(
                1,
                100,
                None,
                SkillSource::LocalDir(staging.path().join("demo")),
                1,
            )
            .await
            .unwrap();
        assert!(matches!(outcome, InstallOutcome::Updated(_)));
        assert_eq!(outcome.meta().version, "2.0.0");

        let result = store.check(1, 100, "demo").await.unwrap();
        assert_eq!(result.package.unwrap().meta.version, "2.0.0");
    }

    #[tokio::test]
    async fn test_uninstall_only_removes_rel() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "demo");

        let store: Arc<dyn SkillPackageStore> = Arc::new(InMemorySkillPackageStore::new());
        let installer = DbInstaller::new(store.clone());

        installer
            .install(
                1,
                100,
                None,
                SkillSource::LocalDir(staging.path().join("demo")),
                1,
            )
            .await
            .unwrap();
        assert!(installer.uninstall(1, 100, "demo").await.unwrap());
        assert!(!installer.uninstall(1, 100, "demo").await.unwrap());

        // 包仍在
        let result = store.check(1, 100, "demo").await.unwrap();
        assert!(result.is_registered());
        // rel 已删
        assert!(!result.is_linked());
    }

    #[tokio::test]
    async fn test_list_returns_only_current_agent() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "demo");

        let store: Arc<dyn SkillPackageStore> = Arc::new(InMemorySkillPackageStore::new());
        let installer = DbInstaller::new(store);

        installer
            .install(
                1,
                100,
                None,
                SkillSource::LocalDir(staging.path().join("demo")),
                1,
            )
            .await
            .unwrap();
        let metas = installer.list(1, 100).await.unwrap();
        assert_eq!(metas.len(), 1);
        let metas = installer.list(1, 200).await.unwrap();
        assert!(metas.is_empty());
    }
}
