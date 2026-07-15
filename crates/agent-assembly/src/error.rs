//! agent-assembly 错误类型

use agent_base::error::{Error as AgentError, SkillError};
use thiserror::Error;

/// agent-assembly 错误
#[derive(Error, Debug)]
pub enum Error {
    /// 数据库查询错误
    #[error("数据库错误: {0}")]
    Database(#[from] sea_orm::DbErr),

    /// 配置未找到
    #[error("智能体配置未找到: agent_id={0}")]
    AgentNotFound(i64),

    /// 模型绑定未找到
    #[error("模型绑定未找到: agent_id={0}")]
    ModelBindingNotFound(i64),

    /// 模型定义未找到
    #[error("模型定义未找到: model_id={0}")]
    LlmModelNotFound(i64),

    /// 本地配置解析错误
    #[error("本地配置解析错误: {0}")]
    LocalConfigParse(String),

    /// JSON 解析错误
    #[error("JSON 解析错误: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// IO 错误
    #[error("Agent Error: {0}")]
    AgentError(#[from] AgentError),

    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// Skill 安装/卸载/查询错误
    #[error(transparent)]
    Skill(#[from] SkillError),

    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

/// agent-assembly 结果类型
pub type PlatformResult<T> = Result<T, Error>;
