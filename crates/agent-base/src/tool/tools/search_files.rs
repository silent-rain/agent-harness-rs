//! 文件搜索工具
//!
//! 在指定目录中按文件名模式搜索文件。
//! 实现 `rig::Tool` trait，可直接注册到 `rig::ToolSet`。

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::error::{AgentResult, Error};

/// 文件搜索工具
///
/// 在指定目录中递归搜索匹配指定名称模式的文件。
pub struct SearchFilesTool;

/// 文件搜索工具参数
#[derive(Debug, Deserialize)]
pub struct SearchFilesArgs {
    /// 搜索根目录
    directory: String,
    /// 文件名搜索关键词（支持部分匹配）
    pattern: String,
    /// 最大返回数量（默认 50）
    max_results: Option<usize>,
}

/// 文件搜索工具输出
#[derive(Debug, Serialize)]
pub struct SearchFilesOutput {
    /// 匹配的文件路径列表
    matches: Vec<String>,
    /// 匹配数量
    count: usize,
}

impl Tool for SearchFilesTool {
    const NAME: &'static str = "search_files";

    type Error = Error;
    type Args = SearchFilesArgs;
    type Output = SearchFilesOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "search_files".to_string(),
            description: "在指定目录中递归搜索匹配名称模式的文件。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "directory": {
                        "type": "string",
                        "description": "搜索根目录路径"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "文件名搜索关键词（支持部分匹配）"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "最大返回数量（默认 50）"
                    }
                },
                "required": ["directory", "pattern"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let max_results = args.max_results.unwrap_or(50);
        let pattern = args.pattern.to_lowercase();
        let mut matches: Vec<String> = Vec::new();

        search_recursive(&args.directory, &pattern, &mut matches, max_results)?;

        let count = matches.len();
        Ok(SearchFilesOutput { matches, count })
    }
}

/// 递归搜索文件
fn search_recursive(
    dir: &str,
    pattern: &str,
    matches: &mut Vec<String>,
    max_results: usize,
) -> AgentResult<()> {
    if matches.len() >= max_results {
        return Ok(());
    }

    let entries = std::fs::read_dir(dir).map_err(|e| Error::ToolExecution {
        tool_name: "search_files".to_string(),
        message: format!("读取目录失败 '{}': {}", dir, e),
    })?;

    for entry in entries.flatten() {
        if matches.len() >= max_results {
            break;
        }

        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_lowercase();

        if path.is_dir() {
            let sub_dir = path.to_string_lossy().to_string();
            // 忽略隐藏目录和常见无关目录
            if !file_name.starts_with('.') && file_name != "target" && file_name != "node_modules" {
                let _ = search_recursive(&sub_dir, pattern, matches, max_results);
            }
        } else if file_name.contains(pattern) {
            matches.push(path.to_string_lossy().to_string());
        }
    }

    Ok(())
}
