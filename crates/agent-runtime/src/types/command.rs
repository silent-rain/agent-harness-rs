//! Command types
use serde::{Deserialize, Serialize};

/// 命令来源
///
/// 描述命令内容的加载来源，用于溯源和调试。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    /// 从本地文件系统加载
    Local,
    /// 在运行时动态注入（如数据库、HTTP 等外部来源）
    Injected,
}

/// 命令配置
///
/// Commands 是一种面向开发场景的指令式交互能力，用于将常见、高频、
/// 结构化的开发任务封装为可复用的命令。
///
/// 与 Skill 的核心区别：
/// - **Skill**：模型自主调用，注入 System Prompt，LLM 按需触发
/// - **Command**：用户 / 宿主显式触发，一条明确指令执行特定任务
///
/// 典型场景：
/// - 用一条指令触发 AI 执行特定任务（如：生成文档、生成单测）
/// - 减少自然语言反复描述成本（如：预设好重构提示词的指令）
/// - 让 AI 行为更稳定、可预期、可复用（如：openspec、speckit 等固定流程）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandConfig {
    /// 命令名称（文件名）
    pub name: String,
    /// 命令描述（本地文件加载时可为空）
    pub description: String,
    /// 命令提示词正文
    pub content: String,
    /// 命令来源
    pub source: CommandSource,
    /// 来源提示（文件路径或数据库标识）
    pub source_hint: Option<String>,
}
