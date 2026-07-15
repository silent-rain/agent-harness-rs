//! 对话消息类型
//!
//! 定义贯穿整个 agent runtime 的统一消息载体：既用于构建 LLM 上下文窗口，
//! 也用于三层上下文记忆（Session / User / Agent）的持久化存储。
//!
//! 提供的主要内容：
//! - `MessageRole`: 对话消息角色枚举
//! - `ConversationMessage`: 对话消息结构体及构造方法

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::session::PlannedToolCall;

/// 对话消息角色
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// 系统消息
    System,
    /// 用户消息
    User,
    /// 助手消息
    Assistant,
    /// 工具结果消息
    Tool,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        write!(f, "{}", s)
    }
}

impl From<&str> for MessageRole {
    fn from(s: &str) -> Self {
        match s {
            "system" => MessageRole::System,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        }
    }
}

/// 对话消息
///
/// 同时承担两类用途：
/// 1. 组装为 [`crate::context::MessageContext`] 后喂给 LLM；
/// 2. 作为三层上下文记忆（Session / User / Agent）的持久化条目。
///
/// `metadata` 字段用于业务侧附加元数据（重要性评分、来源标记等），
/// LLM 请求阶段会被忽略，仅在记忆持久化时落库。
///
/// 当 `role` 为 `Assistant` 时，`tool_calls` 字段存储 LLM 决策的工具调用列表，
/// 在转换为 rig `Message` 时映射为 `AssistantContent::ToolCall`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    /// 消息角色
    pub role: MessageRole,
    /// 消息内容
    pub content: String,
    /// 工具名称（工具结果消息时使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// 工具调用 ID（工具结果消息时使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// 工具调用列表（仅 Assistant 角色使用）
    ///
    /// 当 LLM 决策调用工具时，每个 `PlannedToolCall` 包含工具名、调用 ID 和参数。
    /// 在 [`crate::context::MessageContext::to_rig_messages`] 转换时，这些
    /// `tool_calls` 会被映射为 rig 的 `AssistantContent::ToolCall`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<PlannedToolCall>>,
    /// 业务侧附加元数据（如重要性评分、来源标记等），无则为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl ConversationMessage {
    /// 创建系统消息
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_name: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        }
    }

    /// 创建用户消息
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_name: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        }
    }

    /// 创建助手消息
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_name: None,
            tool_call_id: None,
            tool_calls: None,
            metadata: None,
        }
    }

    /// 追加包含工具调用的助手消息
    ///
    /// # Parameters
    ///
    /// * `content` - 文本内容（可为空）
    /// * `tool_calls` - LLM 决策的工具调用列表
    pub fn assistant_with_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<PlannedToolCall>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_name: None,
            tool_call_id: None,
            tool_calls: Some(tool_calls),
            metadata: None,
        }
    }

    /// 创建工具结果消息
    ///
    /// # Parameters
    ///
    /// * `tool_name` - 工具唯一编码
    /// * `tool_call_id` - 工具调用 ID（与 `tool_name` 配对，便于回放）
    /// * `content` - 工具返回内容
    pub fn tool_result(
        tool_name: impl Into<String>,
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            tool_name: Some(tool_name.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
            metadata: None,
        }
    }

    /// 链式追加业务侧附加元数据
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

// ─── 蒸馏记忆（User / Agent 粒度） ─────────────────────────────

/// 蒸馏记忆召回工具的固定名称
///
/// 在 LLM 上下文中，User / Agent 粒度的蒸馏记忆以此工具名标识。
pub const MEMORY_RECALL_TOOL_NAME: &str = "memory_recall";

/// 蒸馏记忆条目
///
/// User / Agent 粒度的上下文使用独立于 [`ConversationMessage`] 的蒸馏结构。
/// 蒸馏器（distiller）将多轮对话压缩为结构化知识条目后写入此结构。
///
/// 与 `ConversationMessage` 的区别：
/// - 不携带 `role`（蒸馏后的知识不属于任何对话角色）
/// - 携带 `source` / `tags` 等元信息，便于召回时过滤与排序
/// - 在注入 LLM 上下文时，由 [`crate::RuntimeServices`] 统一转换为
///   `tool` 角色消息（以 `memory_recall` 虚拟工具名包装），避免 system 段落膨胀
///
/// ## DB 存储格式
///
/// ```json
/// {
///   "content": "用户偏好：喜欢简洁回答，使用中文",
///   "source": "distiller_v1",
///   "tags": ["preference", "language"]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistilledMemory {
    /// 蒸馏后的知识内容（纯文本）
    pub content: String,
    /// 蒸馏来源标识（如 `distiller_v1`、`manual`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 分类标签（用于召回时过滤）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl DistilledMemory {
    /// 创建蒸馏记忆条目
    ///
    /// # Parameters
    ///
    /// * `content` - 蒸馏后的知识内容
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            source: None,
            tags: None,
        }
    }

    /// 链式设置来源标识
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// 链式设置分类标签
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// 转换为 [`ConversationMessage`]（以 `tool` 角色包装）
    ///
    /// 使用 `memory_recall` 作为虚拟工具名，将蒸馏知识注入 LLM 上下文。
    /// 这种方式避免了将所有长期记忆堆积在 system 段落中导致 system 膨胀，
    /// 同时利用 LLM 对 tool 结果的天然理解能力来消化背景知识。
    ///
    /// # Parameters
    ///
    /// * `call_id` - 工具调用 ID（用于唯一标识本次记忆召回）
    pub fn to_tool_message(&self, call_id: impl Into<String>) -> ConversationMessage {
        ConversationMessage::tool_result(MEMORY_RECALL_TOOL_NAME, call_id, &self.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_round_trip() {
        let msg = ConversationMessage::user("hi");
        let json = serde_json::to_string(&msg).unwrap();
        let back: ConversationMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, MessageRole::User);
        assert_eq!(back.content, "hi");
        assert!(back.tool_name.is_none());
        assert!(back.tool_call_id.is_none());
        assert!(back.tool_calls.is_none());
        assert!(back.metadata.is_none());
    }

    #[test]
    fn tool_result_carries_call_id() {
        let msg = ConversationMessage::tool_result("calc", "call-1", "42");
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_name.as_deref(), Some("calc"));
        assert_eq!(msg.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(msg.content, "42");
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn metadata_round_trip() {
        let msg =
            ConversationMessage::user("hi").with_metadata(serde_json::json!({ "score": 0.9 }));
        let json = serde_json::to_string(&msg).unwrap();
        let back: ConversationMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.metadata.unwrap()["score"], 0.9);
    }

    #[test]
    fn assistant_with_tool_calls_round_trip() {
        let mut msg = ConversationMessage::assistant("thinking...");
        msg.tool_calls = Some(vec![PlannedToolCall {
            call_id: "call-1".to_string(),
            tool_name: "search".to_string(),
            arguments_json: r#"{"query":"test"}"#.to_string(),
            ..Default::default()
        }]);
        let json = serde_json::to_string(&msg).unwrap();
        let back: ConversationMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, MessageRole::Assistant);
        assert_eq!(back.content, "thinking...");
        let tool_calls = back.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].call_id, "call-1");
    }
}
