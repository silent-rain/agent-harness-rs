//! 本地目录安装器
//!
//! 将一个 [`SkillSource`] 写入 `<skills_root>/<skill_code>/` 目录，
//! 与 [`crate::skill::LocalSkillLoader`] 的扫描路径完全对齐。
//!
//! ## 设计要点
//!
//! - 不依赖任何外部存储（不同于 [`crate::skill::installer::DbInstaller`]）
//! - install / update 通过"先写临时目录 → 再 rename"实现原子覆盖
//! - uninstall 直接 `remove_dir_all(<skills_root>/<skill_code>)`
//! - 安装来源支持 [`SkillSource::LocalDir`]（拷贝目录）与 [`SkillSource::InlineZip`]
//!   （解压字节流），由 [`SkillSource`] 协议统一接管

use std::path::{Path, PathBuf};

use tokio::fs;
use tracing::{debug, info, warn};

use super::source::SkillSource;
use super::types::{SkillPackage, SkillPackageMeta};
use super::zip_codec::{
    pack_dir_to_zip, read_skill_md_from_zip, sha256_hex, unpack_zip_to_dir, validate_skill_zip,
};
use crate::Error;
use crate::constant::{
    DEFAULT_SKILL_PACKAGE_MAX_BYTES, DEFAULT_SKILLS_DIR, DEFAULT_WORKSPACE_DIR, SKILL_ENTRY_FILE,
};
use crate::skill::SkillManifest;

/// 本地模式 Skill 安装来源类型 → DB 字段映射（保留以便上层共用语义）
const SOURCE_TYPE_AGENT_INSTALLED: i16 = 1;

/// 本地目录安装器
///
/// 把 [`SkillSource`] 落地到 `<skills_root>/<skill_code>/` 子目录，
/// 由 [`crate::skill::LocalSkillLoader`] 在下一次扫描时自然加载。
pub struct LocalDirInstaller {
    skills_root: PathBuf,
    max_bytes: u64,
}

impl Default for LocalDirInstaller {
    fn default() -> Self {
        Self {
            skills_root: Path::new(DEFAULT_WORKSPACE_DIR).join(DEFAULT_SKILLS_DIR),
            max_bytes: DEFAULT_SKILL_PACKAGE_MAX_BYTES,
        }
    }
}

impl LocalDirInstaller {
    /// 创建本地安装器
    ///
    /// # Parameters
    ///
    /// * `skills_root` - skills 根目录（如 `.codebuddy/skills`）
    pub fn new(skills_root: impl Into<PathBuf>) -> Self {
        Self {
            skills_root: skills_root.into(),
            max_bytes: DEFAULT_SKILL_PACKAGE_MAX_BYTES,
        }
    }

    /// 自定义包大小上限
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// 获取 skills 根目录
    pub fn skills_root(&self) -> &Path {
        &self.skills_root
    }

    /// 安装：将 source 落地到 `<skills_root>/<skill_code>/`
    ///
    /// 已存在同名目录时报错（请改用 [`Self::update`]）。
    ///
    /// # Parameters
    ///
    /// * `skill_code` - 期望的技能编码；为 `None` 时从 SKILL.md / 目录名推断
    /// * `source` - 安装来源
    pub async fn install(
        &self,
        skill_code: Option<String>,
        source: SkillSource,
    ) -> Result<SkillPackageMeta, Error> {
        let prepared = self.prepare(skill_code, source).await?;
        let dest = self.skills_root.join(&prepared.skill_code);
        if dest.exists() {
            return Err(Error::Internal(format!(
                "skill already installed at '{}', use update instead",
                dest.display()
            )));
        }
        self.materialize(&prepared, &dest).await?;
        info!(
            skill_code = %prepared.skill_code,
            dest = %dest.display(),
            "Skill package installed to local dir"
        );
        Ok(prepared.into_meta())
    }

    /// 更新：原子覆盖 `<skills_root>/<skill_code>/`
    ///
    /// 实现方式：
    /// 1. 生成新内容到 `<skills_root>/<skill_code>.new/`
    /// 2. `mv <skill_code>` → `<skill_code>.old`
    /// 3. `mv <skill_code>.new` → `<skill_code>`
    /// 4. 删除 `.old`
    ///
    /// 若步骤 2/3 失败会尽力回滚。
    pub async fn update(
        &self,
        skill_code: Option<String>,
        source: SkillSource,
    ) -> Result<SkillPackageMeta, Error> {
        let prepared = self.prepare(skill_code, source).await?;
        let dest = self.skills_root.join(&prepared.skill_code);
        let staging = self
            .skills_root
            .join(format!("{}.new", prepared.skill_code));
        let backup = self
            .skills_root
            .join(format!("{}.old", prepared.skill_code));

        // 清理可能残留的临时目录
        let _ = fs::remove_dir_all(&staging).await;
        let _ = fs::remove_dir_all(&backup).await;

        self.materialize(&prepared, &staging).await?;

        if dest.exists() {
            fs::rename(&dest, &backup).await.map_err(|e| {
                Error::Internal(format!("failed to backup existing skill dir: {e}"))
            })?;
        }
        if let Err(e) = fs::rename(&staging, &dest).await {
            // 回滚：把备份目录还原
            if backup.exists() {
                let _ = fs::rename(&backup, &dest).await;
            }
            let _ = fs::remove_dir_all(&staging).await;
            return Err(Error::Internal(format!("failed to swap skill dir: {e}")));
        }
        if backup.exists() {
            let _ = fs::remove_dir_all(&backup).await;
        }
        info!(
            skill_code = %prepared.skill_code,
            dest = %dest.display(),
            "Skill package updated in local dir"
        );
        Ok(prepared.into_meta())
    }

    /// 卸载：删除 `<skills_root>/<skill_code>/`
    ///
    /// # Returns
    ///
    /// 是否实际删除（true 表示原本存在）
    pub async fn uninstall(&self, skill_code: &str) -> Result<bool, Error> {
        let dest = self.skills_root.join(skill_code);
        if !dest.exists() {
            warn!(skill_code = %skill_code, "Skill dir not found, nothing to uninstall");
            return Ok(false);
        }
        fs::remove_dir_all(&dest)
            .await
            .map_err(|e| Error::Internal(format!("failed to remove skill dir: {e}")))?;
        info!(skill_code = %skill_code, "Skill package uninstalled from local dir");
        Ok(true)
    }

    /// 列出本地已安装的 skill_code 列表（不读取 SKILL.md 内容）
    pub async fn list_codes(&self) -> Result<Vec<String>, Error> {
        if !self.skills_root.exists() {
            return Ok(Vec::new());
        }
        let mut entries = fs::read_dir(&self.skills_root)
            .await
            .map_err(|e| Error::Internal(format!("failed to read skills_root: {e}")))?;
        let mut codes = Vec::new();
        while let Some(entry) = entries.next_entry().await.transpose() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    debug!(error = %e, "skip unreadable entry");
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // 跳过临时目录（".new" / ".old"）
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') || name.ends_with(".new") || name.ends_with(".old") {
                continue;
            }
            if !path.join(SKILL_ENTRY_FILE).exists() {
                continue;
            }
            codes.push(name.to_string());
        }
        codes.sort();
        Ok(codes)
    }

    /// 第一阶段：把 source 转为 zip 字节流并解析出 manifest，得到所有元数据
    ///
    /// 暴露给 sibling [`super::db::DbInstaller`] 复用打包/校验流程。
    pub(super) async fn prepare(
        &self,
        skill_code: Option<String>,
        source: SkillSource,
    ) -> Result<PreparedSkill, Error> {
        let origin = source.display_origin();
        let zip_bytes = self.acquire_zip_bytes(source).await?;

        if (zip_bytes.len() as u64) > self.max_bytes {
            return Err(Error::Internal(format!(
                "skill package exceeds size limit: {} bytes > {} bytes",
                zip_bytes.len(),
                self.max_bytes
            )));
        }

        let skill_md_path = validate_skill_zip(&zip_bytes)?;
        let skill_md_content = read_skill_md_from_zip(&zip_bytes, &skill_md_path)?;
        let manifest = SkillManifest::parse(&skill_md_content);

        let resolved_code = resolve_skill_code(skill_code, &manifest, &skill_md_path)?;
        let name = manifest
            .name
            .clone()
            .unwrap_or_else(|| resolved_code.clone());
        let description = manifest.description.clone().unwrap_or_default();
        let version = extract_version(&manifest);
        let sha256 = sha256_hex(&zip_bytes);
        let size_bytes = zip_bytes.len() as i64;

        Ok(PreparedSkill {
            skill_code: resolved_code,
            name,
            description,
            version,
            origin,
            sha256,
            size_bytes,
            zip_bytes,
        })
    }

    /// 第二阶段：把 zip 字节流解压到指定目标目录
    async fn materialize(&self, prepared: &PreparedSkill, dest: &Path) -> Result<(), Error> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Internal(format!("failed to create parent dir: {e}")))?;
        }
        unpack_zip_to_dir(prepared.zip_bytes.clone(), dest.to_path_buf(), true).await
    }

    /// 根据来源获取 zip 字节流
    async fn acquire_zip_bytes(&self, source: SkillSource) -> Result<Vec<u8>, Error> {
        match source {
            SkillSource::LocalDir(path) => {
                let top = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());
                pack_dir_to_zip(&path, top.as_deref()).await
            }
            SkillSource::InlineZip { bytes, .. } => Ok(bytes),
        }
    }
}

/// 中间结构：完成 zip 校验 / manifest 解析后的 prepared 包数据
pub(super) struct PreparedSkill {
    pub(super) skill_code: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) version: String,
    pub(super) origin: String,
    pub(super) sha256: String,
    pub(super) size_bytes: i64,
    pub(super) zip_bytes: Vec<u8>,
}

impl PreparedSkill {
    pub(super) fn into_meta(self) -> SkillPackageMeta {
        SkillPackageMeta {
            skill_code: self.skill_code,
            name: self.name,
            description: self.description,
            version: self.version,
            source_type: SOURCE_TYPE_AGENT_INSTALLED,
            source_url: self.origin,
            content_sha256: self.sha256,
            size_bytes: self.size_bytes,
            status: 0,
        }
    }

    /// 转换为完整 [`SkillPackage`]（线上模式由 DbInstaller 调用）
    pub(super) fn into_package(self, installed_by: i64) -> SkillPackage {
        SkillPackage {
            skill_code: self.skill_code,
            name: self.name,
            description: self.description,
            version: self.version,
            source_type: SOURCE_TYPE_AGENT_INSTALLED,
            source_url: self.origin,
            content_zip: self.zip_bytes,
            content_sha256: self.sha256,
            size_bytes: self.size_bytes,
            installed_by,
            status: 0,
        }
    }
}

/// 推断最终的 skill_code
///
/// 优先级：
/// 1. 显式传入的 `requested`
/// 2. zip 内一级目录名（如 `repo-name/SKILL.md` → `repo-name`）
/// 3. SKILL.md 中的 `name` 字段（slug）
fn resolve_skill_code(
    requested: Option<String>,
    manifest: &SkillManifest,
    skill_md_path: &str,
) -> Result<String, Error> {
    if let Some(code) = requested.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    }) {
        return Ok(code);
    }
    if let Some((top, _)) = skill_md_path.split_once('/')
        && !top.is_empty()
    {
        return Ok(top.to_string());
    }
    if let Some(name) = &manifest.name {
        let slug = slugify(name);
        if !slug.is_empty() {
            return Ok(slug);
        }
    }
    Err(Error::Internal(
        "unable to resolve skill_code: no explicit code, no top-level dir, no manifest name"
            .to_string(),
    ))
}

/// 从 manifest.extra 中提取 `version` 字段，缺省为 `0.0.0`
fn extract_version(manifest: &SkillManifest) -> String {
    manifest
        .extra
        .get("version")
        .and_then(|v| v.clone().into_string().ok())
        .unwrap_or_else(|| "0.0.0".to_string())
}

/// 极简 slug：保留字母数字与 `-`/`_`，其余替换为 `-`
fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
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
    async fn test_install_local_dir() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "demo-skill");
        let skills_root = TempDir::new().unwrap();

        let installer = LocalDirInstaller::new(skills_root.path());
        let meta = installer
            .install(
                Some("demo-skill".to_string()),
                SkillSource::LocalDir(staging.path().join("demo-skill")),
            )
            .await
            .unwrap();
        assert_eq!(meta.skill_code, "demo-skill");
        assert_eq!(meta.version, "1.2.3");

        // installed dir contains SKILL.md
        let dest = skills_root.path().join("demo-skill").join(SKILL_ENTRY_FILE);
        assert!(dest.exists());

        let codes = installer.list_codes().await.unwrap();
        assert_eq!(codes, vec!["demo-skill".to_string()]);
    }

    #[tokio::test]
    async fn test_install_rejects_when_existing() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "x");
        let skills_root = TempDir::new().unwrap();

        let installer = LocalDirInstaller::new(skills_root.path());
        installer
            .install(
                Some("x".to_string()),
                SkillSource::LocalDir(staging.path().join("x")),
            )
            .await
            .unwrap();
        let err = installer
            .install(
                Some("x".to_string()),
                SkillSource::LocalDir(staging.path().join("x")),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Internal(_)));
    }

    #[tokio::test]
    async fn test_update_replaces_directory() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "x");
        let skills_root = TempDir::new().unwrap();

        let installer = LocalDirInstaller::new(skills_root.path());
        installer
            .install(
                Some("x".to_string()),
                SkillSource::LocalDir(staging.path().join("x")),
            )
            .await
            .unwrap();

        // 修改源目录内容
        std::fs::write(
            staging.path().join("x").join(SKILL_ENTRY_FILE),
            "---\nname: My Skill\nversion: \"2.0.0\"\ndescription: new\n---\n\nbody",
        )
        .unwrap();
        let meta = installer
            .update(
                Some("x".to_string()),
                SkillSource::LocalDir(staging.path().join("x")),
            )
            .await
            .unwrap();
        assert_eq!(meta.version, "2.0.0");

        let installed =
            std::fs::read_to_string(skills_root.path().join("x").join(SKILL_ENTRY_FILE)).unwrap();
        assert!(installed.contains("2.0.0"));
    }

    #[tokio::test]
    async fn test_uninstall_removes_directory() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "x");
        let skills_root = TempDir::new().unwrap();

        let installer = LocalDirInstaller::new(skills_root.path());
        installer
            .install(
                Some("x".to_string()),
                SkillSource::LocalDir(staging.path().join("x")),
            )
            .await
            .unwrap();
        assert!(installer.uninstall("x").await.unwrap());
        assert!(!installer.uninstall("x").await.unwrap());
        assert!(!skills_root.path().join("x").exists());
    }

    #[tokio::test]
    async fn test_install_rejects_oversized_package() {
        let staging = TempDir::new().unwrap();
        write_skill_dir(staging.path(), "big");
        let skills_root = TempDir::new().unwrap();

        let installer = LocalDirInstaller::new(skills_root.path()).with_max_bytes(1);
        let err = installer
            .install(
                Some("big".to_string()),
                SkillSource::LocalDir(staging.path().join("big")),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Internal(_)));
    }
}
