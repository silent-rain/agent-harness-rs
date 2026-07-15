//! 会话事件流 DAO
//!
//! 封装 `t_agent_session_event` 的追加（append-only）与按会话顺序读取。
//!
//! # 设计要点
//!
//! - 事件按自增 `id` 单调递增，会话内顺序天然成立。
//! - DAO 只负责 `payload_json`（已序列化的 JSON 字符串）的存取，
//!   不解析事件内容，保持职责单一。
//! - `last_event_at` 由 [`SessionDao::ensure`] 在每次 run 入口刷新一次，
//!   不在每条事件 append 时更新，降低写入压力。

use std::sync::Arc;

use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    prelude::DateTime,
};

use entity::agent::session_event;

use crate::error::PlatformResult;

/// 追加事件所需的字段
#[derive(Debug, Clone)]
pub struct NewSessionEvent {
    /// 会话ID
    pub session_id: String,
    /// 智能体ID（冗余字段）
    pub agent_id: i64,
    /// 用户ID（冗余字段，支持多种身份系统）
    pub user_id: String,
    /// 关联运行ID（系统级事件可为 None）
    pub run_id: Option<String>,
    /// 事件类型（如 user_message / tool_call）
    pub event_type: String,
    /// 事件载荷（已序列化的 JSON 字符串）
    pub payload_json: String,
    /// 过期时间（可选）
    pub expires_at: Option<DateTime>,
}

/// 会话事件流 DAO
#[derive(Clone)]
pub struct SessionEventDao {
    db: Arc<DatabaseConnection>,
}

impl SessionEventDao {
    /// 创建 DAO 实例
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    /// 追加单条事件
    ///
    /// # Returns
    ///
    /// 返回插入后的事件模型（含数据库分配的 `id`）。
    pub async fn append(&self, event: NewSessionEvent) -> PlatformResult<session_event::Model> {
        let active = session_event::ActiveModel {
            session_id: Set(event.session_id),
            agent_id: Set(event.agent_id),
            user_id: Set(event.user_id),
            run_id: Set(event.run_id),
            event_type: Set(event.event_type),
            payload_json: Set(event.payload_json),
            expires_at: Set(event.expires_at),
            deleted: Set(false),
            ..Default::default()
        };

        let model = session_event::Entity::insert(active)
            .exec_with_returning(self.db.as_ref())
            .await?;

        Ok(model)
    }

    /// 按会话顺序读取事件
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话ID
    /// * `after_id`   - 仅返回 `id > after_id` 的事件（用于增量拉取，None 表示从头）
    /// * `limit`      - 最大返回条数（None 表示不限制）
    pub async fn list_by_session(
        &self,
        session_id: &str,
        after_id: Option<i64>,
        limit: Option<u64>,
    ) -> PlatformResult<Vec<session_event::Model>> {
        let mut query = session_event::Entity::find()
            .filter(session_event::Column::SessionId.eq(session_id))
            .filter(session_event::Column::Deleted.eq(false))
            .order_by_asc(session_event::Column::Id);

        if let Some(id) = after_id {
            query = query.filter(session_event::Column::Id.gt(id));
        }
        if let Some(n) = limit {
            query = query.limit(n);
        }

        let models = query.all(self.db.as_ref()).await?;
        Ok(models)
    }
}
