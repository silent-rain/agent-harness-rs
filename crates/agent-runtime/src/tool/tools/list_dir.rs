//! 目录列表工具
//!
//! 列出指定目录的内容。
//! 实现 `rig::Tool` trait，可直接注册到 `rig::ToolSet`。

use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::error::{AgentResult, Error};

/// 目录列表工具
///
/// 列出指定目录下的文件和子目录信息。
pub struct ListDirTool;

/// 目录列表工具参数
#[derive(Debug, Deserialize)]
pub struct ListDirArgs {
    /// 目录路径
    path: String,
    /// 是否显示隐藏文件（默认 false）
    show_hidden: Option<bool>,
}

/// 目录条目信息
#[derive(Debug, Serialize)]
pub struct DirEntry {
    name: String,
    entry_type: String,
    size: Option<u64>,
}

/// 目录列表工具输出
#[derive(Debug, Serialize)]
pub struct ListDirOutput {
    /// 目录路径
    path: String,
    /// 条目总数
    total: usize,
    /// 条目列表
    entries: Vec<DirEntry>,
}

impl Tool for ListDirTool {
    const NAME: &'static str = "list_dir";

    type Error = Error;
    type Args = ListDirArgs;
    type Output = ListDirOutput;

    fn description(&self) -> String {
        "列出指定目录下的文件和子目录信息，包括文件大小和类型。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要列出内容的目录路径"
                },
                "show_hidden": {
                    "type": "boolean",
                    "description": "是否显示隐藏文件（默认 false）"
                }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let show_hidden = args.show_hidden.unwrap_or(false);
        let path = args.path.clone();

        // 使用 spawn_blocking 执行同步 IO，避免阻塞异步运行时
        let entries = tokio::task::spawn_blocking(move || read_dir_entries(&path, show_hidden))
            .await
            .map_err(|e| Error::ToolExecution {
                tool_name: "list_dir".to_string(),
                message: format!("任务执行失败: {}", e),
            })??;

        let total = entries.len();
        Ok(ListDirOutput {
            path: args.path,
            total,
            entries,
        })
    }
}

/// 读取目录条目（同步版本，在 spawn_blocking 中执行）
fn read_dir_entries(path: &str, show_hidden: bool) -> AgentResult<Vec<DirEntry>> {
    let read_dir = std::fs::read_dir(path).map_err(|e| Error::ToolExecution {
        tool_name: "list_dir".to_string(),
        message: format!("读取目录失败: {}", e),
    })?;

    let mut entries: Vec<DirEntry> = read_dir
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let is_dir = metadata.is_dir();
            let size = if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            };
            Some(DirEntry {
                name,
                entry_type: if is_dir {
                    "directory".to_string()
                } else {
                    "file".to_string()
                },
                size,
            })
        })
        .collect();

    // 目录在前，文件在后，同类按名称排序
    entries.sort_by(|a, b| {
        let a_is_dir = a.entry_type == "directory";
        let b_is_dir = b.entry_type == "directory";
        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    Ok(entries)
}
