//! 基于数据库的会话服务实现
//!
//! [`DbSessionService`] 实现 [`agent_runtime::session::SessionService`] trait，
//! 将会话事件流与三层上下文记忆全部持久化到数据库。
//!
//! # 设计要点
//!
//! - **完全无状态**：服务不再绑定任何 `(agent_id, user_id, session_id)` 元组，
//!   所有写入所需的标识全部由调用方通过 [`SessionEventRecord`] 携带。
//! - **会话主表创建**：`t_agent_session` 主表的创建由
//!   [`SessionService::create_session`] 提供；`agent-runtime` 的
//!   `LlmAgent::run_stream` 会在每次运行入口先查询会话状态，
//!   仅在会话不存在时调用 `create_session` 直接创建，避免重复查询。
//! - **职责拆分**：本服务是组合层，所有数据操作下沉到 [`SessionDao`] /
//!   [`SessionEventDao`] / [`SessionStateDao`]，本模块只做接口适配与字段映射。
//! - **写入入口收敛**：所有事件追加汇聚到
//!   [`SessionService::append_record`]；事件类型字符串与 `run_id` 提取由
//!   `agent-runtime` 的 [`AgentStreamEvent::event_type`] /
//!   [`AgentStreamEvent::run_id`] 提供。
//! - **上下文按序追加**：`append_context` 将每条 [`ConversationMessage`] 序列化为
//!   一条业务 JSON，调用 [`SessionStateDao::append`] 追加；`list_context`
//!   按 `id` 升序读取并反序列化。

use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::DatabaseConnection;

use agent_runtime::{
    error::{AgentResult, Error},
    message::ConversationMessage,
    session::SessionService,
    types::session::{ContextScope, SessionEventRecord, SessionStatus},
};

use crate::session::{
    NewSession, NewSessionEvent, PendingInputDao, SessionDao, SessionEventDao, SessionStateDao,
};

/// 基于数据库的会话服务
///
/// 通过组合 [`SessionDao`] / [`SessionEventDao`] / [`SessionStateDao`]
/// 三个 DAO，把 [`SessionService`] 的所有方法落地到数据库。
///
/// 本服务**完全无状态**，可以在进程内安全共享为单例（通常通过 `Arc` 注入到
/// 多个 `DbAgentBuilder`），不再持有任何会话级标识。
#[derive(Clone)]
pub struct DbSessionService {
    session_dao: SessionDao,
    session_event_dao: SessionEventDao,
    session_state_dao: SessionStateDao,
    pending_input_dao: PendingInputDao,
}

impl DbSessionService {
    /// 创建会话服务实例
    ///
    /// # Parameters
    ///
    /// * `db` - 数据库连接
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self {
            session_dao: SessionDao::new(db.clone()),
            session_event_dao: SessionEventDao::new(db.clone()),
            session_state_dao: SessionStateDao::new(db.clone()),
            pending_input_dao: PendingInputDao::new(db),
        }
    }
}

#[async_trait]
impl SessionService for DbSessionService {
    /// 获取会话当前状态（纯查询，不创建不更新）
    ///
    /// 由 `LlmAgent::run_stream_inner` 在判断是否需要创建会话时调用。
    /// 实现：直接委托 [`SessionDao::get`]，并将数据库中的 i16 状态转换为
    /// `BaseSessionStatus`。
    async fn get_session(
        &self,
        session_id: &str,
        _agent_id: i64,
        _user_id: &str,
    ) -> AgentResult<Option<SessionStatus>> {
        let model = self
            .session_dao
            .get(session_id)
            .await
            .map_err(|e| Error::Internal(format!("查询会话失败: {e}")))?;
        Ok(model.map(|m| SessionStatus::from_raw(m.status)))
    }

    /// 创建新会话（仅创建，不查询不更新）
    ///
    /// 由 `LlmAgent::run_stream_inner` 在确认会话不存在后调用，
    /// 直接创建新会话记录，避免重复查询。
    /// 实现：直接委托 [`SessionDao::create`]。
    async fn create_session(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> AgentResult<()> {
        self.session_dao
            .create(NewSession {
                session_id: session_id.to_string(),
                agent_id,
                user_id: user_id.to_string(),
                title: None,
                expires_at: None,
            })
            .await
            .map_err(|e| Error::Internal(format!("创建会话失败: {e}")))?;
        Ok(())
    }

    /// 更新会话状态与活跃时间
    ///
    /// 由 `LlmAgent::run_stream_inner` 在确认会话已存在时调用，
    /// 更新 `status` 与 `last_event_at`。实现：委托 [`SessionDao::update_session`]。
    async fn update_session(
        &self,
        session_id: &str,
        _agent_id: i64,
        _user_id: &str,
        status: Option<SessionStatus>,
    ) -> AgentResult<()> {
        self.session_dao
            .update_status(session_id, status)
            .await
            .map_err(|e| Error::Internal(format!("更新会话状态失败: {e}")))?;
        Ok(())
    }
    /// 唯一的事件写入入口
    ///
    /// 直接落事件表，不再做任何主表查询（由 [`Self::create_session`] 负责）。
    async fn append_record(&self, record: SessionEventRecord<'_>) -> AgentResult<()> {
        self.session_event_dao
            .append(NewSessionEvent {
                session_id: record.session_id.to_string(),
                agent_id: record.agent_id,
                user_id: record.user_id.to_string(),
                run_id: record.run_id.map(String::from),
                event_type: record.event_type.to_string(),
                payload_json: record.payload_json,
                expires_at: None,
            })
            .await
            .map_err(|e| Error::Internal(format!("追加事件失败: {e}")))?;
        Ok(())
    }

    async fn append_context(
        &self,
        scope: ContextScope<'_>,
        message: &ConversationMessage,
    ) -> AgentResult<()> {
        let content_json = serde_json::to_string(message)
            .map_err(|e| Error::Internal(format!("序列化上下文消息失败: {e}")))?;

        self.session_state_dao
            .append(scope, Some(content_json), None)
            .await
            .map_err(|e| Error::Internal(format!("追加上下文消息失败: {e}")))?;
        Ok(())
    }

    async fn list_raw_context(&self, scope: ContextScope<'_>) -> AgentResult<Vec<String>> {
        let entries = self
            .session_state_dao
            .list(scope)
            .await
            .map_err(|e| Error::Internal(format!("加载原始上下文条目失败: {e}")))?;

        Ok(entries
            .into_iter()
            .filter_map(|entry| entry.content_json)
            .collect())
    }

    /// 向指定会话的注入队列追加一条待消费消息
    ///
    /// 将消息序列化后写入 `t_agent_session_pending_input` 队列表。
    async fn enqueue_pending_input(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        message: &ConversationMessage,
    ) -> AgentResult<()> {
        let content_json = serde_json::to_string(message)
            .map_err(|e| Error::Internal(format!("序列化注入消息失败: {e}")))?;

        self.pending_input_dao
            .enqueue(session_id, agent_id, user_id, content_json)
            .await
            .map_err(|e| Error::Internal(format!("写入注入队列失败: {e}")))?;
        Ok(())
    }

    /// 消费指定会话的全部未消费注入消息
    ///
    /// 读取并标记已消费，返回反序列化后的消息列表。
    async fn drain_pending_input(&self, session_id: &str) -> AgentResult<Vec<ConversationMessage>> {
        let entries = self
            .pending_input_dao
            .drain(session_id)
            .await
            .map_err(|e| Error::Internal(format!("消费注入队列失败: {e}")))?;

        let mut messages = Vec::with_capacity(entries.len());
        for entry in entries {
            match serde_json::from_str::<ConversationMessage>(&entry.content_json) {
                Ok(msg) => messages.push(msg),
                Err(e) => {
                    tracing::warn!(
                        entry_id = entry.id,
                        error = %e,
                        "跳过无法反序列化的注入消息",
                    );
                }
            }
        }
        Ok(messages)
    }
}
