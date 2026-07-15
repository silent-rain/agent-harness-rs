//! 智能体运行元数据
//!
//! 提供的主要内容：
//! - [`AgentSource`] - 智能体来源标识（主智能体 / 子智能体 / A2A）
//! - [`AgentMeta`] - 智能体运行元数据，挂载到 `LlmAgent` 上透传给子智能体

use serde::{Deserialize, Serialize};

/// 智能体来源标识
///
/// 用于区分智能体是作为主智能体直接调用，还是被父智能体委派的子智能体。
/// `run_stream_inner` 根据此字段决定是否执行 session 生命周期管理。
///
/// ## 设计意图
///
/// - [`AgentSource::Primary`]：主智能体，执行完整的 session 生命周期管理
/// - [`AgentSource::SubAgent`]：本地子智能体，跳过 session 管理，避免覆盖父智能体状态
/// - [`AgentSource::A2aAgent`]：A2A 远程智能体，同样跳过 session 管理
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum AgentSource {
    /// 主智能体（直接由用户 / 接口层调用）
    ///
    /// 执行完整的 session 生命周期管理：
    /// `create/update(Running)` → ... → `update(Completed/Cancelled/Failed)`
    #[default]
    Primary,

    /// 本地子智能体（由父智能体通过工具委派调用）
    ///
    /// 跳过 session 状态管理，避免覆盖父智能体的 `Running` 状态。
    /// 携带父级关联信息，用于事件落库时建立父子关联。
    SubAgent {
        /// 父智能体的 run_id（用于事件关联）
        parent_run_id: String,
        /// 父智能体的 agent_id
        parent_agent_id: i64,
        /// 父智能体的 session_id（冗余，方便落库）
        parent_session_id: String,
    },

    /// A2A 远程智能体（由 A2A 协议委派调用）
    ///
    /// 跳过 session 状态管理，携带远程调用的关联信息。
    A2aAgent {
        /// 父智能体的 run_id（用于事件关联）
        parent_run_id: String,
        /// 父智能体的 agent_id
        parent_agent_id: i64,
        /// 父智能体的 session_id（冗余，方便落库）
        parent_session_id: String,
    },
}

impl AgentSource {
    /// 是否为主智能体（需要执行 session 生命周期管理）
    pub fn is_primary(&self) -> bool {
        matches!(self, AgentSource::Primary)
    }

    /// 获取父级 run_id（子智能体 / A2A 场景）
    ///
    /// 主智能体返回 `None`。
    pub fn parent_run_id(&self) -> Option<&str> {
        match self {
            AgentSource::SubAgent { parent_run_id, .. }
            | AgentSource::A2aAgent { parent_run_id, .. } => Some(parent_run_id),
            AgentSource::Primary => None,
        }
    }

    /// 获取父级 agent_id（子智能体 / A2A 场景）
    ///
    /// 主智能体返回 `None`。
    pub fn parent_agent_id(&self) -> Option<i64> {
        match self {
            AgentSource::SubAgent {
                parent_agent_id, ..
            }
            | AgentSource::A2aAgent {
                parent_agent_id, ..
            } => Some(*parent_agent_id),
            AgentSource::Primary => None,
        }
    }

    /// 获取父级 session_id（子智能体 / A2A 场景）
    ///
    /// 主智能体返回 `None`。
    pub fn parent_session_id(&self) -> Option<&str> {
        match self {
            AgentSource::SubAgent {
                parent_session_id, ..
            }
            | AgentSource::A2aAgent {
                parent_session_id, ..
            } => Some(parent_session_id),
            AgentSource::Primary => None,
        }
    }
}

/// 智能体运行元数据
///
/// 透传给子智能体的上下文信息，包含来源标识和父级关联信息。
/// 挂载到 [`crate::LlmAgent`] 上，在 `run_stream_inner` 中读取。
///
/// ## 扩展性
///
/// 未来可在此结构体中添加更多透传字段，例如：
/// - `trace_id: Option<String>` — 分布式链路追踪 ID
/// - `priority: Option<u8>` — 任务优先级
///
/// 所有新增字段都应提供合理的 `Default` 值，以保证向后兼容。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMeta {
    /// 智能体来源（主智能体 / 子智能体 / A2A）
    pub agent_source: AgentSource,
}

impl AgentMeta {
    /// 构造主智能体元数据（默认值）
    ///
    /// 等价于 `AgentMeta::default()`，显式调用语义更清晰。
    pub fn primary() -> Self {
        Self {
            agent_source: AgentSource::Primary,
        }
    }

    /// 构造本地子智能体元数据
    ///
    /// # Parameters
    ///
    /// * `parent_run_id` - 父智能体的 run_id
    /// * `parent_agent_id` - 父智能体的 agent_id
    /// * `parent_session_id` - 父智能体的 session_id
    pub fn sub_agent(
        parent_run_id: String,
        parent_agent_id: i64,
        parent_session_id: String,
    ) -> Self {
        Self {
            agent_source: AgentSource::SubAgent {
                parent_run_id,
                parent_agent_id,
                parent_session_id,
            },
        }
    }

    /// 构造 A2A 远程智能体元数据
    ///
    /// # Parameters
    ///
    /// * `parent_run_id` - 父智能体的 run_id
    /// * `parent_agent_id` - 父智能体的 agent_id
    /// * `parent_session_id` - 父智能体的 session_id
    pub fn a2a_agent(
        parent_run_id: String,
        parent_agent_id: i64,
        parent_session_id: String,
    ) -> Self {
        Self {
            agent_source: AgentSource::A2aAgent {
                parent_run_id,
                parent_agent_id,
                parent_session_id,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_is_primary() {
        let meta = AgentMeta::primary();
        assert!(meta.agent_source.is_primary());
        assert!(meta.agent_source.parent_run_id().is_none());
        assert!(meta.agent_source.parent_agent_id().is_none());
        assert!(meta.agent_source.parent_session_id().is_none());
    }

    #[test]
    fn sub_agent_carries_parent_info() {
        let meta = AgentMeta::sub_agent("run-1".into(), 42, "sess-1".into());
        assert!(!meta.agent_source.is_primary());
        assert_eq!(meta.agent_source.parent_run_id(), Some("run-1"));
        assert_eq!(meta.agent_source.parent_agent_id(), Some(42));
        assert_eq!(meta.agent_source.parent_session_id(), Some("sess-1"));
    }

    #[test]
    fn a2a_agent_carries_parent_info() {
        let meta = AgentMeta::a2a_agent("run-2".into(), 99, "sess-2".into());
        assert!(!meta.agent_source.is_primary());
        assert_eq!(meta.agent_source.parent_run_id(), Some("run-2"));
        assert_eq!(meta.agent_source.parent_agent_id(), Some(99));
        assert_eq!(meta.agent_source.parent_session_id(), Some("sess-2"));
    }

    #[test]
    fn default_is_primary() {
        let meta = AgentMeta::default();
        assert!(meta.agent_source.is_primary());
    }
}
