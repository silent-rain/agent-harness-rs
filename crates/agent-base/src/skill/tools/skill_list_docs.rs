//! 技能文档列表工具
//!
//! 列出指定技能目录下所有可用的资源文件（文档、脚本等），
//! 供 LLM 在加载技能后按需选择要读取的具体文档。
//!
//! ## 设计说明
//!
//! 属于"三阶段 Skill 加载"中的**第三阶段**辅助工具：
//! - `skill_load` 返回资源文件列表（概览）
//! - `skill_list_docs` 可随时重新查询文档列表（本工具）
//! - `skill_select_docs` 读取具体文档的完整内容

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::{
    constant::{DEFAULT_SKILLS_DIR, DEFAULT_WORKSPACE_DIR},
    error::Error,
};

use super::skill_load::scan_resource_files;

/// 技能文档列表工具
///
/// 列出指定 skill 目录下所有可用的资源文件路径（相对于 skill 目录），
/// 排除 `SKILL.md` 入口文件本身。
pub struct SkillListDocsTool {
    /// Skills 根目录路径（如 `.codebuddy/skills`）
    skills_root: PathBuf,
    /// 可用的 skill_code 集合（白名单）
    available_skills: HashSet<String>,
}

impl Default for SkillListDocsTool {
    fn default() -> Self {
        Self {
            skills_root: Path::new(DEFAULT_WORKSPACE_DIR).join(DEFAULT_SKILLS_DIR),
            available_skills: HashSet::new(),
        }
    }
}

impl SkillListDocsTool {
    /// 创建技能文档列表工具
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
    /// # Parameters
    ///
    /// * `workspace_dir` - 工作空间根目录路径
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

/// 技能文档列表工具参数
#[derive(Debug, Deserialize)]
pub struct SkillListDocsArgs {
    /// 技能编码（对应 skills 目录下的子目录名）
    skill_code: String,
}

/// 技能文档列表工具输出
#[derive(Debug, Serialize)]
pub struct SkillListDocsOutput {
    /// 技能编码
    skill_code: String,
    /// 可用的资源文件列表（相对于 skill 目录的路径）
    docs: Vec<String>,
}

impl Tool for SkillListDocsTool {
    const NAME: &'static str = "skill_list_docs";

    type Error = Error;
    type Args = SkillListDocsArgs;
    type Output = SkillListDocsOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "skill_list_docs".to_string(),
            description: "列出指定技能下所有可用的资源文档和文件。\
                在调用 skill_load 加载技能后，可通过此工具查看该技能目录下的完整文档列表，\
                再通过 skill_select_docs 读取具体文档内容。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill_code": {
                        "type": "string",
                        "description": "技能编码（即 Skills 列表中的技能标识）"
                    }
                },
                "required": ["skill_code"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let skill_code = &args.skill_code;

        if !self.available_skills.contains(skill_code) {
            return Err(Error::ToolExecution {
                tool_name: "skill_list_docs".to_string(),
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
                tool_name: "skill_list_docs".to_string(),
                message: format!("技能 '{}' 的目录不存在", skill_code),
            });
        }

        let docs = scan_resource_files(&skill_dir).await;

        Ok(SkillListDocsOutput {
            skill_code: skill_code.clone(),
            docs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constant::SKILL_ENTRY_FILE;
    use std::fs as std_fs;
    use tempfile::TempDir;

    fn setup_skill(root: &Path, code: &str, resources: &[(&str, &str)]) {
        let skill_dir = root.join(code);
        std_fs::create_dir_all(&skill_dir).unwrap();
        std_fs::write(
            skill_dir.join(SKILL_ENTRY_FILE),
            "---\nname: Test\n---\nContent",
        )
        .unwrap();
        for (path, content) in resources {
            let full_path = skill_dir.join(path);
            if let Some(parent) = full_path.parent() {
                std_fs::create_dir_all(parent).unwrap();
            }
            std_fs::write(full_path, content).unwrap();
        }
    }

    #[tokio::test]
    async fn test_list_docs_returns_resource_files() {
        let tmp = TempDir::new().unwrap();
        setup_skill(
            tmp.path(),
            "my-skill",
            &[
                ("references/guide.md", "# Guide"),
                ("assets/run.sh", "#!/bin/bash"),
            ],
        );

        let tool =
            SkillListDocsTool::new(tmp.path()).with_available_skills(vec!["my-skill".to_string()]);
        let result = tool
            .call(SkillListDocsArgs {
                skill_code: "my-skill".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.skill_code, "my-skill");
        assert_eq!(result.docs.len(), 2);
        assert!(result.docs.contains(&"references/guide.md".to_string()));
        assert!(result.docs.contains(&"assets/run.sh".to_string()));
    }

    #[tokio::test]
    async fn test_list_docs_rejects_unknown_skill() {
        let tmp = TempDir::new().unwrap();
        let tool =
            SkillListDocsTool::new(tmp.path()).with_available_skills(vec!["allowed".to_string()]);
        let result = tool
            .call(SkillListDocsArgs {
                skill_code: "unknown".to_string(),
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_docs_empty_skill() {
        let tmp = TempDir::new().unwrap();
        setup_skill(tmp.path(), "empty-skill", &[]);

        let tool = SkillListDocsTool::new(tmp.path())
            .with_available_skills(vec!["empty-skill".to_string()]);
        let result = tool
            .call(SkillListDocsArgs {
                skill_code: "empty-skill".to_string(),
            })
            .await
            .unwrap();

        assert!(result.docs.is_empty());
    }
}
