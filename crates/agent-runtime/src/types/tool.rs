//! Tool types
use serde::{Deserialize, Serialize};

/// 工具配置
///
/// 描述智能体绑定的工具及其运行时策略。
/// 与 `rig::completion::ToolDefinition` 区分：后者是 LLM function calling 的 JSON Schema，
/// 本类型是平台层的工具元数据配置。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 函数 Schema（JSON Schema 格式）
    pub schema_json: Option<String>,
}

impl ToolConfig {
    /// 从工具名称创建工具配置
    pub fn from_name(name: &str) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            schema_json: None,
        }
    }
}

/// 工具来源类别
///
/// 标识一个被注册到 `ToolSet` 的工具来自哪一层装配源，
/// 供事件层（如 [`crate::session::AgentStreamEvent::ToolCallStarted`]）透传给前端，
/// 帮助 UI 区分卡片样式（图标、标签颜色、分组等）。
///
/// 与 [`ToolConfig`] 的区别：
/// - `ToolConfig` 是平台对工具元数据的"配置面"描述（持久化、模型 schema）；
/// - `ToolSource` 是运行时对工具调用"分类标签"的描述，仅在事件流中出现。
///
/// 工具与来源的运行时映射由 [`crate::tool::ToolPool`] 内嵌维护，
/// 装配阶段由 `AgentBuilder` 写入，运行时由 `ModelCaller` / `ToolExecutor`
/// 在发射工具相关事件时通过 [`crate::tool::ToolPool::source_of`] 回查。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    /// 内置工具（在 crate 内以 `impl rig::Tool` 直接注册）
    #[default]
    Builtin,
    /// 来自 MCP 服务的远端工具
    Mcp,
    /// 来自 A2A 远程 Agent 服务的工具
    A2a,
    /// 由技能（Skill）展开/注册的工具
    Skill,
    /// 子智能体委派工具（统一封装为 `delegate_sub_agent`）
    SubAgent,
    /// 未知来源（兜底；理论上不应出现，仅用于事件向前兼容）
    Unknown,
}

impl ToolSource {
    /// 返回来源的 snake_case 标签字符串
    ///
    /// 与序列化后的 JSON 值保持一致，便于在不进行完整序列化的前提下
    /// 用作日志、metric tag 等标签值。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Mcp => "mcp",
            Self::A2a => "a2a",
            Self::Skill => "skill",
            Self::SubAgent => "sub_agent",
            Self::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_source_as_str_matches_serde_tag() {
        assert_eq!(ToolSource::Builtin.as_str(), "builtin");
        assert_eq!(ToolSource::Mcp.as_str(), "mcp");
        assert_eq!(ToolSource::Skill.as_str(), "skill");
        assert_eq!(ToolSource::SubAgent.as_str(), "sub_agent");
        assert_eq!(ToolSource::Unknown.as_str(), "unknown");
    }

    #[test]
    fn tool_source_serde_round_trip() {
        let json = serde_json::to_string(&ToolSource::SubAgent).unwrap();
        assert_eq!(json, "\"sub_agent\"");
        let back: ToolSource = serde_json::from_str("\"mcp\"").unwrap();
        assert_eq!(back, ToolSource::Mcp);
    }
}
