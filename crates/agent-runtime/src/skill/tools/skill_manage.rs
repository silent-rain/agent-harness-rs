//! Skill 管理工具（`skill_manage`）
//!
//! 暴露给 LLM 的统一技能生命周期管理工具，取代旧的 `skill_install`。
//! 支持五个子命令：
//!
//! | action      | 说明                                     |
//! |-------------|------------------------------------------|
//! | `install`   | 安装一个新的 Skill                       |
//! | `update`    | 覆盖更新一个已存在的 Skill              |
//! | `uninstall` | 卸载（本地：删目录；线上：仅取消 link）  |
//! | `list`      | 列出当前已安装的 Skill                   |
//! | `query`     | 查询单个 Skill 的注册元数据              |
//!
//! ## 后端
//!
//! 工具实例化时绑定一个 [`SkillBackend`]：
//! - `SkillBackend::Local`：使用 [`LocalDirInstaller`]，写本地目录
//! - `SkillBackend::Db`：使用 [`DbInstaller`]，需要绑定 `(user_id, agent_id)`
//!
//! 这样可以避免把 `SkillPackageStore` 透传到每个 agent —— 全局共享一个
//! installer，构建主 agent 时按需注入工具实例。
//!
//! ## 安全策略
//!
//! - 仅支持本地路径来源（远端 URL 由外层 fetcher 预先转换为字节流后再传入）
//! - 通过来源前缀白名单 [`SkillManagementTool::with_allowed_path_prefixes`] 限制可安装位置
//!   为空时表示禁用 install/update（LLM 调用直接拒绝）

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::skill::installer::{
    DbInstaller, InstallOutcome, LocalDirInstaller, SkillPackageMeta, SkillSource,
};
use crate::tool::{ContextualTool, ToolCallCtx};

/// 工具后端：本地目录 or 数据库
///
/// 工具实例只能绑定其中一种后端，避免运行期判别开销。
/// `user_id` / `agent_id` 不再存储在后端枚举中，而是在调用时从
/// [`ToolCallCtx`] 动态获取，以支持多租户场景下同一工具实例被不同
/// 用户/智能体复用。
#[derive(Clone)]
pub enum SkillBackend {
    /// 本地模式：直接读写 `<skills_root>/<skill_code>/`
    Local(Arc<LocalDirInstaller>),
    /// 线上模式：通过 [`crate::skill::installer::SkillPackageStore`] 写 DB。
    /// `user_id` / `agent_id` 在调用时从 [`ToolCallCtx`] 获取。
    Db(Arc<DbInstaller>),
}

/// Skill 管理工具
///
/// LLM 通过 function calling 调用此工具完成 Skill 生命周期管理。
pub struct SkillManagementTool {
    backend: SkillBackend,
    /// 路径前缀白名单（绝对路径前缀）
    ///
    /// LLM 提交的 `source` 会被规范化后检查是否处于这些前缀之一。
    /// 为空表示禁用 install/update，所有调用直接拒绝（防 prompt-injection）。
    allowed_path_prefixes: Vec<PathBuf>,
}

impl SkillManagementTool {
    /// 创建本地模式工具
    pub fn local(installer: Arc<LocalDirInstaller>) -> Self {
        Self {
            backend: SkillBackend::Local(installer),
            allowed_path_prefixes: Vec::new(),
        }
    }

    /// 创建线上模式工具
    ///
    /// `user_id` / `agent_id` 不在构造时绑定，而是在每次调用时从
    /// [`ToolCallCtx`] 动态获取，支持同一工具实例被多个智能体复用。
    ///
    /// # Parameters
    ///
    /// * `installer` - 全局共享的 [`DbInstaller`]
    pub fn db(installer: Arc<DbInstaller>) -> Self {
        Self {
            backend: SkillBackend::Db(installer),
            allowed_path_prefixes: Vec::new(),
        }
    }

    /// 设置允许的路径前缀白名单
    pub fn with_allowed_path_prefixes(mut self, prefixes: Vec<PathBuf>) -> Self {
        self.allowed_path_prefixes = prefixes;
        self
    }

    /// 校验给定路径是否落在白名单内
    fn is_path_allowed(&self, path: &Path) -> bool {
        if self.allowed_path_prefixes.is_empty() {
            return false;
        }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.allowed_path_prefixes.iter().any(|prefix| {
            let prefix_canonical = prefix.canonicalize().unwrap_or_else(|_| prefix.clone());
            canonical.starts_with(prefix_canonical)
        })
    }

    /// 解析并校验 source 字符串
    fn parse_source(&self, source: &str) -> Result<SkillSource, Error> {
        let parsed = SkillSource::parse(source).map_err(|e| Error::ToolExecution {
            tool_name: <Self as ContextualTool>::NAME.to_string(),
            message: format!("解析安装来源失败: {e}"),
        })?;
        if let SkillSource::LocalDir(path) = &parsed
            && !self.is_path_allowed(path)
        {
            return Err(Error::ToolExecution {
                tool_name: <Self as ContextualTool>::NAME.to_string(),
                message: format!(
                    "安装路径 '{}' 不在受信任前缀白名单内（白名单为空表示工具未启用）",
                    path.display()
                ),
            });
        }
        Ok(parsed)
    }
}

/// `skill_manage` 工具参数（按 action 分支）
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SkillManageArgs {
    /// 安装一个新的 Skill
    Install {
        /// 安装来源（本地路径或 file:// URL）
        source: String,
        /// 期望的 skill_code（可选，缺省时从 SKILL.md 推断）
        #[serde(default)]
        skill_code: Option<String>,
    },
    /// 覆盖更新已存在的 Skill
    Update {
        source: String,
        #[serde(default)]
        skill_code: Option<String>,
    },
    /// 卸载 Skill
    Uninstall {
        /// 要卸载的 skill_code
        skill_code: String,
    },
    /// 列出当前已安装的 Skill
    List {},
    /// 查询单个 Skill 的注册元数据
    Query { skill_code: String },
}

/// `skill_manage` 工具输出
#[derive(Debug, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum SkillManageOutput {
    /// install / update 的结果
    Mutation {
        /// `installed` / `updated` / `skipped`
        outcome: &'static str,
        /// 受影响的 Skill 元数据
        meta: SkillPackageMeta,
    },
    /// uninstall 的结果
    Uninstalled {
        skill_code: String,
        /// 是否实际删除（false 表示原本就不存在）
        removed: bool,
    },
    /// list 的结果
    List { skills: Vec<SkillPackageMeta> },
    /// query 的结果
    Query {
        skill_code: String,
        /// 命中时返回元数据，未注册时为 None
        meta: Option<SkillPackageMeta>,
    },
}

#[async_trait]
impl ContextualTool for SkillManagementTool {
    const NAME: &'static str = "skill_manage";

    type Error = Error;
    type Args = SkillManageArgs;
    type Output = SkillManageOutput;

    fn description(&self) -> String {
        "管理当前智能体的 Skill：安装、更新、卸载、列表、查询。\
                参数 `action` 取值 `install` | `update` | `uninstall` | `list` | `query`。\
                `source` 必须是位于受信任路径前缀下的本地目录或 file:// URL，\
                远端来源由外层流程预先下载后再调用。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["install", "update", "uninstall", "list", "query"],
                    "description": "操作类型"
                },
                "source": {
                    "type": "string",
                    "description": "install/update 时使用：本地路径或 file:// URL"
                },
                "skill_code": {
                    "type": "string",
                    "description": "uninstall/query 必填；install/update 可选（缺省时从 SKILL.md 推断）"
                }
            },
            "required": ["action"]
        })
    }

    async fn call(
        &self,
        ctx: ToolCallCtx<'_>,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let user_id: i64 = ctx
            .user_id
            .parse()
            .map_err(|_| Error::Internal(format!("invalid user_id: {}", ctx.user_id)))?;
        let agent_id = ctx.agent_id;
        match args {
            SkillManageArgs::Install { source, skill_code } => {
                let parsed = self.parse_source(&source)?;
                let outcome = self
                    .dispatch_install(user_id, agent_id, skill_code, parsed)
                    .await?;
                Ok(mutation_output(outcome))
            }
            SkillManageArgs::Update { source, skill_code } => {
                let parsed = self.parse_source(&source)?;
                let outcome = self
                    .dispatch_update(user_id, agent_id, skill_code, parsed)
                    .await?;
                Ok(mutation_output(outcome))
            }
            SkillManageArgs::Uninstall { skill_code } => {
                let removed = self
                    .dispatch_uninstall(user_id, agent_id, &skill_code)
                    .await?;
                Ok(SkillManageOutput::Uninstalled {
                    skill_code,
                    removed,
                })
            }
            SkillManageArgs::List {} => {
                let skills = self.dispatch_list(user_id, agent_id).await?;
                Ok(SkillManageOutput::List { skills })
            }
            SkillManageArgs::Query { skill_code } => {
                let meta = self.dispatch_query(user_id, agent_id, &skill_code).await?;
                Ok(SkillManageOutput::Query { skill_code, meta })
            }
        }
    }
}

impl SkillManagementTool {
    async fn dispatch_install(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: Option<String>,
        source: SkillSource,
    ) -> Result<MutationKind, Error> {
        match &self.backend {
            SkillBackend::Local(installer) => {
                let meta = installer.install(skill_code, source).await?;
                Ok(MutationKind::Local("installed", meta))
            }
            SkillBackend::Db(installer) => {
                let outcome = installer
                    .install(user_id, agent_id, skill_code, source, user_id)
                    .await?;
                Ok(MutationKind::Db(outcome))
            }
        }
    }

    async fn dispatch_update(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: Option<String>,
        source: SkillSource,
    ) -> Result<MutationKind, Error> {
        match &self.backend {
            SkillBackend::Local(installer) => {
                let meta = installer.update(skill_code, source).await?;
                Ok(MutationKind::Local("updated", meta))
            }
            SkillBackend::Db(installer) => {
                let outcome = installer
                    .update(user_id, agent_id, skill_code, source, user_id)
                    .await?;
                Ok(MutationKind::Db(outcome))
            }
        }
    }

    async fn dispatch_uninstall(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
    ) -> Result<bool, Error> {
        match &self.backend {
            SkillBackend::Local(installer) => installer.uninstall(skill_code).await,
            SkillBackend::Db(installer) => installer
                .uninstall(user_id, agent_id, skill_code)
                .await
                .map_err(Error::Skill),
        }
    }

    async fn dispatch_list(
        &self,
        user_id: i64,
        agent_id: i64,
    ) -> Result<Vec<SkillPackageMeta>, Error> {
        match &self.backend {
            SkillBackend::Local(installer) => {
                let codes = installer.list_codes().await?;
                // 本地模式只返回 skill_code 列表 —— 用 SkillPackageMeta 装填最小信息
                Ok(codes
                    .into_iter()
                    .map(|code| SkillPackageMeta {
                        skill_code: code.clone(),
                        name: code,
                        description: String::new(),
                        version: String::new(),
                        source_type: 0,
                        source_url: String::new(),
                        content_sha256: String::new(),
                        size_bytes: 0,
                        status: 0,
                    })
                    .collect())
            }
            SkillBackend::Db(installer) => Ok(installer.list(user_id, agent_id).await?),
        }
    }

    async fn dispatch_query(
        &self,
        user_id: i64,
        agent_id: i64,
        skill_code: &str,
    ) -> Result<Option<SkillPackageMeta>, Error> {
        match &self.backend {
            SkillBackend::Local(installer) => {
                let codes = installer.list_codes().await?;
                Ok(codes
                    .iter()
                    .find(|c| c.as_str() == skill_code)
                    .map(|c| SkillPackageMeta {
                        skill_code: c.clone(),
                        name: c.clone(),
                        description: String::new(),
                        version: String::new(),
                        source_type: 0,
                        source_url: String::new(),
                        content_sha256: String::new(),
                        size_bytes: 0,
                        status: 0,
                    }))
            }
            SkillBackend::Db(installer) => {
                let result = installer.check(user_id, agent_id, skill_code).await?;
                Ok(result.package.map(|r| r.meta))
            }
        }
    }
}

/// 中间结果：dispatch_install/update 返回的两种形态
enum MutationKind {
    /// 本地：固定 outcome + meta
    Local(&'static str, SkillPackageMeta),
    /// 线上：完整的 InstallOutcome
    Db(InstallOutcome),
}

fn mutation_output(kind: MutationKind) -> SkillManageOutput {
    match kind {
        MutationKind::Local(outcome, meta) => SkillManageOutput::Mutation { outcome, meta },
        MutationKind::Db(outcome) => match outcome {
            InstallOutcome::Installed(meta) => SkillManageOutput::Mutation {
                outcome: "installed",
                meta,
            },
            InstallOutcome::Updated(meta) => SkillManageOutput::Mutation {
                outcome: "updated",
                meta,
            },
            InstallOutcome::Skipped(meta) => SkillManageOutput::Mutation {
                outcome: "skipped",
                meta,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constant::SKILL_ENTRY_FILE;
    use crate::session::AgentStreamEvent;
    use crate::skill::installer::InMemorySkillPackageStore;
    use crate::skill::installer::SkillPackageStore;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    fn write_skill_dir(root: &Path, code: &str) {
        let dir = root.join(code);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(SKILL_ENTRY_FILE),
            "---\nname: T\nversion: \"0.1.0\"\ndescription: d\n---\n\nbody",
        )
        .unwrap();
    }

    /// Build a dummy [`ToolCallCtx`] with the given `user_id` / `agent_id`.
    fn make_ctx<'a>(
        tx: &'a mpsc::Sender<AgentStreamEvent>,
        user_id: &'a str,
        agent_id: i64,
    ) -> ToolCallCtx<'a> {
        ToolCallCtx {
            run_id: "test-run",
            step: 0,
            session_id: "test-session",
            events: tx,
            user_id,
            agent_id,
        }
    }

    #[tokio::test]
    async fn test_local_install_then_list() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "s");
        let skills_root = TempDir::new().unwrap();

        let local = Arc::new(LocalDirInstaller::new(skills_root.path()));
        let tool = SkillManagementTool::local(local)
            .with_allowed_path_prefixes(vec![staging.path().to_path_buf()]);

        let (tx, _rx) = mpsc::channel(8);
        let out = ContextualTool::call(
            &tool,
            make_ctx(&tx, "0", 0),
            SkillManageArgs::Install {
                source: staging.path().join("s").to_string_lossy().into_owned(),
                skill_code: None,
            },
        )
        .await
        .unwrap();
        match out {
            SkillManageOutput::Mutation { outcome, meta } => {
                assert_eq!(outcome, "installed");
                assert_eq!(meta.skill_code, "s");
            }
            _ => panic!("unexpected output"),
        }

        let listed = ContextualTool::call(&tool, make_ctx(&tx, "0", 0), SkillManageArgs::List {})
            .await
            .unwrap();
        match listed {
            SkillManageOutput::List { skills } => {
                assert_eq!(skills.len(), 1);
                assert_eq!(skills[0].skill_code, "s");
            }
            _ => panic!("unexpected output"),
        }
    }

    #[tokio::test]
    async fn test_local_rejects_when_path_not_in_allowlist() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "s");
        let skills_root = TempDir::new().unwrap();

        let local = Arc::new(LocalDirInstaller::new(skills_root.path()));
        let tool = SkillManagementTool::local(local);
        let (tx, _rx) = mpsc::channel(8);
        let err = ContextualTool::call(
            &tool,
            make_ctx(&tx, "0", 0),
            SkillManageArgs::Install {
                source: staging.path().join("s").to_string_lossy().into_owned(),
                skill_code: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::ToolExecution { .. }));
    }

    #[tokio::test]
    async fn test_db_install_skipped_on_second_call() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "s");

        let store: Arc<dyn SkillPackageStore> = Arc::new(InMemorySkillPackageStore::new());
        let installer = Arc::new(DbInstaller::new(store));
        let tool = SkillManagementTool::db(installer)
            .with_allowed_path_prefixes(vec![staging.path().to_path_buf()]);

        let (tx, _rx) = mpsc::channel(8);
        ContextualTool::call(
            &tool,
            make_ctx(&tx, "1", 100),
            SkillManageArgs::Install {
                source: staging.path().join("s").to_string_lossy().into_owned(),
                skill_code: None,
            },
        )
        .await
        .unwrap();
        let out = ContextualTool::call(
            &tool,
            make_ctx(&tx, "1", 100),
            SkillManageArgs::Install {
                source: staging.path().join("s").to_string_lossy().into_owned(),
                skill_code: None,
            },
        )
        .await
        .unwrap();
        match out {
            SkillManageOutput::Mutation { outcome, .. } => assert_eq!(outcome, "skipped"),
            _ => panic!("unexpected output"),
        }
    }

    #[tokio::test]
    async fn test_db_uninstall_only_unlinks() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "s");

        let store: Arc<dyn SkillPackageStore> = Arc::new(InMemorySkillPackageStore::new());
        let installer = Arc::new(DbInstaller::new(store.clone()));
        let tool = SkillManagementTool::db(installer)
            .with_allowed_path_prefixes(vec![staging.path().to_path_buf()]);

        let (tx, _rx) = mpsc::channel(8);
        ContextualTool::call(
            &tool,
            make_ctx(&tx, "1", 100),
            SkillManageArgs::Install {
                source: staging.path().join("s").to_string_lossy().into_owned(),
                skill_code: None,
            },
        )
        .await
        .unwrap();
        let out = ContextualTool::call(
            &tool,
            make_ctx(&tx, "1", 100),
            SkillManageArgs::Uninstall {
                skill_code: "s".to_string(),
            },
        )
        .await
        .unwrap();
        match out {
            SkillManageOutput::Uninstalled { removed, .. } => assert!(removed),
            _ => panic!("unexpected output"),
        }
        // 包仍在
        let result = store.check(1, 100, "s").await.unwrap();
        assert!(result.is_registered());
    }
}
