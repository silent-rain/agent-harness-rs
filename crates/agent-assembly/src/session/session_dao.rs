//! 会话主表 DAO
//!
//! 封装 `t_agent_session` 的 CRUD 与按用户/智能体维度的列表查询。
//!
//! # 设计要点
//!
//! - 业务主键：`session_id`（UUID 字符串），自增 `id` 仅用于内部排序。
//! - 软删除：默认查询自动过滤 `deleted = false`，需要包含已删除时显式调用
//!   `*_with_deleted` 系列方法。
//! - 时间戳：`created_at` / `updated_at` 由 `ActiveModelBehavior::before_save` 维护，
//!   `last_event_at` 由事件 DAO 在 append 时同步刷新，不在此处维护。

use std::sync::Arc;

use chrono::Local;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, NotSet, PaginatorTrait,
    QueryFilter, QueryOrder, Set, prelude::DateTime,
};

use agent_base::types::session::SessionStatus;
use entity::agent::session::{ActiveModel, Column, Entity, Model};

use crate::error::{Error, PlatformResult};

/// 创建会话所需的字段
#[derive(Debug, Clone)]
pub struct NewSession {
    /// 会话业务ID（UUID）
    pub session_id: String,
    /// 智能体ID
    pub agent_id: i64,
    /// 用户ID
    pub user_id: String,
    /// 标题（可选）
    pub title: Option<String>,
    /// 过期时间（可选）
    pub expires_at: Option<DateTime>,
}

/// 更新会话所需的字段（None 字段表示不修改）
#[derive(Debug, Clone, Default)]
pub struct SessionUpdate {
    /// 标题
    pub title: Option<Option<String>>,
    /// 状态
    pub status: Option<SessionStatus>,
    /// 最近事件时间
    pub last_event_at: Option<DateTime>,
    /// 过期时间
    pub expires_at: Option<Option<DateTime>>,
}

/// 会话主表 DAO
#[derive(Clone)]
pub struct SessionDao {
    db: Arc<DatabaseConnection>,
}

impl SessionDao {
    /// 创建 DAO 实例
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    /// 创建一个新会话
    ///
    /// # Returns
    ///
    /// 返回插入后的会话模型（含数据库分配的 `id` 与时间戳）。
    pub async fn create(&self, new_session: NewSession) -> PlatformResult<Model> {
        let active = ActiveModel {
            id: NotSet,
            session_id: Set(new_session.session_id),
            agent_id: Set(new_session.agent_id),
            user_id: Set(new_session.user_id),
            title: Set(new_session.title),
            status: Set(SessionStatus::Idle.into()),
            last_event_at: Set(None),
            expires_at: Set(new_session.expires_at),
            deleted: Set(false),
            ..Default::default()
        };

        let model = active.insert(self.db.as_ref()).await?;
        Ok(model)
    }

    /// 根据 `session_id` 查询单条记录（自动过滤已删除）
    pub async fn get(&self, session_id: &str) -> PlatformResult<Option<Model>> {
        let model = Entity::find()
            .filter(Column::SessionId.eq(session_id))
            .filter(Column::Deleted.eq(false))
            .one(self.db.as_ref())
            .await?;
        Ok(model)
    }

    /// 幂等获取或创建会话主表记录
    ///
    /// 用于在每次 run 入口调用，保证会话记录存在。
    /// 已存在则直接返回原记录，不存在则插入并返回新记录。
    /// 与 [`Self::ensure`] 不同，本方法**不刷新** `last_event_at`。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话业务 ID（UUID）
    /// * `agent_id`   - 智能体 ID
    /// * `user_id`    - 用户 ID
    ///
    /// # Returns
    ///
    /// 已存在则返回原记录，不存在则插入并返回新记录。
    pub async fn get_or_create(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
    ) -> PlatformResult<Model> {
        if let Some(model) = self.get(session_id).await? {
            return Ok(model);
        }
        self.create(NewSession {
            session_id: session_id.to_string(),
            agent_id,
            user_id: user_id.to_string(),
            title: None,
            expires_at: None,
        })
        .await
    }

    /// 更新会话状态与活跃时间
    ///
    /// 更新会话主表的 `status` 与 `last_event_at` 字段。
    /// `last_event_at` 会被自动设置为当前时间。
    /// 仅 `update.status` 中显式提供的字段会被更新。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话业务 ID
    /// * `status` - 目标状态（传入 `None` 表示不修改状态，仅刷新时间）
    pub async fn update_status(
        &self,
        session_id: &str,
        status: Option<SessionStatus>,
    ) -> PlatformResult<Model> {
        let now = Local::now().naive_local();
        self.update(
            session_id,
            SessionUpdate {
                status,
                last_event_at: Some(now),
                ..Default::default()
            },
        )
        .await
    }

    /// 列出指定 (`agent_id`, `user_id`) 下的会话，按 `last_event_at` / `created_at` 倒序
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体ID
    /// * `user_id`  - 用户ID
    /// * `page`     - 页码（从 1 开始）
    /// * `page_size`- 每页数量
    pub async fn list_by_agent_user(
        &self,
        agent_id: i64,
        user_id: &str,
        page: u64,
        page_size: u64,
    ) -> PlatformResult<Vec<Model>> {
        let page_index = page.saturating_sub(1);
        let paginator = Entity::find()
            .filter(Column::AgentId.eq(agent_id))
            .filter(Column::UserId.eq(user_id))
            .filter(Column::Deleted.eq(false))
            .order_by_desc(Column::LastEventAt)
            .order_by_desc(Column::CreatedAt)
            .paginate(self.db.as_ref(), page_size);

        let models = paginator.fetch_page(page_index).await?;
        Ok(models)
    }

    /// 统计指定 (`agent_id`, `user_id`) 下的会话总数
    pub async fn count_by_agent_user(&self, agent_id: i64, user_id: &str) -> PlatformResult<u64> {
        let total = Entity::find()
            .filter(Column::AgentId.eq(agent_id))
            .filter(Column::UserId.eq(user_id))
            .filter(Column::Deleted.eq(false))
            .count(self.db.as_ref())
            .await?;
        Ok(total)
    }

    /// 按字段更新会话
    ///
    /// 仅 `update.title` / `status` / `last_event_at` / `expires_at` 中显式提供的字段会被更新。
    pub async fn update(&self, session_id: &str, update: SessionUpdate) -> PlatformResult<Model> {
        let model = self
            .get(session_id)
            .await?
            .ok_or_else(|| Error::Internal(format!("session 不存在: {session_id}")))?;
        let mut active: ActiveModel = model.into();

        if let Some(title) = update.title {
            active.title = Set(title);
        }
        if let Some(status) = update.status {
            active.status = Set(status.into());
        }
        if let Some(last_event_at) = update.last_event_at {
            active.last_event_at = Set(Some(last_event_at));
        }
        if let Some(expires_at) = update.expires_at {
            active.expires_at = Set(expires_at);
        }

        let updated = active.update(self.db.as_ref()).await?;
        Ok(updated)
    }

    /// 软删除会话（设置 `deleted = true`，不级联删除事件/状态表）
    pub async fn soft_delete(&self, session_id: &str) -> PlatformResult<()> {
        let model = self
            .get(session_id)
            .await?
            .ok_or_else(|| Error::Internal(format!("session 不存在: {session_id}")))?;
        let mut active: ActiveModel = model.into();
        active.deleted = Set(true);
        active.update(self.db.as_ref()).await?;
        Ok(())
    }
}
