//! 智能体会话相关枚举

/// 会话状态
///
/// 对应数据库 `t_agent_session.status` 字段。
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// 是否正在运行中
    pub fn is_running(&self) -> bool {
        matches!(self, SessionStatus::Running)
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
