//! `InMemorySessionService` —— 内存版会话服务
//!
//! 主要用于测试与示例场景，提供 [`SessionService`] 的内存实现。

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::AgentResult;
use crate::message::ConversationMessage;

use crate::types::session::{ContextScope, SessionEventRecord, SessionStatus};

use super::{event::AgentStreamEvent, traits::SessionService};

/// 会话（一等公民）
///
/// 持有事件历史、会话状态和元数据，
/// 是事件存储与状态管理的统一载体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// 会话 ID
    pub id: String,
    /// 智能体 ID
    pub agent_id: i64,
    /// 用户 ID
    pub user_id: String,
    /// 会话状态
    pub status: SessionStatus,
    /// 事件历史
    pub events: Vec<AgentStreamEvent>,
    /// 累计消息数
    pub message_count: u32,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
}

impl Session {
    /// 创建新会话实例
    ///
    /// # Parameters
    ///
    /// * `id` - 会话 ID
    /// * `agent_id` - 智能体 ID
    /// * `user_id` - 用户 ID
    pub fn new(id: impl Into<String>, agent_id: i64, user_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            agent_id,
            user_id: user_id.into(),
            status: SessionStatus::Idle,
            events: Vec::new(),
            message_count: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 内存会话服务（用于测试）
pub struct InMemorySessionService {
    sessions: RwLock<HashMap<String, Session>>,
    /// session 级上下文池：`session_id` → 该会话的上下文消息流
    session_contexts: RwLock<HashMap<String, Vec<ConversationMessage>>>,
    /// 用户级上下文池：`user_id` → 原始 JSON 字符串列表（蒸馏记忆）
    user_contexts: RwLock<HashMap<String, Vec<String>>>,
    /// agent 私有上下文池：`(agent_id, user_id)` → 原始 JSON 字符串列表（蒸馏记忆）
    agent_contexts: RwLock<HashMap<(i64, String), Vec<String>>>,
    /// 注入消息队列：`session_id` → 待消费的消息列表
    pending_inputs: RwLock<HashMap<String, Vec<ConversationMessage>>>,
}

impl InMemorySessionService {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            session_contexts: RwLock::new(HashMap::new()),
            user_contexts: RwLock::new(HashMap::new()),
            agent_contexts: RwLock::new(HashMap::new()),
            pending_inputs: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemorySessionService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionService for InMemorySessionService {
    /// 获取会话当前状态（纯查询，不创建不更新）
    async fn get_session(
        &self,
        session_id: &str,
        _agent_id: i64,
        _user_id: &str,
    ) -> AgentResult<Option<SessionStatus>> {
        let store = self.sessions.read().await;
        Ok(store.get(session_id).map(|s| s.status))
    }

    /// 创建新会话（仅创建，不查询不更新）
    async fn create_session(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> AgentResult<()> {
        let mut store = self.sessions.write().await;
        store.insert(
            session_id.to_string(),
            Session::new(session_id, agent_id, user_id),
        );
        Ok(())
    }

    /// 更新会话状态与活跃时间
    async fn update_session(
        &self,
        session_id: &str,
        _agent_id: i64,
        _user_id: &str,
        status: Option<SessionStatus>,
    ) -> AgentResult<()> {
        let mut store = self.sessions.write().await;
        if let Some(session) = store.get_mut(session_id) {
            if let Some(s) = status {
                session.status = s;
            }
            session.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn append_record(&self, record: SessionEventRecord<'_>) -> AgentResult<()> {
        // 内存实现仅尝试将"形如 AgentStreamEvent 的 payload"重新解析后追加到
        // 会话事件流中；其它类型事件（memory_* / run_record_*）在内存场景没有读取需求，
        // 直接忽略以保持实现极简。
        let mut store = self.sessions.write().await;
        if let Some(session) = store.get_mut(record.session_id) {
            if let Ok(event) = serde_json::from_str::<AgentStreamEvent>(&record.payload_json) {
                session.events.push(event);
            }
            session.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn append_context(
        &self,
        scope: ContextScope<'_>,
        message: &ConversationMessage,
    ) -> AgentResult<()> {
        match scope {
            ContextScope::Session { session_id, .. } => {
                let mut pool = self.session_contexts.write().await;
                pool.entry(session_id.to_string())
                    .or_default()
                    .push(message.clone());
            }
            ContextScope::User { user_id } => {
                let json = serde_json::to_string(message)
                    .map_err(|e| crate::Error::Internal(format!("序列化消息失败: {e}")))?;
                let mut pool = self.user_contexts.write().await;
                pool.entry(user_id.to_string()).or_default().push(json);
            }
            ContextScope::Agent { agent_id, user_id } => {
                let json = serde_json::to_string(message)
                    .map_err(|e| crate::Error::Internal(format!("序列化消息失败: {e}")))?;
                let mut pool = self.agent_contexts.write().await;
                pool.entry((agent_id, user_id.to_string()))
                    .or_default()
                    .push(json);
            }
        }
        Ok(())
    }

    async fn list_raw_context(&self, scope: ContextScope<'_>) -> AgentResult<Vec<String>> {
        match scope {
            ContextScope::Session { session_id, .. } => {
                let pool = self.session_contexts.read().await;
                let msgs = pool.get(session_id).cloned().unwrap_or_default();
                msgs.iter()
                    .map(|m| {
                        serde_json::to_string(m)
                            .map_err(|e| crate::Error::Internal(format!("序列化消息失败: {e}")))
                    })
                    .collect()
            }
            ContextScope::User { user_id } => {
                let pool = self.user_contexts.read().await;
                Ok(pool.get(user_id).cloned().unwrap_or_default())
            }
            ContextScope::Agent { agent_id, user_id } => {
                let pool = self.agent_contexts.read().await;
                Ok(pool
                    .get(&(agent_id, user_id.to_string()))
                    .cloned()
                    .unwrap_or_default())
            }
        }
    }

    async fn enqueue_pending_input(
        &self,
        session_id: &str,
        _agent_id: i64,
        _user_id: &str,
        message: &ConversationMessage,
    ) -> AgentResult<()> {
        let mut pool = self.pending_inputs.write().await;
        pool.entry(session_id.to_string())
            .or_default()
            .push(message.clone());
        Ok(())
    }

    async fn drain_pending_input(&self, session_id: &str) -> AgentResult<Vec<ConversationMessage>> {
        let mut pool = self.pending_inputs.write().await;
        Ok(pool.remove(session_id).unwrap_or_default())
    }
}
