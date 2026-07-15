//! 技能按需加载工具
//!
//! 当 LLM 判断用户需求匹配某个技能时，通过此工具按需加载
//! 该技能的完整 `SKILL.md` 正文内容及目录内可用资源清单。
//!
//! ## 设计说明
//!
//! 属于"三阶段 Skill 加载"中的**第二阶段**：
//! 1. 启动时：仅将 skill 的 `name` + `description` 摘要注入 System Prompt
//! 2. **命中时**：LLM 调用 `skill_load` 工具加载完整指令（本工具）
//! 3. 执行时：LLM 按 SKILL.md 指令自主调用 `read_file`、`run_command` 等工具

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::{
    constant::{DEFAULT_SKILLS_DIR, DEFAULT_WORKSPACE_DIR, SKILL_ENTRY_FILE},
    error::Error,
    skill::SkillManifest,
};

/// 技能按需加载工具
///
/// 根据 `skill_code` 定位本地 skill 目录，读取 `SKILL.md` 完整正文，
/// 并列出目录内可用的资源文件（`references/`、`assets/` 等）。
pub struct SkillLoadTool {
    /// Skills 根目录路径（如 `.codebuddy/skills`）
    skills_root: PathBuf,
    /// 可用的 skill_code 集合（白名单）
    available_skills: HashSet<String>,
}

impl Default for SkillLoadTool {
    fn default() -> Self {
        Self {
            skills_root: Path::new(DEFAULT_WORKSPACE_DIR).join(DEFAULT_SKILLS_DIR),
            available_skills: HashSet::new(),
        }
    }
}

impl SkillLoadTool {
    /// 创建技能加载工具
    ///
    /// # Parameters
    ///
    /// * `skills_root` - skills 根目录路径
    pub fn new(skills_root: impl Into<PathBuf>) -> Self {
        Self {
            skills_root: skills_root.into(),
            ..Default::default()
        }
    }

    /// 切换工作空间根目录
    ///
    /// 将 `skills_root` 重新设置为 `workspace_dir/skills`，用于在不同工作空间之间切换。
    ///
    /// # Parameters
    ///
    /// * `workspace_dir` - 工作空间根目录路径（如 `.codebuddy` 或自定义路径）
    ///
    /// # Returns
    ///
    /// 返回 `self` 以支持链式调用
    pub fn with_workspace_dir(mut self, workspace_dir: impl Into<PathBuf>) -> Self {
        self.skills_root = workspace_dir.into().join(DEFAULT_SKILLS_DIR);
        self
    }

    /// 设置可用的 skill_code 集合
    ///
    /// # Parameters
    ///
    /// * `available_skills` - 可用的 skill_code 列表（白名单）
    pub fn with_available_skills(mut self, available_skills: Vec<String>) -> Self {
        self.available_skills = available_skills.into_iter().collect();
        self
    }
}

/// 技能加载工具参数
#[derive(Debug, Deserialize)]
pub struct SkillLoadArgs {
    /// 技能编码（对应 skills 目录下的子目录名）
    skill_code: String,
}

/// 技能加载工具输出
#[derive(Debug, Serialize)]
pub struct SkillLoadOutput {
    /// 技能编码
    skill_code: String,
    /// 技能名称
    name: String,
    /// SKILL.md 完整正文内容
    content: String,
    /// 目录内可用的资源文件列表（相对于 skill 目录的路径）
    resource_files: Vec<String>,
}

impl Tool for SkillLoadTool {
    const NAME: &'static str = "skill_load";

    type Error = Error;
    type Args = SkillLoadArgs;
    type Output = SkillLoadOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "skill_load".to_string(),
            description: "加载指定技能的完整指令。当用户需求匹配某个技能时调用此工具，\
                获取该技能的完整操作指令和可用资源文件列表。\
                加载后可通过 skill_list_docs 查看文档列表，通过 skill_select_docs 读取具体文档内容。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill_code": {
                        "type": "string",
                        "description": "要加载的技能编码（即 Skills 列表中的技能标识）"
                    }
                },
                "required": ["skill_code"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let skill_code = &args.skill_code;

        // Validate skill_code is in the available set
        if !self.available_skills.contains(skill_code) {
            return Err(Error::ToolExecution {
                tool_name: "skill_load".to_string(),
                message: format!(
                    "技能 '{}' 不在可用列表中。可用技能: {}",
                    skill_code,
                    self.available_skills
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }

        let skill_dir = self.skills_root.join(skill_code);
        if !skill_dir.exists() {
            return Err(Error::ToolExecution {
                tool_name: "skill_load".to_string(),
                message: format!("技能 '{}' 的目录不存在", skill_code),
            });
        }

        let entry_path = skill_dir.join(SKILL_ENTRY_FILE);
        if !entry_path.exists() {
            return Err(Error::ToolExecution {
                tool_name: "skill_load".to_string(),
                message: format!("技能 '{}' 的 SKILL.md 文件不存在", skill_code),
            });
        }

        // Read SKILL.md
        let raw_content =
            fs::read_to_string(&entry_path)
                .await
                .map_err(|e| Error::ToolExecution {
                    tool_name: "skill_load".to_string(),
                    message: format!("读取技能文件失败: {} ({})", entry_path.display(), e),
                })?;

        let manifest = SkillManifest::parse(&raw_content);
        let body = SkillManifest::body(&raw_content);
        let name = manifest.name.unwrap_or_else(|| skill_code.clone());

        // Scan resource files in the skill directory (excluding SKILL.md itself)
        let resource_files = scan_resource_files(&skill_dir).await;

        Ok(SkillLoadOutput {
            skill_code: skill_code.clone(),
            name,
            content: body.to_string(),
            resource_files,
        })
    }
}

/// 递归扫描 skill 目录下的资源文件
pub(super) async fn scan_resource_files(skill_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if collect_files_recursive(skill_dir, skill_dir, &mut files)
        .await
        .is_err()
    {
        return files;
    }
    // Exclude SKILL.md entry file from resource list
    files.retain(|f| f != SKILL_ENTRY_FILE);
    files.sort();
    files
}

/// 递归收集目录下的所有文件路径
///
/// 返回相对于 `base` 目录的文件路径列表，包含所有文件（不做任何过滤）。
pub(super) async fn collect_files_recursive(
    base: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<(), std::io::Error> {
    let mut entries = fs::read_dir(current).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(collect_files_recursive(base, &path, files)).await?;
        } else if let Ok(relative) = path.strip_prefix(base) {
            let rel_str = relative.to_string_lossy().to_string();
            files.push(rel_str);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::TempDir;

    fn setup_skill(root: &Path, code: &str, skill_md: &str, resources: &[(&str, &str)]) {
        let skill_dir = root.join(code);
        std_fs::create_dir_all(&skill_dir).unwrap();
        std_fs::write(skill_dir.join(SKILL_ENTRY_FILE), skill_md).unwrap();
        for (path, content) in resources {
            let full_path = skill_dir.join(path);
            if let Some(parent) = full_path.parent() {
                std_fs::create_dir_all(parent).unwrap();
            }
            std_fs::write(full_path, content).unwrap();
        }
    }

    #[tokio::test]
    async fn test_skill_load_loads_content_and_resources() {
        let tmp = TempDir::new().unwrap();
        setup_skill(
            tmp.path(),
            "test-skill",
            "---\nname: Test Skill\ndescription: A test\n---\n\n# Instructions\n\nDo the thing.",
            &[
                ("references/guide.md", "# Guide\nSome guide content."),
                ("assets/run.sh", "#!/bin/bash\necho hello"),
            ],
        );

        let tool =
            SkillLoadTool::new(tmp.path()).with_available_skills(vec!["test-skill".to_string()]);
        let result = tool
            .call(SkillLoadArgs {
                skill_code: "test-skill".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.skill_code, "test-skill");
        assert_eq!(result.name, "Test Skill");
        assert!(result.content.contains("# Instructions"));
        assert!(result.content.contains("Do the thing."));
        assert_eq!(result.resource_files.len(), 2);
        assert!(result.resource_files.contains(&"assets/run.sh".to_string()));
        assert!(
            result
                .resource_files
                .contains(&"references/guide.md".to_string())
        );
    }

    #[tokio::test]
    async fn test_skill_load_rejects_unknown_skill() {
        let tmp = TempDir::new().unwrap();
        let tool =
            SkillLoadTool::new(tmp.path()).with_available_skills(vec!["allowed-skill".to_string()]);
        let result = tool
            .call(SkillLoadArgs {
                skill_code: "unknown-skill".to_string(),
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_skill_load_no_resources() {
        let tmp = TempDir::new().unwrap();
        setup_skill(
            tmp.path(),
            "minimal",
            "---\nname: Minimal\n---\n\nJust content.",
            &[],
        );

        let tool =
            SkillLoadTool::new(tmp.path()).with_available_skills(vec!["minimal".to_string()]);
        let result = tool
            .call(SkillLoadArgs {
                skill_code: "minimal".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.name, "Minimal");
        assert!(result.resource_files.is_empty());
    }
}
