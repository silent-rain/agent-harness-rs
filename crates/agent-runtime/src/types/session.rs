//! 会话相关核心类型定义
//!
//! 统一管理会话模块的核心结构体与枚举，供 `session` 子模块 re-export。
//!
//! 提供的主要内容：
//! - [`ContextScope`] - 三层上下文记忆寻址键
//! - [`SessionRef`] - agent + user + session_id 三元组值对象
//! - [`SessionStatus`] - 会话状态枚举（统一运行时状态与会话生命周期状态）
//! - [`SessionEventRecord`] - 会话事件写入载荷

use std::fmt;

use serde::{Deserialize, Serialize};

// ─── ContextScope ──────────────────────────────────────

/// 上下文作用域寻址键
///
/// 三层上下文按下列规则落库：
///
/// - `Session`：携带 `agent_id` / `user_id` 冗余字段，便于免 join 查询
/// - `User`：跨 agent 共享，仅以 `user_id` 寻址
/// - `Agent`：agent 私有，`user_id` 为空字符串表示真正全局共享
#[derive(Debug, Clone)]
pub enum ContextScope<'a> {
    /// 会话级上下文
    Session {
        /// 会话 ID
        session_id: &'a str,
        /// 智能体 ID
        agent_id: i64,
        /// 用户 ID
        user_id: &'a str,
    },
    /// 用户级上下文（跨 agent）
    User {
        /// 用户 ID
        user_id: &'a str,
    },
    /// Agent 私有上下文
    Agent {
        /// 智能体 ID
        agent_id: i64,
        /// 用户 ID
        user_id: &'a str,
    },
}

// ─── SessionRef ────────────────────────────────────────

/// 会话级上下文引用
///
/// 在 `(agent_id, user_id)` 之上附加 `session_id`，构成会话相关接口
/// 的完整三元组身份。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionRef {
    /// 会话 ID
    pub session_id: String,
    /// 智能体 ID
    pub agent_id: i64,
    /// 用户 ID（字符串，支持多种身份系统）
    pub user_id: String,
}

impl SessionRef {
    /// 构造 [`SessionRef`]
    pub fn new(agent_id: i64, user_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            agent_id,
            user_id: user_id.into(),
        }
    }

    /// 智能体 ID 直通访问
    #[inline]
    pub const fn agent_id(&self) -> i64 {
        self.agent_id
    }

    /// 用户 ID 直通访问
    #[inline]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
}

impl fmt::Display for SessionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "agent#{}:user#{}:session#{}",
            self.agent_id, self.user_id, self.session_id
        )
    }
}

// ─── SessionStatus ─────────────────────────────────────

/// 会话状态
///
/// 对应数据库 `t_agent_session.status` 字段（i16）。
/// 精简为 4 个核心状态，覆盖会话的完整生命周期。
///
/// ## 状态流转
///
/// ```text
/// (不存在) ──创建──► Idle ──开始运行──► Running
///                     ▲                    │
///                     │   成功完成          │
///                     │   (回到 Idle)       │
///                     ├────────────────────┤
///                     │                    ├──► Cancelled（取消）
///                     │                    └──► Completed（完成/失败）
///                     │                              │
///                     └──────────────────────────────┘
///                              (可再次运行)
/// ```
///
/// ## 语义说明
///
/// - `Idle`：未运行 / 空闲状态，可接受新运行
/// - `Running`：有活跃运行正在执行
/// - `Cancelled`：最近一次运行被取消
/// - `Completed`：最近一次运行已结束（成功或失败）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i16)]
pub enum SessionStatus {
    /// 未运行 / 空闲（可接受新运行）
    Idle = 0,
    /// 运行中（有活跃运行正在执行）
    Running = 1,
    /// 已取消（最近一次运行被取消）
    Cancelled = 2,
    /// 已完成（最近一次运行已结束，成功或失败）
    Completed = 3,
}

impl SessionStatus {
    /// 是否允许启动新运行
    ///
    /// 除 `Running` 外的所有状态均可启动新运行。
    pub fn can_run(&self) -> bool {
        !self.is_running()
    }

    /// 是否正在运行中
    pub fn is_running(&self) -> bool {
        matches!(self, SessionStatus::Running)
    }

    /// 是否已取消
    pub fn is_cancelled(&self) -> bool {
        matches!(self, SessionStatus::Cancelled)
    }

    /// 从 i16 转换为枚举，未知值默认为 [`SessionStatus::Idle`]。
    pub fn from_raw(raw: i16) -> Self {
        match raw {
            0 => Self::Idle,
            1 => Self::Running,
            2 => Self::Cancelled,
            3 => Self::Completed,
            _ => Self::Idle,
        }
    }
}

impl From<SessionStatus> for i16 {
    fn from(s: SessionStatus) -> Self {
        s as i16
    }
}

// ─── SessionEventRecord ────────────────────────────────

/// Write payload for a session event row.
///
/// The convenience method [`crate::session::SessionService::append_event`]
/// constructs this struct and calls [`crate::session::SessionService::append_record`],
/// keeping the persistence surface minimal for custom implementations.
///
/// `agent_id` 与 `user_id` 由调用方（通常是 `LlmAgent` 自身）从
/// `AgentDefinition` 中读取并填入，使得 `SessionService` 实现可以完全无状态，
/// 不再绑定具体的 (agent_id, user_id) 二元组。
#[derive(Debug, Clone)]
pub struct SessionEventRecord<'a> {
    /// Session ID.
    pub session_id: &'a str,
    /// Agent ID（落库到 `t_agent_session_event.agent_id`）
    pub agent_id: i64,
    /// User ID（落库到 `t_agent_session_event.user_id`；字符串，支持多种身份系统）
    pub user_id: &'a str,
    /// Optional run ID (system-level events may omit it).
    pub run_id: Option<&'a str>,
    /// Event type string.
    pub event_type: &'a str,
    /// Serialized JSON payload.
    pub payload_json: String,
}
