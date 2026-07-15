//! 智能体会话数据访问层
//!
//! 仅封装 `AgentSessionService` 所需的会话查询与状态更新操作，
//! 不依赖外部 `agent-assembly` 的 `SessionDao`。

use chrono::Local;
use nject::injectable;
use sea_orm::{ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, Set};

use entity::agent::session;
use inject::PrimaryDb;

use crate::enums::agent_session::SessionStatus;

/// 会话数据访问
#[injectable]
pub struct AgentSessionDao {
    pub db: PrimaryDb,
}

impl AgentSessionDao {
    /// 根据 `session_id` 查询单条记录（自动过滤已删除）
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话业务 ID
    pub async fn get(&self, session_id: &str) -> Result<Option<session::Model>, DbErr> {
        session::Entity::find()
            .filter(session::Column::SessionId.eq(session_id))
            .filter(session::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 更新会话状态
    ///
    /// 将指定会话的 `status` 更新为目标值，同时刷新 `last_event_at` 为当前时间。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话业务 ID
    /// * `status` - 目标状态
    pub async fn update_status(
        &self,
        session_id: &str,
        status: SessionStatus,
    ) -> Result<session::Model, DbErr> {
        let model = self
            .get(session_id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound(format!("session 不存在: {session_id}")))?;

        let now = Local::now().naive_local();
        let mut active: session::ActiveModel = model.into();
        active.status = Set(i16::from(status));
        active.last_event_at = Set(Some(now));

        active.update(self.db.db()).await
    }
}
