//! 智能体配置相关枚举

/// 智能体状态
///
/// 对应数据库 `t_agent_config.status` 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum AgentStatus {
    /// 草稿（未发布）
    Draft = 0,
    /// 启用（正常运行）
    Active = 1,
    /// 禁用
    Disabled = 2,
}

impl From<AgentStatus> for i16 {
    fn from(s: AgentStatus) -> Self {
        s as i16
    }
}
