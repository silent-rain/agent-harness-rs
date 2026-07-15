//! 智能体运行相关枚举和错误类型

use thiserror::Error;

/// 智能体运行错误
///
/// 定义智能体运行过程中可能出现的具体错误类型，
/// 替代通用的 `Error::InternalServer`。
#[derive(Error, Debug)]
pub enum AgentRunError {
    /// 智能体运行失败
    #[error("智能体运行失败: {message}")]
    RunFailed { message: String },

    /// 未收到运行开始事件
    #[error("未收到运行开始事件")]
    RunStartedEventMissing,

    /// 智能体不存在或无权限
    #[error("智能体 {agent_id} 不存在或无权限")]
    AgentNotFoundOrNoPermission { agent_id: i64 },
}

impl From<AgentRunError> for err_code::Error {
    fn from(error: AgentRunError) -> Self {
        err_code::Error::InternalServer(error.to_string())
    }
}
