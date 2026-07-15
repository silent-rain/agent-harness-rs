//! 技能文档选择读取工具
//!
//! 读取指定技能目录下一个或多个资源文件的完整内容，
//! 供 LLM 在加载技能后精确获取所需文档。
//!
//! ## 设计说明
//!
//! 属于"三阶段 Skill 加载"中的**第三阶段**核心工具：
//! - `skill_load` 返回资源文件列表（概览）
//! - `skill_list_docs` 重新查询文档列表
//! - `skill_select_docs` 读取具体文档的完整内容（本工具）

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::{
    constant::{DEFAULT_SKILLS_DIR, DEFAULT_WORKSPACE_DIR, SKILL_ENTRY_FILE},
    error::Error,
};

/// 技能文档选择读取工具
///
/// 根据 `skill_code` 和文件路径列表，读取 skill 目录下指定资源文件的完整内容。
/// 路径为相对于 skill 目录的相对路径（如 `references/guide.md`）。
pub struct SkillSelectDocsTool {
    /// Skills 根目录路径（如 `.codebuddy/skills`）
    skills_root: PathBuf,
    /// 可用的 skill_code 集合（白名单）
    available_skills: HashSet<String>,
}

impl Default for SkillSelectDocsTool {
    fn default() -> Self {
        Self {
            skills_root: Path::new(DEFAULT_WORKSPACE_DIR).join(DEFAULT_SKILLS_DIR),
            available_skills: HashSet::new(),
        }
    }
}

impl SkillSelectDocsTool {
    /// 创建技能文档选择读取工具
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

/// 技能文档选择工具参数
#[derive(Debug, Deserialize)]
pub struct SkillSelectDocsArgs {
    /// 技能编码（对应 skills 目录下的子目录名）
    skill_code: String,
    /// 要读取的文件路径列表（相对于 skill 目录，如 `["references/guide.md", "assets/run.sh"]`）
    docs: Vec<String>,
}

/// 单个文档的读取结果
#[derive(Debug, Serialize)]
pub struct DocEntry {
    /// 文件路径（相对于 skill 目录）
    path: String,
    /// 文件完整内容
    content: String,
}

/// 技能文档选择工具输出
#[derive(Debug, Serialize)]
pub struct SkillSelectDocsOutput {
    /// 技能编码
    skill_code: String,
    /// 成功读取的文档列表
    docs: Vec<DocEntry>,
    /// 未找到或读取失败的文件路径列表
    not_found: Vec<String>,
}

impl Tool for SkillSelectDocsTool {
    const NAME: &'static str = "skill_select_docs";

    type Error = Error;
    type Args = SkillSelectDocsArgs;
    type Output = SkillSelectDocsOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "skill_select_docs".to_string(),
            description: "读取指定技能下一个或多个资源文档的完整内容。\
                先通过 skill_load 或 skill_list_docs 获取文档路径列表，\
                再通过此工具精确读取所需文档内容。\
                支持一次请求读取多个文档。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill_code": {
                        "type": "string",
                        "description": "技能编码（即 Skills 列表中的技能标识）"
                    },
                    "docs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "要读取的文件路径列表（相对于 skill 目录，如 [\"references/guide.md\"]）"
                    }
                },
                "required": ["skill_code", "docs"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let skill_code = &args.skill_code;

        if !self.available_skills.contains(skill_code) {
            return Err(Error::ToolExecution {
                tool_name: "skill_select_docs".to_string(),
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
                tool_name: "skill_select_docs".to_string(),
                message: format!("技能 '{}' 的目录不存在", skill_code),
            });
        }

        let mut docs = Vec::new();
        let mut not_found = Vec::new();

        for doc_path in &args.docs {
            // Reject attempts to read SKILL.md itself or path traversal
            if doc_path == SKILL_ENTRY_FILE || doc_path.contains("..") {
                not_found.push(doc_path.clone());
                continue;
            }

            let full_path = skill_dir.join(doc_path);

            // Ensure the resolved path is still within skill_dir (path traversal guard)
            let canonical_skill = skill_dir
                .canonicalize()
                .unwrap_or_else(|_| skill_dir.clone());
            let canonical_file = match full_path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    not_found.push(doc_path.clone());
                    continue;
                }
            };

            if !canonical_file.starts_with(&canonical_skill) {
                not_found.push(doc_path.clone());
                continue;
            }

            match fs::read_to_string(&full_path).await {
                Ok(content) => docs.push(DocEntry {
                    path: doc_path.clone(),
                    content,
                }),
                Err(_) => not_found.push(doc_path.clone()),
            }
        }

        Ok(SkillSelectDocsOutput {
            skill_code: skill_code.clone(),
            docs,
            not_found,
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
    async fn test_select_docs_reads_content() {
        let tmp = TempDir::new().unwrap();
        setup_skill(
            tmp.path(),
            "my-skill",
            &[
                ("references/guide.md", "# Guide\nSome guide content."),
                ("assets/run.sh", "#!/bin/bash\necho hello"),
            ],
        );

        let tool = SkillSelectDocsTool::new(tmp.path())
            .with_available_skills(vec!["my-skill".to_string()]);
        let result = tool
            .call(SkillSelectDocsArgs {
                skill_code: "my-skill".to_string(),
                docs: vec!["references/guide.md".to_string()],
            })
            .await
            .unwrap();

        assert_eq!(result.skill_code, "my-skill");
        assert_eq!(result.docs.len(), 1);
        assert_eq!(result.docs[0].path, "references/guide.md");
        assert!(result.docs[0].content.contains("Some guide content."));
        assert!(result.not_found.is_empty());
    }

    #[tokio::test]
    async fn test_select_docs_multiple_files() {
        let tmp = TempDir::new().unwrap();
        setup_skill(
            tmp.path(),
            "my-skill",
            &[
                ("references/guide.md", "# Guide"),
                ("assets/run.sh", "#!/bin/bash"),
            ],
        );

        let tool = SkillSelectDocsTool::new(tmp.path())
            .with_available_skills(vec!["my-skill".to_string()]);
        let result = tool
            .call(SkillSelectDocsArgs {
                skill_code: "my-skill".to_string(),
                docs: vec![
                    "references/guide.md".to_string(),
                    "assets/run.sh".to_string(),
                ],
            })
            .await
            .unwrap();

        assert_eq!(result.docs.len(), 2);
        assert!(result.not_found.is_empty());
    }

    #[tokio::test]
    async fn test_select_docs_not_found() {
        let tmp = TempDir::new().unwrap();
        setup_skill(tmp.path(), "my-skill", &[]);

        let tool = SkillSelectDocsTool::new(tmp.path())
            .with_available_skills(vec!["my-skill".to_string()]);
        let result = tool
            .call(SkillSelectDocsArgs {
                skill_code: "my-skill".to_string(),
                docs: vec!["nonexistent.md".to_string()],
            })
            .await
            .unwrap();

        assert!(result.docs.is_empty());
        assert_eq!(result.not_found, vec!["nonexistent.md"]);
    }

    #[tokio::test]
    async fn test_select_docs_rejects_skill_md() {
        let tmp = TempDir::new().unwrap();
        setup_skill(tmp.path(), "my-skill", &[]);

        let tool = SkillSelectDocsTool::new(tmp.path())
            .with_available_skills(vec!["my-skill".to_string()]);
        let result = tool
            .call(SkillSelectDocsArgs {
                skill_code: "my-skill".to_string(),
                docs: vec![SKILL_ENTRY_FILE.to_string()],
            })
            .await
            .unwrap();

        // SKILL.md should be in not_found (rejected)
        assert!(result.docs.is_empty());
        assert_eq!(result.not_found.len(), 1);
    }

    #[tokio::test]
    async fn test_select_docs_rejects_unknown_skill() {
        let tmp = TempDir::new().unwrap();
        let tool =
            SkillSelectDocsTool::new(tmp.path()).with_available_skills(vec!["allowed".to_string()]);
        let result = tool
            .call(SkillSelectDocsArgs {
                skill_code: "unknown".to_string(),
                docs: vec!["guide.md".to_string()],
            })
            .await;

        assert!(result.is_err());
    }
}
