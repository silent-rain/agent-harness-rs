//! 本地技能加载器
//!
//! 从本地文件系统扫描 skill 目录，将每个子目录中的 `SKILL.md` 转换为 [`SkillConfig`]。
//!
//! ## 设计
//!
//! 加载器仅做"被动扫描"：每次 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
//! DB 模式下的内容加载由 platform 层 `DbSkillLoader` 单独负责，与 installer 完全解耦。
//!
//! 提供的主要内容：
//! - [`LocalSkillLoader`] - 本地文件系统技能加载器

use std::path::{Path, PathBuf};

use tokio::fs;
use tracing::{debug, info, warn};

use crate::{
    Error,
    constant::{DEFAULT_SKILLS_DIR, DEFAULT_WORKSPACE_DIR, SKILL_ENTRY_FILE},
    skill::SkillManifest,
    types::skill::SkillConfig,
};

/// 本地技能加载器
///
/// 扫描指定目录下的所有直接子目录，解析每个目录中的 `SKILL.md`，
/// 将其转换为 [`SkillConfig`] 列表。
///
/// 设计上保持"被动"：仅在 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
/// 加载结果由调用方通过
/// [`AgentBuilder::add_skills`](crate::runtime::AgentBuilder::add_skills) 注入到 bundle 中统一合并。
///
/// ## 使用示例
///
/// ```rust,no_run
/// use agent_runtime::skill::LocalSkillLoader;
///
/// # async fn example() -> anyhow::Result<()> {
/// let loader = LocalSkillLoader::new(".codebuddy/skills");
/// let skills = loader.load().await?;
///
/// // 通过 AgentBuilder::add_skills 注入到 bundle
/// // builder.add_skills(skills);
/// # Ok(())
/// # }
/// ```
pub struct LocalSkillLoader {
    /// Skills 根目录路径
    skills_root: PathBuf,
}

impl Default for LocalSkillLoader {
    fn default() -> Self {
        Self {
            skills_root: Path::new(DEFAULT_WORKSPACE_DIR).join(DEFAULT_SKILLS_DIR),
        }
    }
}

impl LocalSkillLoader {
    /// 创建本地技能加载器
    ///
    /// # Parameters
    ///
    /// * `skills_root` - skills 根目录路径（如 `.codebuddy/skills`）
    pub fn new(skills_root: impl Into<PathBuf>) -> Self {
        Self {
            skills_root: skills_root.into(),
        }
    }

    /// 切换工作空间根目录
    ///
    /// 将 `skills_root` 重新设置为 `workspace_dir/skills`。
    pub fn with_workspace_dir(mut self, workspace_dir: impl Into<PathBuf>) -> Self {
        self.skills_root = workspace_dir.into().join(DEFAULT_SKILLS_DIR);
        self
    }

    /// 获取 skills 根目录
    pub fn skills_root(&self) -> &Path {
        &self.skills_root
    }

    /// 扫描本地 skills 根目录并加载所有技能
    ///
    /// 加载流程：
    /// 1. 扫描 `skills_root` 下的所有直接子目录
    /// 2. 跳过临时/隐藏目录（以 `.` 开头、`.new` / `.old` 结尾）
    /// 3. 解析每个目录中的 `SKILL.md` 为 [`SkillConfig`]
    ///
    /// # Returns
    ///
    /// 返回所有成功加载的 [`SkillConfig`] 列表
    ///
    /// # Errors
    ///
    /// 当 `skills_root` 不可读时返回 IO 错误；单个目录解析失败时跳过并记录警告
    pub async fn load(&self) -> Result<Vec<SkillConfig>, Error> {
        if !self.skills_root.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&self.skills_root).await?;
        let mut skill_dirs: Vec<PathBuf> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') || name.ends_with(".new") || name.ends_with(".old") {
                continue;
            }
            skill_dirs.push(path);
        }
        skill_dirs.sort();

        let mut skills: Vec<SkillConfig> = Vec::with_capacity(skill_dirs.len());
        for dir in skill_dirs {
            match self.scan_skill_at(&dir).await {
                Ok(Some(config)) => {
                    info!(
                        skill_code = %config.skill_code,
                        "Loaded skill from local directory"
                    );
                    skills.push(config);
                }
                Ok(None) => {
                    debug!(
                        dir = %dir.display(),
                        "Directory does not contain a valid SKILL.md, skipped"
                    );
                }
                Err(e) => {
                    warn!(
                        dir = %dir.display(),
                        error = %e,
                        "Failed to load skill directory, skipped"
                    );
                }
            }
        }

        info!(
            skills_root = %self.skills_root.display(),
            total_loaded = skills.len(),
            "Skill directory scan completed"
        );

        Ok(skills)
    }

    /// 扫描单个目录，解析 `SKILL.md` 为 [`SkillConfig`]
    /// 扫描指定目录并解析为 [`SkillConfig`]
    ///
    /// 读取 `dir/SKILL.md`，解析 frontmatter 中的 `name` 和 `description`。
    /// 目录名作为 `skill_code`。
    ///
    /// # Parameters
    ///
    /// * `dir` - 技能子目录路径（如 `.codebuddy/skills/code-review`）
    ///
    /// # Returns
    ///
    /// - `Ok(Some(config))` - 解析成功
    /// - `Ok(None)` - 目录不含有效的 `SKILL.md`
    ///
    /// # Errors
    ///
    /// 文件读取失败时返回 IO 错误
    pub async fn scan_skill_at(&self, dir: &Path) -> Result<Option<SkillConfig>, Error> {
        let skill_code = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if skill_code.is_empty() {
            return Ok(None);
        }

        let entry_path = dir.join(SKILL_ENTRY_FILE);
        if !entry_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&entry_path).await?;
        let manifest = SkillManifest::parse(&content);

        let name = manifest.name.unwrap_or_else(|| {
            debug!(
                skill_code = %skill_code,
                "SKILL.md has no derivable name, falling back to directory name"
            );
            skill_code.clone()
        });

        let description = manifest.description.unwrap_or_else(|| {
            warn!(
                skill_code = %skill_code,
                "SKILL.md has no derivable description — LLM routing quality may degrade"
            );
            content.clone()
        });

        Ok(Some(SkillConfig {
            skill_code,
            name,
            description,
            source_hint: Some(entry_path.to_string_lossy().into_owned()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::TempDir;

    fn create_skill_dir(root: &Path, name: &str, content: &str) {
        let dir = root.join(name);
        std_fs::create_dir_all(&dir).unwrap();
        std_fs::write(dir.join(SKILL_ENTRY_FILE), content).unwrap();
    }

    #[tokio::test]
    async fn test_load_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let loader = LocalSkillLoader::new(tmp.path());
        let skills = loader.load().await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_load_nonexistent_directory() {
        let loader = LocalSkillLoader::new("/nonexistent/path");
        let skills = loader.load().await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_load_valid_skills() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(
            tmp.path(),
            "test-skill",
            "---\nname: test-skill\ndescription: A test skill\n---\n\n# Test\n\nBody content.",
        );

        let loader = LocalSkillLoader::new(tmp.path());
        let skills = loader.load().await.unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].skill_code, "test-skill");
        assert_eq!(skills[0].name, "test-skill");
    }

    #[tokio::test]
    async fn test_load_skips_dir_without_skill_md() {
        let tmp = TempDir::new().unwrap();
        std_fs::create_dir_all(tmp.path().join("no-skill")).unwrap();
        std_fs::write(tmp.path().join("no-skill/README.md"), "not a skill").unwrap();
        create_skill_dir(
            tmp.path(),
            "valid-skill",
            "---\nname: valid-skill\ndescription: Valid\n---\n\nContent",
        );

        let loader = LocalSkillLoader::new(tmp.path());
        let skills = loader.load().await.unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].skill_code, "valid-skill");
    }

    #[tokio::test]
    async fn test_load_skips_temp_directories() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(
            tmp.path(),
            "valid",
            "---\nname: valid\ndescription: ok\n---\n\nbody",
        );
        create_skill_dir(
            tmp.path(),
            "valid.new",
            "---\nname: valid.new\ndescription: ok\n---\n\nbody",
        );
        create_skill_dir(
            tmp.path(),
            "valid.old",
            "---\nname: valid.old\ndescription: ok\n---\n\nbody",
        );
        create_skill_dir(
            tmp.path(),
            ".cache",
            "---\nname: cache\ndescription: ok\n---\n\nbody",
        );

        let loader = LocalSkillLoader::new(tmp.path());
        let skills = loader.load().await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].skill_code, "valid");
    }
}
