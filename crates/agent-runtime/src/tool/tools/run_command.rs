//! 受控终端命令执行工具
//!
//! 执行 Shell 命令。
//! 实现 `rig::Tool` trait，可直接注册到 `rig::ToolSet`。

use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::Error;

/// 受控终端命令执行工具
///
/// 执行 Shell 命令。
pub struct RunCommandTool;

/// 命令执行工具参数
#[derive(Debug, Deserialize)]
pub struct RunCommandArgs {
    /// 要执行的命令
    command: String,
    /// 工作目录（可选，默认使用当前目录）
    working_directory: Option<String>,
    /// 超时时间（秒，默认 30）
    timeout_secs: Option<u64>,
}

/// 命令执行工具输出
#[derive(Debug, Serialize)]
pub struct RunCommandOutput {
    /// 退出码
    exit_code: i32,
    /// 标准输出
    stdout: String,
    /// 标准错误
    stderr: String,
    /// 是否成功
    success: bool,
}

impl Tool for RunCommandTool {
    const NAME: &'static str = "run_command";

    type Error = Error;
    type Args = RunCommandArgs;
    type Output = RunCommandOutput;

    fn description(&self) -> String {
        "执行 Shell 命令。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "要执行的 Shell 命令"
                },
                "working_directory": {
                    "type": "string",
                    "description": "工作目录（可选，默认使用当前目录）"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "超时时间（秒，默认 30）"
                }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let working_dir = args.working_directory.as_deref().unwrap_or(".");
        let timeout_secs = args.timeout_secs.unwrap_or(30);

        // 执行命令（使用 sh -c 支持管道和复合命令）
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            Command::new("sh")
                .arg("-c")
                .arg(&args.command)
                .current_dir(working_dir)
                .output(),
        )
        .await
        .map_err(|_| Error::ToolExecution {
            tool_name: "run_command".to_string(),
            message: format!("命令执行超时（{}秒）", timeout_secs),
        })?
        .map_err(|e| Error::ToolExecution {
            tool_name: "run_command".to_string(),
            message: format!("命令执行失败: {}", e),
        })?;

        Ok(RunCommandOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
        })
    }
}
