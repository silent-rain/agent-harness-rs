//! Skill 脚本执行工具
//!
//! 在指定技能的工作目录下执行脚本，并按通配符收集输出文件，
//! 将文件内容与 MIME 类型一并回传给 LLM。
//!
//! ## 设计说明
//!
//! 属于"三阶段 Skill 加载"中的**第三阶段**执行工具：
//! 1. `skill_load`：加载 SKILL.md 完整指令
//! 2. `skill_select_docs`：读取参考文档
//! 3. **`skill_exec`**：执行脚本并收集输出文件（本工具）
//!
//! ## 输出文件收集
//!
//! 脚本执行完成后，工具按 `output_patterns` 中的 glob 通配符扫描工作目录，
//! 将匹配的文件读取为文本内容，并根据文件扩展名推断 MIME 类型后回传。
//!
//! ## 安全约束
//!
//! - 脚本路径必须在 skill 目录内（防路径穿越）
//! - 工作目录固定为 skill 目录（不可自定义）
//! - 仅允许白名单内的 skill_code

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command;

use crate::{
    constant::{DEFAULT_SKILLS_DIR, DEFAULT_WORKSPACE_DIR},
    error::Error,
};

/// Skill 脚本执行工具
///
/// 在指定技能目录下执行脚本文件，执行完成后按通配符收集输出文件，
/// 将文件内容与 MIME 类型一并回传。
pub struct SkillExecTool {
    /// Skills 根目录路径（如 `.codebuddy/skills`）
    skills_root: PathBuf,
    /// 可用的 skill_code 集合（白名单）
    available_skills: HashSet<String>,
}

impl Default for SkillExecTool {
    fn default() -> Self {
        Self {
            skills_root: Path::new(DEFAULT_WORKSPACE_DIR).join(DEFAULT_SKILLS_DIR),
            available_skills: HashSet::new(),
        }
    }
}

impl SkillExecTool {
    /// 创建 Skill 脚本执行工具
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
    /// 将 `skills_root` 重新设置为 `workspace_dir/skills`。
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

/// 脚本执行工具参数
#[derive(Debug, Deserialize)]
pub struct SkillExecArgs {
    /// 技能编码（对应 skills 目录下的子目录名）
    skill_code: String,
    /// 要执行的脚本路径（相对于 skill 目录，如 `assets/run.sh`）
    script: String,
    /// 传递给脚本的参数列表（可选）
    #[serde(default)]
    args: Vec<String>,
    /// 执行完成后按通配符收集输出文件的模式列表（可选，如 `["output/*.json", "*.patch"]`）
    #[serde(default)]
    output_patterns: Vec<String>,
    /// 注入到脚本进程的环境变量（可选）
    #[serde(default)]
    env: Vec<EnvVar>,
}

/// 环境变量键值对
#[derive(Debug, Deserialize)]
pub struct EnvVar {
    /// 环境变量名
    key: String,
    /// 环境变量值
    value: String,
}

/// 单个输出文件的内容与元信息
#[derive(Debug, Serialize)]
pub struct OutputFile {
    /// 文件路径（相对于 skill 目录）
    path: String,
    /// 文件内容（文本）
    content: String,
    /// MIME 类型（根据文件扩展名推断）
    mime_type: String,
}

/// 脚本执行工具输出
#[derive(Debug, Serialize)]
pub struct SkillExecOutput {
    /// 技能编码
    skill_code: String,
    /// 执行的脚本路径
    script: String,
    /// 脚本退出码
    exit_code: i32,
    /// 脚本标准输出
    stdout: String,
    /// 脚本标准错误
    stderr: String,
    /// 按通配符收集到的输出文件列表
    output_files: Vec<OutputFile>,
}

impl Tool for SkillExecTool {
    const NAME: &'static str = "skill_exec";

    type Error = Error;
    type Args = SkillExecArgs;
    type Output = SkillExecOutput;

    fn description(&self) -> String {
        "在指定技能的工作目录下执行脚本，并按通配符收集输出文件内容。\
                脚本路径相对于 skill 目录（如 `assets/run.sh`）。\
                执行完成后可通过 output_patterns 指定 glob 模式收集生成的文件，\
                工具会自动读取文件内容并推断 MIME 类型后回传。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_code": {
                    "type": "string",
                    "description": "技能编码（即 Skills 列表中的技能标识）"
                },
                "script": {
                    "type": "string",
                    "description": "要执行的脚本路径（相对于 skill 目录，如 \"assets/run.sh\"）"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "传递给脚本的参数列表（可选）"
                },
                "output_patterns": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "执行后按通配符收集输出文件的模式列表（可选，如 [\"output/*.json\", \"*.patch\"]）"
                },
                "env": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "key": { "type": "string" },
                            "value": { "type": "string" }
                        },
                        "required": ["key", "value"]
                    },
                    "description": "注入到脚本进程的环境变量（可选）"
                }
            },
            "required": ["skill_code", "script"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let skill_code = &args.skill_code;

        // Validate skill_code is in the available set
        if !self.available_skills.contains(skill_code) {
            return Err(Error::ToolExecution {
                tool_name: "skill_exec".to_string(),
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
                tool_name: "skill_exec".to_string(),
                message: format!("技能 '{}' 的目录不存在", skill_code),
            });
        }

        // Validate script path (no path traversal, must be within skill_dir)
        let script_path = validate_script_path(&skill_dir, &args.script)?;

        if !script_path.exists() {
            return Err(Error::ToolExecution {
                tool_name: "skill_exec".to_string(),
                message: format!("脚本文件不存在: {}", args.script),
            });
        }

        // Execute the script
        let exec_result = execute_script(&skill_dir, &script_path, &args.args, &args.env).await?;

        // Collect output files by glob patterns
        let output_files = if args.output_patterns.is_empty() {
            Vec::new()
        } else {
            collect_output_files(&skill_dir, &args.output_patterns).await
        };

        Ok(SkillExecOutput {
            skill_code: skill_code.clone(),
            script: args.script.clone(),
            exit_code: exec_result.exit_code,
            stdout: exec_result.stdout,
            stderr: exec_result.stderr,
            output_files,
        })
    }
}

/// 脚本执行中间结果
struct ExecResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

/// 验证脚本路径安全性
///
/// 确保脚本路径在 skill 目录内，防止路径穿越攻击。
///
/// # Parameters
///
/// * `skill_dir` - skill 目录绝对路径
/// * `script` - 脚本相对路径（来自 LLM 输入）
///
/// # Returns
///
/// 验证通过返回脚本的绝对路径
fn validate_script_path(skill_dir: &Path, script: &str) -> Result<PathBuf, Error> {
    // Reject obvious path traversal attempts
    if script.contains("..") {
        return Err(Error::WorkspacePolicy(format!(
            "脚本路径不允许包含 '..': {}",
            script
        )));
    }

    let script_path = skill_dir.join(script);

    // Canonicalize skill_dir for comparison (may not exist yet, use as-is if fails)
    let canonical_skill = skill_dir
        .canonicalize()
        .unwrap_or_else(|_| skill_dir.to_path_buf());

    // For the script path, we check the parent directory since the file may not exist
    let canonical_script = script_path
        .canonicalize()
        .unwrap_or_else(|_| script_path.clone());

    if !canonical_script.starts_with(&canonical_skill) {
        return Err(Error::WorkspacePolicy(format!(
            "脚本路径超出 skill 目录范围: {}",
            script
        )));
    }

    Ok(script_path)
}

/// 执行脚本
///
/// 在 `work_dir` 目录下执行 `script_path` 脚本，传入参数和环境变量。
///
/// # Parameters
///
/// * `work_dir` - 工作目录（skill 目录）
/// * `script_path` - 脚本绝对路径
/// * `args` - 脚本参数列表
/// * `env` - 额外注入的环境变量
///
/// # Returns
///
/// 返回退出码、stdout 和 stderr
async fn execute_script(
    work_dir: &Path,
    script_path: &Path,
    args: &[String],
    env: &[EnvVar],
) -> Result<ExecResult, Error> {
    let mut cmd = Command::new(script_path);
    cmd.current_dir(work_dir);
    cmd.args(args);

    for var in env {
        cmd.env(&var.key, &var.value);
    }

    // Retry on ETXTBSY (os error 26): Linux may return "Text file busy" when a
    // script file is executed immediately after being written. A short backoff
    // resolves the kernel-level race condition.
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY: Duration = Duration::from_millis(50);

    let output = {
        let mut last_err = None;
        let mut result = None;
        for _ in 0..MAX_RETRIES {
            match cmd.output().await {
                Ok(out) => {
                    result = Some(out);
                    break;
                }
                Err(e) if e.raw_os_error() == Some(26) => {
                    last_err = Some(e);
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(e) => {
                    return Err(Error::ToolExecution {
                        tool_name: "skill_exec".to_string(),
                        message: format!("脚本执行失败: {}", e),
                    });
                }
            }
        }
        result.ok_or_else(|| Error::ToolExecution {
            tool_name: "skill_exec".to_string(),
            message: format!(
                "脚本执行失败: {}",
                last_err.map(|e| e.to_string()).unwrap_or_default()
            ),
        })?
    };

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    Ok(ExecResult {
        exit_code,
        stdout,
        stderr,
    })
}

/// 按通配符收集输出文件
///
/// 在 `skill_dir` 目录下，按 `patterns` 中的 glob 模式匹配文件，
/// 读取文件内容并推断 MIME 类型后返回。
///
/// 读取失败的文件会被静默跳过。
///
/// # Parameters
///
/// * `skill_dir` - skill 目录路径（glob 的根目录）
/// * `patterns` - glob 通配符模式列表（相对于 skill 目录）
async fn collect_output_files(skill_dir: &Path, patterns: &[String]) -> Vec<OutputFile> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();

    for pattern in patterns {
        let full_pattern = skill_dir.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        let paths = match glob::glob(&pattern_str) {
            Ok(paths) => paths,
            Err(_) => continue,
        };

        for entry in paths.flatten() {
            if !entry.is_file() {
                continue;
            }

            // Deduplicate files matched by multiple patterns
            let canonical = entry.canonicalize().unwrap_or_else(|_| entry.clone());
            if !seen.insert(canonical) {
                continue;
            }

            // Compute relative path from skill_dir
            let rel_path = match entry.strip_prefix(skill_dir) {
                Ok(p) => p.to_string_lossy().into_owned(),
                Err(_) => continue,
            };

            // Read file content
            let content = match fs::read_to_string(&entry).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mime_type = infer_mime_type(&entry);

            result.push(OutputFile {
                path: rel_path,
                content,
                mime_type,
            });
        }
    }

    // Sort by path for deterministic output
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

/// 根据文件扩展名推断 MIME 类型
///
/// 仅返回 IANA 标准注册的 MIME 类型，避免使用 `text/x-*` 等实验性类型。
/// 源代码、脚本、补丁、配置等纯文本文件统一归为 `text/plain`，
/// 具体语言信息可由文件扩展名（path 中保留）传达。
///
/// 推断策略：
/// 1. 已知的纯文本扩展名（源码/脚本/补丁/配置/日志）→ `text/plain`
/// 2. 其余交给 `mime_guess` 按 IANA 标准表推断
/// 3. 未知扩展名 → `application/octet-stream`
///
/// # Parameters
///
/// * `path` - 文件路径
///
/// # Returns
///
/// MIME 类型字符串
fn infer_mime_type(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 源码 / 脚本 / 补丁 / 配置 / 日志统一归为 text/plain
    // （避免 text/x-rust 等非标准类型，前端/工具链不识别）
    if is_plain_text_extension(&ext) {
        return "text/plain".to_string();
    }

    // 其余扩展名交给 mime_guess（IANA 标准表）
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}

/// 判断扩展名是否属于"统一归为 text/plain"的白名单
///
/// 这些扩展名要么没有 IANA 注册的标准 MIME（如 `.rs`、`.go`、`.patch`），
/// 要么 `mime_guess` 默认返回 `text/x-*` 等非标准类型（如 `.sh`、`.py`），
/// 统一归为 `text/plain` 以保证前端/工具链兼容。
fn is_plain_text_extension(ext: &str) -> bool {
    matches!(
        ext,
        // Plain text / logs
        "txt" | "log"
        // Source code
        | "rs" | "go" | "py" | "ts" | "tsx" | "jsx" | "java"
        | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx"
        | "kt" | "swift" | "rb" | "php" | "scala" | "lua"
        // Shell scripts
        | "sh" | "bash" | "zsh" | "fish"
        // Patches / diffs
        | "patch" | "diff"
        // Config files (mime_guess 对部分配置返回 octet-stream)
        | "ini" | "conf" | "cfg" | "env" | "properties"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn setup_skill(root: &Path, code: &str) -> PathBuf {
        let skill_dir = root.join(code);
        std_fs::create_dir_all(&skill_dir).unwrap();
        skill_dir
    }

    fn write_script(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std_fs::create_dir_all(parent).unwrap();
        }
        std_fs::write(&path, content).unwrap();
        // Make executable
        let mut perms = std_fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std_fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[test]
    fn test_infer_mime_type() {
        // Standard MIME types (provided by mime_guess / IANA-registered)
        assert_eq!(
            infer_mime_type(Path::new("result.json")),
            "application/json"
        );
        assert_eq!(infer_mime_type(Path::new("report.html")), "text/html");
        assert_eq!(infer_mime_type(Path::new("page.css")), "text/css");
        assert_eq!(infer_mime_type(Path::new("readme.md")), "text/markdown");
        assert_eq!(infer_mime_type(Path::new("logo.png")), "image/png");

        // Source code / scripts / patches → unified text/plain
        assert_eq!(infer_mime_type(Path::new("main.rs")), "text/plain");
        assert_eq!(infer_mime_type(Path::new("server.go")), "text/plain");
        assert_eq!(infer_mime_type(Path::new("script.sh")), "text/plain");
        assert_eq!(infer_mime_type(Path::new("diff.patch")), "text/plain");
        assert_eq!(infer_mime_type(Path::new("app.log")), "text/plain");

        // Unknown extensions fall back to octet-stream
        // (use a deliberately invented extension that mime_guess won't recognize)
        assert_eq!(
            infer_mime_type(Path::new("unknown.zzznoexist")),
            "application/octet-stream"
        );
        assert_eq!(
            infer_mime_type(Path::new("noext")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_validate_script_path_rejects_traversal() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std_fs::create_dir_all(&skill_dir).unwrap();

        let result = validate_script_path(&skill_dir, "../other/evil.sh");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".."));
    }

    #[test]
    fn test_validate_script_path_accepts_valid() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std_fs::create_dir_all(&skill_dir).unwrap();
        std_fs::write(skill_dir.join("run.sh"), "#!/bin/bash").unwrap();

        let result = validate_script_path(&skill_dir, "run.sh");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_exec_rejects_unknown_skill() {
        let tmp = TempDir::new().unwrap();
        let tool =
            SkillExecTool::new(tmp.path()).with_available_skills(vec!["allowed".to_string()]);

        let result = tool
            .call(SkillExecArgs {
                skill_code: "unknown".to_string(),
                script: "run.sh".to_string(),
                args: vec![],
                output_patterns: vec![],
                env: vec![],
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exec_script_and_collect_output() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = setup_skill(tmp.path(), "my-skill");

        // Write a script that creates output files
        write_script(
            &skill_dir,
            "assets/run.sh",
            "#!/bin/bash\nmkdir -p output\necho '{\"ok\":true}' > output/result.json\necho done",
        );

        let tool =
            SkillExecTool::new(tmp.path()).with_available_skills(vec!["my-skill".to_string()]);

        let result = tool
            .call(SkillExecArgs {
                skill_code: "my-skill".to_string(),
                script: "assets/run.sh".to_string(),
                args: vec![],
                output_patterns: vec!["output/*.json".to_string()],
                env: vec![],
            })
            .await
            .unwrap();

        assert_eq!(result.skill_code, "my-skill");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("done"));
        assert_eq!(result.output_files.len(), 1);
        assert_eq!(result.output_files[0].path, "output/result.json");
        assert_eq!(result.output_files[0].mime_type, "application/json");
        assert!(result.output_files[0].content.contains("true"));
    }

    #[tokio::test]
    async fn test_exec_no_output_patterns() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = setup_skill(tmp.path(), "my-skill");

        write_script(&skill_dir, "run.sh", "#!/bin/bash\necho hello");

        let tool =
            SkillExecTool::new(tmp.path()).with_available_skills(vec!["my-skill".to_string()]);

        let result = tool
            .call(SkillExecArgs {
                skill_code: "my-skill".to_string(),
                script: "run.sh".to_string(),
                args: vec![],
                output_patterns: vec![],
                env: vec![],
            })
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
        assert!(result.output_files.is_empty());
    }

    #[tokio::test]
    async fn test_exec_with_args_and_env() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = setup_skill(tmp.path(), "my-skill");

        write_script(
            &skill_dir,
            "run.sh",
            "#!/bin/bash\necho \"arg=$1 env=$MY_VAR\"",
        );

        let tool =
            SkillExecTool::new(tmp.path()).with_available_skills(vec!["my-skill".to_string()]);

        let result = tool
            .call(SkillExecArgs {
                skill_code: "my-skill".to_string(),
                script: "run.sh".to_string(),
                args: vec!["hello".to_string()],
                output_patterns: vec![],
                env: vec![EnvVar {
                    key: "MY_VAR".to_string(),
                    value: "world".to_string(),
                }],
            })
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("arg=hello"));
        assert!(result.stdout.contains("env=world"));
    }

    #[tokio::test]
    async fn test_exec_nonzero_exit_code() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = setup_skill(tmp.path(), "my-skill");

        write_script(&skill_dir, "fail.sh", "#!/bin/bash\nexit 42");

        let tool =
            SkillExecTool::new(tmp.path()).with_available_skills(vec!["my-skill".to_string()]);

        let result = tool
            .call(SkillExecArgs {
                skill_code: "my-skill".to_string(),
                script: "fail.sh".to_string(),
                args: vec![],
                output_patterns: vec![],
                env: vec![],
            })
            .await
            .unwrap();

        // Non-zero exit code is returned, not an error
        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    async fn test_collect_output_files_multiple_patterns() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = setup_skill(tmp.path(), "my-skill");

        // Create output files
        std_fs::create_dir_all(skill_dir.join("output")).unwrap();
        std_fs::write(skill_dir.join("output/a.json"), r#"{"a":1}"#).unwrap();
        std_fs::write(skill_dir.join("output/b.json"), r#"{"b":2}"#).unwrap();
        std_fs::write(skill_dir.join("diff.patch"), "--- a\n+++ b").unwrap();

        let files = collect_output_files(
            &skill_dir,
            &["output/*.json".to_string(), "*.patch".to_string()],
        )
        .await;

        assert_eq!(files.len(), 3);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"diff.patch"));
        assert!(paths.contains(&"output/a.json"));
        assert!(paths.contains(&"output/b.json"));

        // Verify MIME types
        let patch = files.iter().find(|f| f.path == "diff.patch").unwrap();
        assert_eq!(patch.mime_type, "text/plain");
    }
}
