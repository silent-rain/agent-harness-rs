//! 命令按需加载工具
//!
//! 当用户使用 `/<command>` 斜杠指令时，通过此工具按需加载
//! 对应命令的完整提示词内容，并注入到当前对话的 prompt 中。
//!
//! ## 设计说明
//!
//! 属于"两阶段 Command 加载"：
//! 1. 启动时：仅将 command 的 `name` 摘要注入 System Prompt
//! 2. **触发时**：LLM 调用 `command_load` 工具加载完整命令内容（本工具）
//!
//! ## 与 Skill 的区别
//!
//! - **Skill**：模型自主调用，注入 System Prompt，LLM 按需触发
//! - **Command**：用户 / 宿主通过 `/<command>` 显式触发，一条明确指令执行特定任务

use std::collections::HashMap;
use std::sync::Arc;

use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::{error::Error, types::command::CommandConfig};

/// 命令按需加载工具
///
/// 根据 `command_name` 从已注册的命令列表中查找并返回命令的完整提示词内容。
/// 命令内容在构建时已加载到内存中（来自本地文件或动态注入），无需再次读取文件。
pub struct CommandLoadTool {
    /// 已注册的命令映射表（name → content）
    commands: Arc<HashMap<String, String>>,
}

impl CommandLoadTool {
    /// 创建命令加载工具
    ///
    /// # Parameters
    ///
    /// * `command_configs` - 已加载的命令配置列表
    pub fn new(command_configs: &[CommandConfig]) -> Self {
        let commands: HashMap<String, String> = command_configs
            .iter()
            .map(|c| (c.name.clone(), c.content.clone()))
            .collect();
        Self {
            commands: Arc::new(commands),
        }
    }

    /// 获取可用的命令名称列表（已排序）
    pub fn available_commands(&self) -> Vec<String> {
        let mut names: Vec<String> = self.commands.keys().cloned().collect();
        names.sort();
        names
    }
}

/// 命令加载工具参数
#[derive(Debug, Deserialize)]
pub struct CommandLoadArgs {
    /// 命令名称（即 Commands 列表中的命令标识）
    command_name: String,
}

/// 命令加载工具输出
#[derive(Debug, Serialize)]
pub struct CommandLoadOutput {
    /// 命令名称
    command_name: String,
    /// 命令完整提示词内容
    content: String,
}

impl Tool for CommandLoadTool {
    const NAME: &'static str = "command_load";

    type Error = Error;
    type Args = CommandLoadArgs;
    type Output = CommandLoadOutput;

    fn description(&self) -> String {
        "加载指定命令的完整提示词内容。当用户使用 /<command> 斜杠指令时，\
                调用此工具获取该命令的完整操作指令。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command_name": {
                    "type": "string",
                    "description": "要加载的命令名称（即 Commands 列表中的命令标识）"
                }
            },
            "required": ["command_name"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let command_name = &args.command_name;

        // Look up command content from the in-memory registry
        let content = self
            .commands
            .get(command_name)
            .ok_or_else(|| Error::ToolExecution {
                tool_name: "command_load".to_string(),
                message: format!(
                    "命令 '{}' 不在可用列表中。可用命令: {}",
                    command_name,
                    self.available_commands().join(", ")
                ),
            })?;

        Ok(CommandLoadOutput {
            command_name: command_name.clone(),
            content: content.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::command::CommandSource;

    fn make_commands() -> Vec<CommandConfig> {
        vec![
            CommandConfig {
                name: "gen-doc".to_string(),
                description: String::new(),
                content: "Generate documentation for the given code.".to_string(),
                source: CommandSource::Local,
                source_hint: Some(".codebuddy/commands/gen-doc.md".to_string()),
            },
            CommandConfig {
                name: "gen-test".to_string(),
                description: "Generate unit tests for a given module.".to_string(),
                content: "Generate unit tests for the given code.".to_string(),
                source: CommandSource::Injected,
                source_hint: Some("db:42".to_string()),
            },
        ]
    }

    #[tokio::test]
    async fn test_command_load_loads_content() {
        let commands = make_commands();
        let tool = CommandLoadTool::new(&commands);

        let result = tool
            .call(CommandLoadArgs {
                command_name: "gen-doc".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.command_name, "gen-doc");
        assert_eq!(result.content, "Generate documentation for the given code.");
    }

    #[tokio::test]
    async fn test_command_load_loads_injected_command() {
        let commands = make_commands();
        let tool = CommandLoadTool::new(&commands);

        let result = tool
            .call(CommandLoadArgs {
                command_name: "gen-test".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.command_name, "gen-test");
        assert!(result.content.contains("Generate unit tests"));
    }

    #[tokio::test]
    async fn test_command_load_rejects_unknown_command() {
        let commands = make_commands();
        let tool = CommandLoadTool::new(&commands);

        let result = tool
            .call(CommandLoadArgs {
                command_name: "unknown-cmd".to_string(),
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown-cmd"));
        assert!(err.contains("不在可用列表中"));
    }

    #[tokio::test]
    async fn test_command_load_empty_registry() {
        let tool = CommandLoadTool::new(&[]);

        let result = tool
            .call(CommandLoadArgs {
                command_name: "any".to_string(),
            })
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_available_commands_sorted() {
        let commands = make_commands();
        let tool = CommandLoadTool::new(&commands);

        let available = tool.available_commands();
        assert_eq!(available, vec!["gen-doc", "gen-test"]);
    }
}
