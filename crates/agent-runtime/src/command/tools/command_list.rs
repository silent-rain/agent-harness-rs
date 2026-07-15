//! 命令列表扫描工具
//!
//! 提供对当前已注册命令列表的只读查询能力。
//! LLM 可通过此工具获取所有可用命令的摘要信息，
//! 再按需调用 `command_load` 工具加载具体命令的完整内容。
//!
//! ## 设计说明
//!
//! 与 `command_load` 配合使用，构成"两步式命令发现"：
//! 1. 调用 `command_list` 获取命令名称列表
//! 2. 调用 `command_load` 加载指定命令的完整提示词

use std::sync::Arc;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::{
    error::Error,
    types::command::{CommandConfig, CommandSource},
};

/// 命令列表扫描工具
///
/// 返回所有已注册命令的摘要信息（名称与来源），无需任何参数。
/// 适用于 LLM 需要了解当前可用命令全集的场景。
pub struct CommandListTool {
    /// 已注册的命令摘要列表（按名称排序）
    entries: Arc<Vec<CommandEntry>>,
}

/// 命令摘要条目
#[derive(Debug, Clone, Serialize)]
pub struct CommandEntry {
    /// 命令名称
    pub name: String,
    /// 命令描述（本地文件加载时可为空）
    pub description: String,
    /// 命令来源
    pub source: CommandSource,
}

impl CommandListTool {
    /// 创建命令列表工具
    ///
    /// # Parameters
    ///
    /// * `command_configs` - 已加载的命令配置列表
    pub fn new(command_configs: &[CommandConfig]) -> Self {
        let mut entries: Vec<CommandEntry> = command_configs
            .iter()
            .map(|c| CommandEntry {
                name: c.name.clone(),
                description: c.description.clone(),
                source: c.source.clone(),
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Self {
            entries: Arc::new(entries),
        }
    }
}

/// 命令列表工具参数（无参数）
#[derive(Debug, Deserialize)]
pub struct CommandListArgs {}

/// 命令列表工具输出
#[derive(Debug, Serialize)]
pub struct CommandListOutput {
    /// 可用命令总数
    pub total: usize,
    /// 命令摘要列表
    pub commands: Vec<CommandEntry>,
}

impl Tool for CommandListTool {
    const NAME: &'static str = "command_list";

    type Error = Error;
    type Args = CommandListArgs;
    type Output = CommandListOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "command_list".to_string(),
            description: "列出所有已注册的可用命令摘要（名称与来源）。\
                当需要了解当前可用命令全集时调用此工具，\
                再通过 command_load 工具加载具体命令的完整内容。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(CommandListOutput {
            total: self.entries.len(),
            commands: (*self.entries).clone(),
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
                name: "refactor".to_string(),
                description: String::new(),
                content: "Refactor the selected code.".to_string(),
                source: CommandSource::Local,
                source_hint: Some(".codebuddy/commands/refactor.md".to_string()),
            },
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
    async fn test_command_list_returns_all_commands() {
        let commands = make_commands();
        let tool = CommandListTool::new(&commands);

        let result = tool.call(CommandListArgs {}).await.unwrap();

        assert_eq!(result.total, 3);
        assert_eq!(result.commands.len(), 3);
    }

    #[tokio::test]
    async fn test_command_list_sorted_by_name() {
        let commands = make_commands();
        let tool = CommandListTool::new(&commands);

        let result = tool.call(CommandListArgs {}).await.unwrap();

        let names: Vec<&str> = result.commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["gen-doc", "gen-test", "refactor"]);
    }

    #[tokio::test]
    async fn test_command_list_source_field() {
        let commands = make_commands();
        let tool = CommandListTool::new(&commands);

        let result = tool.call(CommandListArgs {}).await.unwrap();

        let gen_test = result
            .commands
            .iter()
            .find(|c| c.name == "gen-test")
            .unwrap();
        assert_eq!(gen_test.source, CommandSource::Injected);

        let gen_doc = result
            .commands
            .iter()
            .find(|c| c.name == "gen-doc")
            .unwrap();
        assert_eq!(gen_doc.source, CommandSource::Local);
    }

    #[tokio::test]
    async fn test_command_list_empty_registry() {
        let tool = CommandListTool::new(&[]);

        let result = tool.call(CommandListArgs {}).await.unwrap();

        assert_eq!(result.total, 0);
        assert!(result.commands.is_empty());
    }
}
