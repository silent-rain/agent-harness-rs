//! 文件读取工具
//!
//! 读取指定路径的文件内容。
//! 实现 `rig::Tool` trait，可直接注册到 `rig::ToolSet`。

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::error::Error;

/// 文件读取工具
///
/// 读取指定路径的文件内容。
pub struct ReadFileTool;

/// 文件读取工具参数
#[derive(Debug, Deserialize)]
pub struct ReadFileArgs {
    /// 文件路径（绝对路径或相对于工作区根路径）
    path: String,
    /// 起始行号（从 1 开始，可选）
    start_line: Option<u64>,
    /// 读取行数（可选，默认读取全部）
    line_count: Option<u64>,
}

/// 文件读取工具输出
#[derive(Debug, Serialize)]
pub struct ReadFileOutput {
    /// 文件内容
    content: String,
    /// 文件路径
    path: String,
    /// 字符数
    char_count: usize,
}

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";

    type Error = Error;
    type Args = ReadFileArgs;
    type Output = ReadFileOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "读取指定路径的文件内容。支持按行范围读取。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "要读取的文件路径"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "起始行号（从 1 开始，可选）"
                    },
                    "line_count": {
                        "type": "integer",
                        "description": "读取行数（可选，默认读取全部）"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let content =
            tokio::fs::read_to_string(&args.path)
                .await
                .map_err(|e| Error::ToolExecution {
                    tool_name: "read_file".to_string(),
                    message: format!("读取文件失败: {}", e),
                })?;

        let result = if args.start_line.is_some() || args.line_count.is_some() {
            let start = args.start_line.unwrap_or(1).saturating_sub(1) as usize;
            let lines: Vec<&str> = content.lines().collect();
            let end = args
                .line_count
                .map(|c| (start + c as usize).min(lines.len()))
                .unwrap_or(lines.len());
            lines[start..end].join("\n")
        } else {
            content
        };

        let char_count = result.chars().count();
        Ok(ReadFileOutput {
            char_count,
            path: args.path,
            content: result,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_read_file_success() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "line1\nline2\nline3").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let tool = ReadFileTool;
        let args = ReadFileArgs {
            path,
            start_line: None,
            line_count: None,
        };
        let result = tool.call(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let tool = ReadFileTool;
        let args = ReadFileArgs {
            path: "/nonexistent/file.txt".to_string(),
            start_line: None,
            line_count: None,
        };
        let result = tool.call(args).await;
        assert!(result.is_err());
    }
}
