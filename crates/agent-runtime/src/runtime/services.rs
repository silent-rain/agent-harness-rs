//! 运行时服务聚合
//!
//! 将各类运行时依赖聚合到 [`RuntimeServices`]，方便按职责定位和未来扩展：
//!
//! - `session`：会话生命周期、事件流、状态、运行记录、工具审计、上下文记忆（统一接管）
//! - `knowledge`：外部知识库检索（RAG）
//!
//! 所有字段均有 Noop 默认实现，可按需替换。
//!
//! 同时提供一组**容错便捷方法**（`try_append_event` / `try_append_context` /
//! `list_all_contexts`），用于在主流程中以"写入/读取失败仅警告、不阻断主链路"
//! 的方式落地可观测性数据，避免在调用方重复书写
//! `if let Err(e) = ... warn!(...)` 模板。

use std::sync::Arc;

use tokio::join;
use tracing::warn;

use crate::{
    error::AgentResult,
    knowledge::{AgentKnowledgeRetriever, NoopKnowledgeRetriever},
    message::ConversationMessage,
    session::{
        AgentStreamEvent, InMemorySessionService, ScopedContexts, SharedSessionService,
        scoped_contexts::{convert_session_messages, deserialize_distilled_memories},
    },
    types::session::{ContextScope, SessionStatus},
};

/// 运行时服务聚合
///
/// 聚合所有可选的运行时依赖，简化 `Agent` 的构造。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_runtime::RuntimeServices;
///
/// // 全部 Noop
/// let services = RuntimeServices::noop();
///
/// // 仅自定义 session
/// # use agent_runtime::session::InMemorySessionService;
/// # use std::sync::Arc;
/// # fn custom_session() -> Arc<InMemorySessionService> { Arc::new(InMemorySessionService::new()) }
/// let services = RuntimeServices {
///     session: custom_session(),
///     ..RuntimeServices::noop()
/// };
/// ```
#[derive(Clone)]
pub struct RuntimeServices {
    /// 会话服务（包含事件、状态、可观测性）
    pub session: SharedSessionService,
    /// 外部知识库检索器（RAG）
    pub knowledge: Arc<dyn AgentKnowledgeRetriever>,
}

impl RuntimeServices {
    /// 创建全部使用 Noop 实现的默认服务
    ///
    /// 适用于测试和不需要持久化的场景。
    pub fn noop() -> Self {
        Self {
            session: Arc::new(InMemorySessionService::new()),
            knowledge: Arc::new(NoopKnowledgeRetriever),
        }
    }

    // ── 可观测性便捷方法（容错写入） ──────────────────────────────

    /// 获取会话当前状态（纯查询，失败向上传播）
    ///
    /// 调用底层 [`SessionService::get_session`] 获取会话状态。
    /// 返回 `None` 表示会话不存在，`Some(status)` 表示会话当前状态。
    /// 与 [`Self::create_session`] 不同，此方法不创建也不更新。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    pub async fn get_session(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> AgentResult<Option<SessionStatus>> {
        self.session
            .get_session(session_id, agent_id, user_id)
            .await
    }

    /// 创建新会话（失败向上传播）
    ///
    /// 调用底层 [`SessionService::create_session`] 直接创建会话主表记录。
    /// 仅在 [`Self::get_session`] 返回 `None` 后调用，避免重复查询。
    ///
    /// 与其他容错便捷方法不同，此方法**不吞错**：会话主表是后续事件 /
    /// 上下文 / 工具审计写入与查询的外键依赖，缺失会让整个运行链路退化
    /// 为只读流水线。失败时由调用方决定退出策略（通常是发布
    /// `RunFinished{success:false}` 后终止 `run_stream`）。
    ///
    /// 由 `Agent` 在 `run_stream` 入口处一次性触发，调用方无需在装配阶段
    /// 单独调用，从而支持同一个 `Agent` 实例跨多个 `session_id` 复用。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    pub async fn create_session(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> AgentResult<()> {
        self.session
            .create_session(session_id, agent_id, user_id)
            .await
    }

    /// 更新会话状态与活跃时间（失败向上传播）
    ///
    /// 调用底层 [`SessionService::update_session`] 更新会话主表的
    /// `status` 与 `last_event_at`。仅在确认会话已存在时调用。
    /// `last_event_at` 会被自动设置为当前时间。
    ///
    /// 与 [`Self::ensure_session`] 不同，此方法**不创建**新会话，
    /// 仅更新已有会话的状态与时间。失败时向上传播错误。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    /// * `status`     - 目标状态（传入 `None` 表示仅刷新时间，不修改状态）
    pub async fn update_session(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        status: Option<SessionStatus>,
    ) -> AgentResult<()> {
        self.session
            .update_session(session_id, agent_id, user_id, status)
            .await
    }

    /// 容错追加事件（失败仅警告，不阻断主流程）
    ///
    /// 所有运行生命周期事件（`RunStarted` / `RunFinished` 等）均通过此方法
    /// 落入会话事件表。调用方需要先把事件构造好；写入失败时仅记录 `warn!`，
    /// 主流程继续执行。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID（落库到 `t_agent_session_event.agent_id`）
    /// * `user_id`    - 用户 ID（落库到 `t_agent_session_event.user_id`）
    /// * `event`      - 待落库的事件
    pub async fn try_append_event(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        event: &AgentStreamEvent,
    ) {
        if let Err(e) = self
            .session
            .append_event(session_id, agent_id, user_id, event)
            .await
        {
            warn!(
                session_id = %session_id,
                event_type = %event.event_type(),
                error = %e,
                "追加会话事件失败"
            );
        }
    }

    /// 容错追加一条上下文消息（失败仅警告，不阻断主流程）
    ///
    /// 上下文消息按三粒度作用域（Session / User / Agent）落入对应的
    /// 上下文记忆存储，作为后续 LLM 请求的 messages 来源。写入失败时
    /// 仅记录 `warn!`，主流程继续执行。
    ///
    /// # Parameters
    ///
    /// * `scope` - 上下文作用域
    /// * `message` - 上下文消息条目
    pub async fn try_append_context(&self, scope: ContextScope<'_>, message: &ConversationMessage) {
        if let Err(e) = self.session.append_context(scope, message).await {
            warn!(role = ?message.role, error = %e, "追加上下文消息失败");
        }
    }

    /// 容错聚合读取三粒度上下文（任一粒度失败仅警告，对应字段返回空 Vec）
    ///
    /// 一次性读取 [`ContextScope::Agent`] / [`ContextScope::User`] /
    /// [`ContextScope::Session`] 三个粒度的上下文消息，便于调用方按差异化
    /// 策略拼接 LLM 请求 messages（典型用法：长期记忆段落 + 会话历史回放）。
    ///
    /// **反序列化策略**（按粒度差异化处理）：
    ///
    /// - **User / Agent 粒度**：原始 JSON 反序列化为 [`DistilledMemory`]，
    ///   由调用方（如 `inject_long_term_memory`）决定如何转换为 LLM 消息。
    /// - **Session 粒度**：原始 JSON 为 [`ConversationMessage`] 格式，
    ///   直接反序列化为 [`ConversationMessage`]。
    ///
    /// 反序列化失败的条目仅记录警告并跳过，不影响其他条目。
    ///
    /// 三路读取并发执行；任一粒度抛错时仅记录 `warn!`，对应字段以空 Vec
    /// 返回，确保单一粒度的故障不会让整个上下文加载失败。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID（用于 `Session` 粒度寻址）
    /// * `agent_id`   - 智能体 ID（用于 `Agent` / `Session` 粒度寻址）
    /// * `user_id`    - 用户 ID（用于 `User` / `Agent` / `Session` 粒度寻址）
    pub async fn list_all_contexts(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> ScopedContexts {
        let agent_fut = self
            .session
            .list_raw_context(ContextScope::Agent { agent_id, user_id });
        let user_fut = self
            .session
            .list_raw_context(ContextScope::User { user_id });
        let session_fut = self.session.list_raw_context(ContextScope::Session {
            session_id,
            agent_id,
            user_id,
        });

        let (agent_res, user_res, session_res) = join!(agent_fut, user_fut, session_fut);

        let agent = agent_res
            .map(|raw| deserialize_distilled_memories(raw, "agent"))
            .unwrap_or_else(|e| {
                warn!(agent_id, user_id, error = %e, "加载 agent 级上下文失败");
                Vec::new()
            });

        let user = user_res
            .map(|raw| deserialize_distilled_memories(raw, "user"))
            .unwrap_or_else(|e| {
                warn!(user_id, error = %e, "加载用户级上下文失败");
                Vec::new()
            });

        let session = session_res
            .map(convert_session_messages)
            .unwrap_or_else(|e| {
                warn!(session_id, error = %e, "加载会话级上下文失败");
                Vec::new()
            });

        ScopedContexts {
            agent,
            user,
            session,
        }
    }

    /// 向指定会话的注入队列追加一条待消费消息（容错）
    ///
    /// 由接口层调用，将消息写入 `t_agent_session_pending_input` 队列表。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    /// * `message`    - 待注入的消息
    pub async fn try_enqueue_pending_input(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        message: &ConversationMessage,
    ) {
        if let Err(e) = self
            .session
            .enqueue_pending_input(session_id, agent_id, user_id, message)
            .await
        {
            warn!(session_id, error = %e, "写入注入队列失败");
        }
    }

    /// 消费指定会话的全部未消费注入消息（容错）
    ///
    /// 由 `drive_react_loop` 每轮开头调用，读取并标记已消费。
    /// 返回的消息应由调用方同步固化到 `t_agent_session_state`。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    ///
    /// # Returns
    ///
    /// 未消费的消息列表（按写入顺序，FIFO）；失败时返回空列表。
    pub async fn drain_pending_input(&self, session_id: &str) -> Vec<ConversationMessage> {
        match self.session.drain_pending_input(session_id).await {
            Ok(msgs) => msgs,
            Err(e) => {
                warn!(session_id, error = %e, "消费注入队列失败");
                Vec::new()
            }
        }
    }
}

impl Default for RuntimeServices {
    fn default() -> Self {
        Self::noop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_services_created() {
        let services = RuntimeServices::noop();
        assert!(Arc::strong_count(&services.session) > 0);
        assert!(Arc::strong_count(&services.knowledge) > 0);
    }

    #[test]
    fn test_partial_override_with_struct_update() {
        let services = RuntimeServices {
            session: Arc::new(InMemorySessionService::new()),
            ..RuntimeServices::noop()
        };
        assert!(Arc::strong_count(&services.knowledge) > 0);
    }
}
