//! 会话服务 trait 定义
//!
//! 定义 [`SessionService`] trait，统一会话事件追加和上下文记忆管理的写入入口，
//! 以及共享类型别名 [`SharedSessionService`]。

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{AgentResult, Error};
use crate::message::ConversationMessage;
use crate::types::session::{ContextScope, SessionEventRecord, SessionStatus};

use super::event::AgentStreamEvent;

/// 会话服务 trait
///
/// 统一管理会话的事件追加和上下文记忆管理。
///
/// # 写入入口收敛
///
/// 所有"追加事件"语义最终都汇聚到 [`SessionService::append_record`]，
/// 这是**唯一**需要新实现覆盖的写入方法。基于此入口，trait 提供了
/// [`SessionService::append_event`] 作为便捷入口。
///
/// 运行生命周期事件（`run_started` / `run_finished`）统一以
/// [`AgentStreamEvent`] 的形式通过 [`SessionService::append_event`] 写入。
///
/// 三元组询取/写入接口使用强类型参数：
/// - `session_id: &str` - 会话 ID
/// - `scope: ContextScope<'_>` - 上下文作用域
#[async_trait]
pub trait SessionService: Send + Sync + 'static {
    // ── 三层上下文记忆（一读一写） ──

    /// 追加一条上下文消息到指定作用域
    ///
    /// 区别于 [`Self::append_record`]（流水事件），本方法专用于持久化
    /// 「可回放为 LLM messages」的对话上下文。三种作用域分别落入不同的
    /// 上下文存储表，由实现侧决定。
    ///
    /// # Parameters
    ///
    /// * `scope` - 三粒度作用域（Session / User / Agent）
    /// * `message` - 上下文消息条目
    async fn append_context(
        &self,
        scope: ContextScope<'_>,
        message: &ConversationMessage,
    ) -> AgentResult<()>;

    /// 列出指定作用域下的全部原始 JSON 条目（按写入顺序升序）
    ///
    /// 直接返回数据库中存储的原始 JSON 字符串，**不做反序列化**。
    /// 不同粒度的数据使用不同的序列化结构：
    ///
    /// - **Session 粒度**：存储 [`ConversationMessage`] JSON
    /// - **User / Agent 粒度**：存储 [`DistilledMemory`] JSON
    ///
    /// 由上层调用方根据粒度自行决定反序列化策略。
    ///
    /// # Parameters
    ///
    /// * `scope` - 三粒度作用域
    ///
    /// # Returns
    ///
    /// 原始 JSON 字符串列表（跳过 null 条目）
    async fn list_raw_context(&self, scope: ContextScope<'_>) -> AgentResult<Vec<String>>;

    // ── 注入消息队列 ──

    /// 向指定会话的注入队列追加一条待消费消息
    ///
    /// 由接口层调用，将消息写入 `t_agent_session_pending_input` 队列表，
    /// 等待 `drive_react_loop` 下一轮循环消费。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    /// * `message`    - 待注入的消息
    async fn enqueue_pending_input(
        &self,
        _session_id: &str,
        _agent_id: i64,
        _user_id: &str,
        _message: &ConversationMessage,
    ) -> AgentResult<()> {
        Ok(())
    }

    /// 消费（drain）指定会话的全部未消费注入消息
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
    /// 未消费的消息列表（按写入顺序，FIFO）
    async fn drain_pending_input(
        &self,
        _session_id: &str,
    ) -> AgentResult<Vec<ConversationMessage>> {
        Ok(Vec::new())
    }
    // ── 唯一的事件写入入口 ──

    /// 追加一条会话事件到底层存储
    ///
    /// 这是**唯一**需要实现覆盖的写入方法。便捷方法
    /// [`append_event`](Self::append_event) 都会落到此入口。
    ///
    /// # Parameters
    ///
    /// * `record` - 事件写入载荷
    async fn append_record(&self, record: SessionEventRecord<'_>) -> AgentResult<()>;

    // ── 会话主表幂等保障 ──

    /// 获取会话当前状态（纯查询，不创建不更新）
    ///
    /// 返回 `None` 表示该会话不存在，`Some(status)` 表示会话存在并返回当前状态。
    /// 由 `LlmAgent::run_stream_inner` 在判断是否需要创建会话时调用。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    ///
    /// # Returns
    ///
    /// `None` 表示会话不存在，`Some(SessionStatus)` 表示会话当前状态
    async fn get_session(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> AgentResult<Option<SessionStatus>>;

    /// 创建新会话（仅创建，不查询不更新）
    ///
    /// 仅在 [`Self::get_session`] 返回 `None` 后调用，直接创建新会话记录，
    /// 避免重复查询。新会话的初始状态为 [`SessionStatus::Running`]（直接进入运行态）。
    ///
    /// 与 [`Self::get_session`] 配合使用：
    /// - `get_session` 返回 `None` → 调用 `create_session` 创建
    /// - `get_session` 返回 `Some(_)` → 调用 `update_session` 更新状态与时间
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    async fn create_session(
        &self,
        _session_id: &str,
        _agent_id: i64,
        _user_id: &str,
    ) -> AgentResult<()> {
        Ok(())
    }

    /// 更新会话状态与活跃时间
    ///
    /// 更新会话主表的 `status` 与 `last_event_at` 字段。
    /// `last_event_at` 会被自动设置为当前时间。
    /// 由 `LlmAgent::run_stream_inner` 在确认会话已存在时调用。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    /// * `status`     - 目标状态（传入 `None` 表示仅刷新时间，不修改状态）
    async fn update_session(
        &self,
        _session_id: &str,
        _agent_id: i64,
        _user_id: &str,
        _status: Option<SessionStatus>,
    ) -> AgentResult<()> {
        Ok(())
    }

    // ── 便捷方法（基于 append_record 组合） ──

    /// 追加一条 [`AgentStreamEvent`] 到事件流（便捷方法）
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    /// * `event`      - 事件
    async fn append_event(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        event: &AgentStreamEvent,
    ) -> AgentResult<()> {
        let payload_json = serde_json::to_string(event)
            .map_err(|e| Error::Internal(format!("序列化事件失败: {e}")))?;
        self.append_record(SessionEventRecord {
            session_id,
            agent_id,
            user_id,
            run_id: event.run_id(),
            event_type: event.event_type(),
            payload_json,
        })
        .await
    }
}

/// 共享 [`SessionService`] 引用类型
pub type SharedSessionService = Arc<dyn SessionService>;
