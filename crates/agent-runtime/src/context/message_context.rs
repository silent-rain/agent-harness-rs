//! 对话消息上下文
//!
//! 管理 LLM 对话上下文窗口：消息累加、查询、转换为 rig `Message` 列表。
//!
//! 该模块从 `Agent` / `DefaultRunner` 中抽离原先散落的消息操作，
//! 作为未来接入上下文压缩（Context Compact）、Token 预算控制等能力的统一入口。
//!
//! 提供的主要内容：
//! - [`MessageContext`][]: 对话消息上下文管理器

use rig_core::completion::message::{
    AssistantContent, AssistantMessage, CallId, Message, Text, ToolCall, ToolFunction, ToolResult,
    ToolResultContent, UserContent,
};
use uuid::Uuid;

use crate::tool::tool_name_of;

use crate::message::{ConversationMessage, MessageRole};
use crate::session::PlannedToolCall;

/// 对话消息上下文
///
/// 封装 [`ConversationMessage`] 列表，提供统一的消息追加、查询、转换能力。
///
/// 相比直接操作 `Vec<ConversationMessage>`，该类型：
/// - 将"如何转换为 rig 消息"等细节内聚
/// - 为未来扩展（token 预算、裁剪、压缩）预留位置
/// - 保持最小 API 面，调用方仅感知"增/查/转"三类操作
#[derive(Debug, Clone, Default)]
pub struct MessageContext {
    messages: Vec<ConversationMessage>,
}

impl MessageContext {
    /// 创建空上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 基于已有消息列表构造上下文
    pub fn from_messages(messages: Vec<ConversationMessage>) -> Self {
        Self { messages }
    }

    /// 追加系统消息
    pub fn push_system(&mut self, content: impl Into<String>) {
        self.messages
            .push(ConversationMessage::system(content.into()));
    }

    /// 追加用户消息
    pub fn push_user(&mut self, content: impl Into<String>) {
        self.messages
            .push(ConversationMessage::user(content.into()));
    }

    /// 追加助手消息
    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.messages
            .push(ConversationMessage::assistant(content.into()));
    }

    /// 追加包含工具调用的助手消息
    ///
    /// # Parameters
    ///
    /// * `content` - 文本内容（可为空）
    /// * `tool_calls` - LLM 决策的工具调用列表
    pub fn push_assistant_with_tool_calls(
        &mut self,
        content: impl Into<String>,
        tool_calls: Vec<PlannedToolCall>,
    ) {
        self.messages.push(ConversationMessage {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_name: None,
            tool_call_id: None,
            tool_calls: Some(tool_calls),
            metadata: None,
        });
    }

    /// 追加工具结果消息
    ///
    /// # Parameters
    ///
    /// * `tool_name` - 工具唯一编码
    /// * `tool_call_id` - 工具调用 ID
    /// * `content` - 工具返回内容
    pub fn push_tool_result(
        &mut self,
        tool_name: impl Into<String>,
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
    ) {
        self.messages.push(ConversationMessage::tool_result(
            tool_name,
            tool_call_id,
            content,
        ));
    }

    /// 追加任意构造好的对话消息
    pub fn push(&mut self, message: ConversationMessage) {
        self.messages.push(message);
    }

    /// 批量追加对话消息
    ///
    /// 适用于把外部已经构造好的 [`ConversationMessage`] 列表（如会话级
    /// 上下文回放）一次性合并到当前上下文，避免逐条拆字段重组。
    pub fn extend<I>(&mut self, messages: I)
    where
        I: IntoIterator<Item = ConversationMessage>,
    {
        self.messages.extend(messages);
    }

    /// 以只读切片暴露内部消息列表
    pub fn as_slice(&self) -> &[ConversationMessage] {
        &self.messages
    }

    /// 当前消息条数
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// 上下文是否为空
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// 消费自身并返回底层消息列表
    pub fn into_messages(self) -> Vec<ConversationMessage> {
        self.messages
    }

    /// 将当前上下文转换为 rig `Message` 列表
    ///
    /// 工具结果消息以 rig 的 `UserContent::ToolResult` 形式表示；
    /// 若消息未显式携带 `tool_call_id`，将生成一个 UUID 填充。
    pub fn to_rig_messages(&self) -> Vec<Message> {
        self.messages.iter().map(to_rig_message).collect()
    }
}

/// 将单条 [`ConversationMessage`] 转换为 rig [`Message`]
///
/// 当 `Assistant` 角色消息携带 `tool_calls` 时，每个 `PlannedToolCall`
/// 被映射为 rig 的 `AssistantContent::ToolCall`；若有文本内容，
/// 则文本作为 `AssistantContent::Text` 一并加入 `content` 列表。
fn to_rig_message(msg: &ConversationMessage) -> Message {
    match msg.role {
        MessageRole::System => Message::System {
            content: msg.content.clone(),
        },
        MessageRole::User => Message::User {
            content: vec![UserContent::Text(Text::new(msg.content.clone()))],
        },
        MessageRole::Assistant => {
            let mut contents: Vec<AssistantContent> = Vec::new();

            // 若有文本内容，先加入 Text
            if !msg.content.is_empty() {
                contents.push(AssistantContent::Text(Text::new(msg.content.clone())));
            }

            // 若有 tool_calls，逐一映射为 AssistantContent::ToolCall
            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    // arguments_json 是 JSON 字符串，需要解析为 serde_json::Value
                    let arguments = serde_json::from_str(&tc.arguments_json)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    contents.push(AssistantContent::ToolCall(ToolCall::new(
                        CallId::from_wire(tc.call_id.clone()),
                        ToolFunction::new(tool_name_of(&tc.tool_name), arguments),
                    )));
                }
            }

            // 若两者都为空，至少保留一个空 Text，保证 assistant 轮有内容块
            if contents.is_empty() {
                contents.push(AssistantContent::Text(Text::new(String::new())));
            }

            // `AssistantMessage` 是 non_exhaustive 结构体：先取默认值再写入内容块
            let mut assistant = AssistantMessage::default();
            assistant.content = contents;
            Message::Assistant(assistant)
        }
        MessageRole::Tool => {
            let tool_call_id = msg
                .tool_call_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string());

            Message::User {
                content: vec![UserContent::ToolResult(ToolResult {
                    call: CallId::from_wire(tool_call_id),
                    name: tool_name_of(msg.tool_name.as_deref().unwrap_or("")),
                    content: vec![ToolResultContent::Text(Text::new(msg.content.clone()))],
                    is_error: false,
                })],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_len() {
        let mut ctx = MessageContext::new();
        assert!(ctx.is_empty());

        ctx.push_system("sys");
        ctx.push_user("hi");
        ctx.push_assistant("hello");
        ctx.push_tool_result("echo", "call-1", "ok");

        assert_eq!(ctx.len(), 4);
        assert!(!ctx.is_empty());
    }

    #[test]
    fn extend_appends_all_messages() {
        let mut ctx = MessageContext::new();
        ctx.push_user("first");

        ctx.extend([
            ConversationMessage::assistant("second"),
            ConversationMessage::tool_result("echo", "call-1", "third"),
        ]);

        assert_eq!(ctx.len(), 3);
        assert_eq!(ctx.as_slice()[1].role, MessageRole::Assistant);
        assert_eq!(ctx.as_slice()[2].role, MessageRole::Tool);
    }

    #[test]
    fn to_rig_messages_covers_all_roles() {
        let mut ctx = MessageContext::new();
        ctx.push_system("sys");
        ctx.push_user("hi");
        ctx.push_assistant("hello");
        ctx.push_tool_result("echo", "call-1", "ok");

        let rig_messages = ctx.to_rig_messages();
        assert_eq!(rig_messages.len(), 4);

        matches!(rig_messages[0], Message::System { .. });
        matches!(rig_messages[1], Message::User { .. });
        matches!(rig_messages[2], Message::Assistant(_));
        matches!(rig_messages[3], Message::User { .. });
    }

    #[test]
    fn tool_result_without_id_gets_generated() {
        let msg = ConversationMessage {
            role: MessageRole::Tool,
            content: "ok".into(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            metadata: None,
        };
        let ctx = MessageContext::from_messages(vec![msg]);
        let rig_messages = ctx.to_rig_messages();
        assert_eq!(rig_messages.len(), 1);
        matches!(rig_messages[0], Message::User { .. });
    }
}
