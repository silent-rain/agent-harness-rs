//! Skill 安装包持久化抽象（线上模式专用）
//!
//! 定义 [`SkillPackageStore`] trait，将 agent-base 与具体存储实现（SeaORM / 内存）解耦。
//! 真实实现位于 `agent-assembly::loader::skill::db_store`，agent-base 内仅提供
//! [`InMemorySkillPackageStore`] 用于单元测试与离线运行。
//!
//! ## 设计要点
//!
//! 该 trait 仅服务于"线上 DB 安装"场景，不再承担本地文件系统加载职责。
//! 本地模式由 [`crate::skill::installer::LocalDirInstaller`] 直接读写
//! `<skills_root>` 目录，与 store 无关。
//!
//! 数据模型：
//! - `t_agent_skill_package`：物理包（`(user_id, skill_code)` 联合唯一）
//! - `t_agent_skill_rel`：Agent 与 skill 的绑定关系
//!
//! ## 五个方法对应的业务语义
//!
//! | 方法          | 说明                                         |
//! |---------------|----------------------------------------------|
//! | `check`       | 查询包注册状态与 Agent 关联状态              |
//! | `install`     | 注册包 + 建立关联（`update=true` 时覆盖包）  |
//! | `uninstall`   | 取消关联（`unlink_only=false` 时同时删包）   |
//! | `list`        | 列出 Agent 已绑定的全部技能元数据            |
//! | `load_zip`    | 按包 ID 加载完整 zip 字节流（运行时专用）    |

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::types::{
    SkillCheckResult, SkillPackage, SkillPackageMeta, SkillPackageRecord, SkillRelRecord,
};
use crate::error::SkillError;

/// Skill 安装包持久化 Store（线上模式）
///
/// 实现方负责操作 `t_agent_skill_package` 与 `t_agent_skill_rel` 两张表，
/// 保证：
/// - `(user_id, skill_code)` 在包表中唯一
/// - `(user_id, agent_id, skill_package_id)` 在关系表中唯一
///
/// 实现方应在涉及多表写入的方法内自行管理事务边界。
#[async_trait]
pub trait SkillPackageStore: Send + Sync + 'static {
    /// 查询包注册状态与 Agent 关联状态
    ///
    /// 合并"包是否已注册"与"当前 Agent 是否已关联"两个维度，
    /// 供调用方决策是否需要执行 install / uninstall。
    ///
    /// # Parameters
    ///
    /// * `user_id`    - 所属用户 ID（0 = 系统级）
    /// * `agent_id`   - 智能体 ID
    /// * `skill_code` - 技能编码
    async fn check(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
    ) -> Result<SkillCheckResult, SkillError>;

    /// 安装：注册包并建立 Agent 关联
    ///
    /// - `update = false`：若当前 Agent 已关联则幂等跳过，返回 `Skipped`；
    ///   若包已注册但未关联，则仅补建关联，返回 `Installed`。
    /// - `update = true`：强制覆盖包内容（zip / sha / version），
    ///   并确保关联存在，返回 `Updated`。
    ///
    /// 实现方应在单次事务内完成"upsert 包 + upsert 关联"。
    ///
    /// # Parameters
    ///
    /// * `user_id`    - 所属用户 ID
    /// * `agent_id`   - 智能体 ID
    /// * `pkg`        - 完整包数据（含 zip 字节流）
    /// * `update`     - 是否强制覆盖已有包内容
    async fn install(
        &self,
        user_id: i64,
        agent_id: i64,
        pkg: SkillPackage,
        update: bool,
    ) -> Result<InstallRecord, SkillError>;

    /// 卸载：取消 Agent 关联，可选同时删除包
    ///
    /// - `unlink_only = true`：仅解除 `(user, agent, skill_code)` 绑定，保留包记录。
    /// - `unlink_only = false`：解除绑定后，若该包不再被任何 Agent 引用，则一并删除包记录。
    ///
    /// # Parameters
    ///
    /// * `user_id`     - 所属用户 ID
    /// * `agent_id`    - 智能体 ID
    /// * `skill_code`  - 技能编码
    /// * `unlink_only` - 是否仅取消关联（不删包）
    ///
    /// # Returns
    ///
    /// 是否实际解除了关联（`false` 表示原本就不存在该绑定）
    async fn uninstall(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
        unlink_only: bool,
    ) -> Result<bool, SkillError>;

    /// 列出指定 Agent 已绑定且启用的全部技能元数据
    ///
    /// 主要用于 `skill_manage` 工具的 `list` 子命令。
    ///
    /// # Parameters
    ///
    /// * `user_id`  - 所属用户 ID
    /// * `agent_id` - 智能体 ID
    async fn list(&self, user_id: i64, agent_id: i64) -> Result<Vec<SkillPackageMeta>, SkillError>;

    /// 按包 ID 加载完整包（含 zip 字节流）
    ///
    /// 仅用于运行时按需取包内容；常规列表请用 [`Self::list`]。
    ///
    /// # Parameters
    ///
    /// * `package_id` - 包主键（`t_agent_skill_package.id`）
    async fn load_zip(&self, package_id: i64) -> Result<Option<SkillPackage>, SkillError>;
}

/// `install` 操作的结果
///
/// 由 [`SkillPackageStore::install`] 返回，便于调用方区分三种情况。
#[derive(Debug, Clone)]
pub enum InstallRecord {
    /// 首次安装：包注册 + 关联均为新建
    Installed(SkillPackageRecord),
    /// 已存在关联：跳过任何写入
    Skipped(SkillPackageRecord),
    /// 覆盖更新：包内容被替换
    Updated(SkillPackageRecord),
}

impl InstallRecord {
    /// 提取内部的包记录（无论何种结果）
    pub fn record(&self) -> &SkillPackageRecord {
        match self {
            Self::Installed(r) | Self::Skipped(r) | Self::Updated(r) => r,
        }
    }

    /// 操作是否实际写入了数据库
    pub fn is_mutated(&self) -> bool {
        !matches!(self, Self::Skipped(_))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 内存实现
// ─────────────────────────────────────────────────────────────────────────────

/// 内存版 [`SkillPackageStore`] 实现
///
/// 用于单元测试与未配置数据库的运行模式。进程重启数据丢失，**不可用于生产**。
#[derive(Debug, Default, Clone)]
pub struct InMemorySkillPackageStore {
    inner: Arc<RwLock<MemoryState>>,
}

#[derive(Debug, Default)]
struct MemoryState {
    /// (user_id, skill_code) → 包记录（含 id、zip 内容）
    packages: HashMap<(i64, String), MemoryPackage>,
    /// (user_id, agent_id, skill_package_id) → 关系记录
    rels: HashMap<(i64, i64, i64), SkillRelRecord>,
    next_package_id: i64,
    next_rel_id: i64,
}

#[derive(Debug, Clone)]
struct MemoryPackage {
    id: i64,
    pkg: SkillPackage,
}

impl InMemorySkillPackageStore {
    /// 创建空的内存 store
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
#[async_trait]
impl SkillPackageStore for InMemorySkillPackageStore {
    async fn check(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
    ) -> Result<SkillCheckResult, SkillError> {
        let guard = self.inner.read().await;
        let package = guard.packages.get(&(user_id, skill_code.to_string())).map(
            |MemoryPackage { id, pkg }| SkillPackageRecord {
                id: *id,
                user_id,
                meta: SkillPackageMeta::from(pkg),
            },
        );
        let rel = package
            .as_ref()
            .and_then(|rec| guard.rels.get(&(user_id, agent_id, rec.id)).cloned());
        Ok(SkillCheckResult { package, rel })
    }

    async fn install(
        &self,
        user_id: i64,
        agent_id: i64,
        pkg: SkillPackage,
        update: bool,
    ) -> Result<InstallRecord, SkillError> {
        let mut guard = self.inner.write().await;
        let key = (user_id, pkg.skill_code.clone());

        // 若已关联且不强制更新，直接跳过
        if !update && let Some(existing) = guard.packages.get(&key) {
            let rel_key = (user_id, agent_id, existing.id);
            if guard.rels.contains_key(&rel_key) {
                let record = SkillPackageRecord {
                    id: existing.id,
                    user_id,
                    meta: SkillPackageMeta::from(&existing.pkg),
                };
                return Ok(InstallRecord::Skipped(record));
            }
        }

        // upsert 包
        let id = match guard.packages.get(&key) {
            Some(existing) => existing.id,
            None => {
                guard.next_package_id += 1;
                guard.next_package_id
            }
        };
        let is_update = guard.packages.contains_key(&key) && update;
        let meta = SkillPackageMeta::from(&pkg);
        guard.packages.insert(key, MemoryPackage { id, pkg });

        // upsert 关联
        let rel_key = (user_id, agent_id, id);
        if !guard.rels.contains_key(&rel_key) {
            guard.next_rel_id += 1;
            let rel_id = guard.next_rel_id;
            let skill_code = guard
                .packages
                .iter()
                .find_map(|((uid, code), p)| {
                    if *uid == user_id && p.id == id {
                        Some(code.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            guard.rels.insert(
                rel_key,
                SkillRelRecord {
                    id: rel_id,
                    user_id,
                    agent_id,
                    skill_package_id: id,
                    skill_code,
                    enabled: true,
                },
            );
        }

        let record = SkillPackageRecord { id, user_id, meta };
        if is_update {
            Ok(InstallRecord::Updated(record))
        } else {
            Ok(InstallRecord::Installed(record))
        }
    }

    async fn uninstall(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
        unlink_only: bool,
    ) -> Result<bool, SkillError> {
        let mut guard = self.inner.write().await;
        let Some(MemoryPackage { id, .. }) = guard
            .packages
            .get(&(user_id, skill_code.to_string()))
            .cloned()
        else {
            return Ok(false);
        };

        let removed = guard.rels.remove(&(user_id, agent_id, id)).is_some();

        // 若不仅取消关联，且包不再被任何 Agent 引用，则删除包记录
        if removed && !unlink_only {
            let still_referenced = guard.rels.values().any(|r| r.skill_package_id == id);
            if !still_referenced {
                guard.packages.remove(&(user_id, skill_code.to_string()));
            }
        }

        Ok(removed)
    }

    async fn list(&self, user_id: i64, agent_id: i64) -> Result<Vec<SkillPackageMeta>, SkillError> {
        let guard = self.inner.read().await;
        let metas = guard
            .rels
            .values()
            .filter(|r| r.user_id == user_id && r.agent_id == agent_id && r.enabled)
            .filter_map(|rel| {
                guard
                    .packages
                    .values()
                    .find(|p| p.id == rel.skill_package_id)
                    .map(|p| SkillPackageMeta::from(&p.pkg))
            })
            .collect();
        Ok(metas)
    }

    async fn load_zip(&self, package_id: i64) -> Result<Option<SkillPackage>, SkillError> {
        let guard = self.inner.read().await;
        Ok(guard
            .packages
            .values()
            .find(|p| p.id == package_id)
            .map(|p| p.pkg.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pkg(code: &str) -> SkillPackage {
        SkillPackage {
            skill_code: code.to_string(),
            name: code.to_string(),
            description: "desc".to_string(),
            version: "0.0.1".to_string(),
            source_type: 1,
            source_url: "test://local".to_string(),
            content_zip: vec![1, 2, 3],
            content_sha256: "deadbeef".to_string(),
            size_bytes: 3,
            installed_by: 0,
            status: 0,
        }
    }

    #[tokio::test]
    async fn test_install_assigns_stable_id() {
        let store = InMemorySkillPackageStore::new();
        let r1 = store.install(1, 100, sample_pkg("a"), false).await.unwrap();
        // 第二次 install（update=false）应 Skipped，id 不变
        let r2 = store.install(1, 100, sample_pkg("a"), false).await.unwrap();
        assert!(matches!(r1, InstallRecord::Installed(_)));
        assert!(matches!(r2, InstallRecord::Skipped(_)));
        assert_eq!(r1.record().id, r2.record().id);
    }

    #[tokio::test]
    async fn test_install_update_overwrites_package() {
        let store = InMemorySkillPackageStore::new();
        store.install(1, 100, sample_pkg("a"), false).await.unwrap();
        let mut updated = sample_pkg("a");
        updated.version = "1.0.0".to_string();
        let r = store.install(1, 100, updated, true).await.unwrap();
        assert!(matches!(r, InstallRecord::Updated(_)));
        assert_eq!(r.record().meta.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_check_returns_registration_and_link_status() {
        let store = InMemorySkillPackageStore::new();

        // 未注册
        let result = store.check(1, 100, "a").await.unwrap();
        assert!(!result.is_registered());
        assert!(!result.is_linked());

        // 已注册但未关联另一个 agent
        store.install(1, 100, sample_pkg("a"), false).await.unwrap();
        let result = store.check(1, 200, "a").await.unwrap();
        assert!(result.is_registered());
        assert!(!result.is_linked());

        // 已注册且已关联
        let result = store.check(1, 100, "a").await.unwrap();
        assert!(result.is_registered());
        assert!(result.is_linked());
    }

    #[tokio::test]
    async fn test_uninstall_unlink_only_keeps_package() {
        let store = InMemorySkillPackageStore::new();
        store.install(1, 100, sample_pkg("a"), false).await.unwrap();

        // unlink_only=true：仅取消关联，包保留
        assert!(store.uninstall(1, 100, "a", true).await.unwrap());
        let result = store.check(1, 100, "a").await.unwrap();
        assert!(result.is_registered());
        assert!(!result.is_linked());

        // 再次卸载返回 false
        assert!(!store.uninstall(1, 100, "a", true).await.unwrap());
    }

    #[tokio::test]
    async fn test_uninstall_removes_package_when_no_other_refs() {
        let store = InMemorySkillPackageStore::new();
        store.install(1, 100, sample_pkg("a"), false).await.unwrap();

        // unlink_only=false：取消关联 + 无其他引用时删包
        assert!(store.uninstall(1, 100, "a", false).await.unwrap());
        let result = store.check(1, 100, "a").await.unwrap();
        assert!(!result.is_registered());
    }

    #[tokio::test]
    async fn test_uninstall_keeps_package_when_other_agent_still_linked() {
        let store = InMemorySkillPackageStore::new();
        store.install(1, 100, sample_pkg("a"), false).await.unwrap();
        store.install(1, 200, sample_pkg("a"), false).await.unwrap();

        // agent 100 卸载（unlink_only=false），但 agent 200 仍引用，包应保留
        assert!(store.uninstall(1, 100, "a", false).await.unwrap());
        let result = store.check(1, 200, "a").await.unwrap();
        assert!(result.is_registered());
        assert!(result.is_linked());
    }

    #[tokio::test]
    async fn test_list_returns_only_current_agent_bindings() {
        let store = InMemorySkillPackageStore::new();
        store.install(1, 100, sample_pkg("a"), false).await.unwrap();
        store.install(1, 100, sample_pkg("b"), false).await.unwrap();
        store.install(1, 200, sample_pkg("a"), false).await.unwrap();

        let metas = store.list(1, 100).await.unwrap();
        assert_eq!(metas.len(), 2);
        let metas = store.list(1, 200).await.unwrap();
        assert_eq!(metas.len(), 1);
        let metas = store.list(2, 100).await.unwrap();
        assert!(metas.is_empty());
    }

    #[tokio::test]
    async fn test_load_zip_by_package_id() {
        let store = InMemorySkillPackageStore::new();
        let rec = store.install(1, 100, sample_pkg("a"), false).await.unwrap();
        let pkg = store.load_zip(rec.record().id).await.unwrap().unwrap();
        assert_eq!(pkg.skill_code, "a");
        assert_eq!(pkg.content_zip, vec![1, 2, 3]);
    }
}
