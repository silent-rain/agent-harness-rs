//! 错误类型定义
//!
//! 提供 `agent-runtime` 库级别的统一错误类型。

use std::io;

use thiserror::Error;

/// Agent 运行时错误
#[derive(Error, Debug)]
pub enum Error {
    /// io error, no data available
    #[error("io error, no data available")]
    NoDataAvailable,
    #[error(transparent)]
    Io(io::Error),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 模型调用错误
    #[error("模型调用错误: {0}")]
    ModelCall(String),

    /// 工具执行错误
    #[error("工具执行错误: {tool_name} - {message}")]
    ToolExecution { tool_name: String, message: String },

    /// 工作区策略违规
    #[error("工作区策略违规: {0}")]
    WorkspacePolicy(String),

    /// 函数调用协议错误
    #[error("函数调用协议错误: {0}")]
    FunctionCallProtocol(String),

    /// 函数参数校验失败
    #[error("函数参数校验失败: {function_name} - {message}")]
    FunctionValidation {
        function_name: String,
        message: String,
    },

    /// 未知函数名
    #[error("未知函数名: {0}")]
    UnknownFunction(String),

    /// 记忆层错误
    #[error("记忆层错误: {0}")]
    Memory(String),

    /// 知识检索错误
    #[error("知识检索错误: {0}")]
    KnowledgeQuery(String),

    /// 子智能体已禁用
    #[error("子智能体已禁用")]
    SubAgentDisabled,

    /// 超出预算限制
    #[error("超出预算限制: {0}")]
    BudgetExceeded(String),

    /// 超出最大轮次
    #[error("超出最大推理轮次: {0}")]
    MaxTurnsExceeded(u32),

    /// 流式输出错误
    #[error("流式输出错误: {0}")]
    Streaming(String),

    /// 序列化/反序列化错误
    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    /// MCP 不支持的传输类型
    #[error("MCP 不支持的传输类型: '{transport_type}', 支持: stdio, http, sse")]
    McpUnsupportedTransport { transport_type: String },

    /// MCP 连接错误
    #[error("MCP 连接错误: service='{service_name}', {message}")]
    McpConnection {
        service_name: String,
        message: String,
    },

    /// MCP 配置错误
    #[error("MCP 配置错误: service='{service_name}', {message}")]
    McpConfig {
        service_name: String,
        message: String,
    },

    /// MCP 工具列表获取错误
    #[error("MCP 工具列表获取错误: service='{service_name}', {message}")]
    McpToolList {
        service_name: String,
        message: String,
    },

    /// MCP 服务未找到
    #[error("MCP 服务未找到: '{service_name}'")]
    McpServiceNotFound { service_name: String },

    /// Agent 运行结束但未产出最终回答
    #[error("Agent 运行结束但未产出最终回答")]
    NoFinalAnswer,

    /// Skill 安装/卸载/查询错误
    #[error(transparent)]
    Skill(#[from] SkillError),

    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

/// Agent 运行时 Result 类型别名
pub type AgentResult<T> = Result<T, Error>;

/// IO 错误转换
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        if err.kind() == io::ErrorKind::UnexpectedEof {
            return Error::NoDataAvailable;
        }
        Error::Io(err)
    }
}

/// Skill 安装/卸载/查询相关错误
///
/// 由 [`SkillPackageStore`](crate::skill::SkillPackageStore) 的各方法产生，
#[derive(Error, Debug)]
pub enum SkillError {
    /// 数据库操作失败
    #[error("skill db error: {0}")]
    Database(String),

    /// 包未找到
    #[error("skill package not found: user_id={user_id}, skill_code={skill_code}")]
    PackageNotFound { user_id: i64, skill_code: String },

    /// 包 ID 未找到
    #[error("skill package not found: user_id={user_id}, package_id={package_id}")]
    PackageIdNotFound { user_id: i64, package_id: i64 },

    /// 内部错误
    #[error("skill internal error: {0}")]
    Internal(String),
}
