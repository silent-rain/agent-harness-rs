//! 三粒度上下文聚合视图
//!
//! 提供的主要内容：
//! - [`ScopedContexts`] - 三粒度（agent / user / session）上下文聚合视图

use tracing::warn;

use crate::message::{ConversationMessage, DistilledMemory};

/// 三粒度上下文聚合视图
///
/// 由 [`RuntimeServices::list_all_contexts`](crate::runtime::RuntimeServices::list_all_contexts)
/// 一次性产出，按粒度分桶：
///
/// - [`Self::agent`] —— agent 私有蒸馏记忆（反序列化后的 [`DistilledMemory`] 列表）
/// - [`Self::user`]  —— 用户级蒸馏记忆（反序列化后的 [`DistilledMemory`] 列表）
/// - [`Self::session`] —— 当前会话级上下文（原始对话消息）
///
/// Agent / User 粒度仅完成反序列化，由调用方（如 `inject_long_term_memory`）
/// 决定如何聚合并转换为 LLM 上下文消息。
///
/// 所有字段均已完成反序列化，调用方可直接使用。
#[derive(Debug, Default, Clone)]
pub struct ScopedContexts {
    /// agent 私有蒸馏记忆（反序列化后的条目列表）
    pub agent: Vec<DistilledMemory>,
    /// 用户级蒸馏记忆（反序列化后的条目列表）
    pub user: Vec<DistilledMemory>,
    /// 会话级上下文（按写入顺序升序）
    pub session: Vec<ConversationMessage>,
}

impl ScopedContexts {
    /// 三粒度是否全为空
    pub fn is_empty(&self) -> bool {
        self.agent.is_empty() && self.user.is_empty() && self.session.is_empty()
    }
}

// ─── Deserialization helpers ───────────────────────────────────

/// Deserialize User / Agent scoped raw JSON list into [`DistilledMemory`].
///
/// Entries that fail deserialization are skipped with a warning.
///
/// # Parameters
///
/// * `raw_jsons` - raw JSON string list
/// * `scope_label` - scope label (`"agent"` / `"user"`) for log identification
pub(crate) fn deserialize_distilled_memories(
    raw_jsons: Vec<String>,
    scope_label: &str,
) -> Vec<DistilledMemory> {
    let mut memories = Vec::with_capacity(raw_jsons.len());
    for (idx, json) in raw_jsons.iter().enumerate() {
        match serde_json::from_str::<DistilledMemory>(json) {
            Ok(memory) => memories.push(memory),
            Err(e) => {
                warn!(
                    scope = scope_label,
                    index = idx,
                    error = %e,
                    "跳过无法反序列化的蒸馏记忆条目"
                );
            }
        }
    }
    memories
}

/// Deserialize Session scoped raw JSON list into [`ConversationMessage`].
///
/// Entries that fail deserialization are skipped with a warning.
///
/// # Parameters
///
/// * `raw_jsons` - raw JSON string list
pub(crate) fn convert_session_messages(raw_jsons: Vec<String>) -> Vec<ConversationMessage> {
    let mut messages = Vec::with_capacity(raw_jsons.len());
    for json in &raw_jsons {
        match serde_json::from_str::<ConversationMessage>(json) {
            Ok(msg) => messages.push(msg),
            Err(e) => {
                warn!(error = %e, "跳过无法反序列化的会话上下文条目");
            }
        }
    }
    messages
}
