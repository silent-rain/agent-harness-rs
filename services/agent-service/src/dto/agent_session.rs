//! 智能体会话管理请求/响应数据传输对象

use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 取消请求 ─────────────────────────────────────────────────────────────────

/// 取消会话运行请求
///
/// 用于 `POST /agent/session/cancel`。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SessionCancelReq {
    /// 会话 ID
    #[validate(length(min = 1, max = 128, message = "会话 ID 不能为空且不超过 128 字符"))]
    pub session_id: String,
    /// 智能体 ID
    pub agent_id: i64,
}

/// 取消会话运行响应
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionCancelResp {
    /// 是否成功取消
    pub cancelled: bool,
    /// 提示信息
    pub message: String,
}

// ─── 状态查询请求 ─────────────────────────────────────────────────────────────

/// 查询会话状态请求
///
/// 用于 `POST /agent/session/status`。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SessionStatusReq {
    /// 会话 ID
    #[validate(length(min = 1, max = 128, message = "会话 ID 不能为空且不超过 128 字符"))]
    pub session_id: String,
    /// 智能体 ID
    pub agent_id: i64,
}

/// 查询会话状态响应
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionStatusResp {
    /// 会话是否存在
    pub exists: bool,
    /// 会话状态（不存在时为 None）
    pub status: Option<String>,
    /// 状态码（不存在时为 None）
    pub status_code: Option<i16>,
}

// ─── 消息注入请求 ─────────────────────────────────────────────────────────────

/// 向运行中的会话注入用户消息请求
///
/// 用于 `POST /agent/session/inject-message`。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct InjectMessageReq {
    /// 会话 ID
    #[validate(length(min = 1, max = 128, message = "会话 ID 不能为空且不超过 128 字符"))]
    pub session_id: String,
    /// 智能体 ID
    pub agent_id: i64,
    /// 用户 ID
    pub user_id: i64,
    /// 要注入的消息内容
    #[validate(length(min = 1, max = 65536, message = "消息内容不能为空且不超过 64KB"))]
    pub message: String,
}

/// 消息注入响应
#[derive(Debug, Serialize, Deserialize)]
pub struct InjectMessageResp {
    /// 是否成功注入
    pub injected: bool,
    /// 提示信息
    pub message: String,
}
