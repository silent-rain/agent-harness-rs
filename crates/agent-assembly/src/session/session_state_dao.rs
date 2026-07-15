//! 三层上下文统一 DAO
//!
//! 通过 [`ContextScope`] 一次性寻址三层上下文表：
//!
//! - [`ContextScope::Session`] → `t_agent_session_state`
//! - [`ContextScope::User`]    → `t_agent_session_user_state`（**跨 agent**，不携带 `agent_id`）
//! - [`ContextScope::Agent`]   → `t_agent_session_agent_state`（agent 私有；`user_id = 0` 表示
//!   agent 全局共享，非零表示 agent 视角下的用户记忆）
//!
//! # 设计要点
//!
//! - 表语义已从「KV 状态」演化为「按序追加的上下文流」：每行存储一条完整业务 JSON，
//!   按 `id` 升序即写入顺序，由调用方负责序列化/反序列化业务结构。
//! - 仅暴露 [`SessionStateDao::append`]（写）与 [`SessionStateDao::list`]（读）两个核心方法，
//!   对应「写入用户记忆」与「召回相关记忆」两条主路径。

use std::sync::Arc;

use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, NotSet, QueryFilter, QueryOrder, Set,
    prelude::DateTime,
};

use agent_base::types::session::ContextScope;
use entity::agent::{session_agent_state, session_state, session_user_state};

use crate::error::PlatformResult;

/// 通用上下文条目 DTO
///
/// 三张表对外暴露的统一返回结构，调用方按需将 [`Self::content_json`]
/// 反序列化为具体业务结构（如对话消息、蒸馏知识等）。
#[derive(Debug, Clone)]
pub struct SessionStateEntry {
    /// 自增 ID（同时承载写入顺序）
    pub id: i64,
    /// 完整业务 JSON 字符串（None 表示 null）
    pub content_json: Option<String>,
    /// 过期时间
    pub expires_at: Option<DateTime>,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
    pub updated_at: DateTime,
}

impl From<session_state::Model> for SessionStateEntry {
    fn from(m: session_state::Model) -> Self {
        Self {
            id: m.id,
            content_json: m.content_json,
            expires_at: m.expires_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

impl From<session_user_state::Model> for SessionStateEntry {
    fn from(m: session_user_state::Model) -> Self {
        Self {
            id: m.id,
            content_json: m.content_json,
            expires_at: m.expires_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

impl From<session_agent_state::Model> for SessionStateEntry {
    fn from(m: session_agent_state::Model) -> Self {
        Self {
            id: m.id,
            content_json: m.content_json,
            expires_at: m.expires_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

/// 三层上下文统一 DAO
#[derive(Clone)]
pub struct SessionStateDao {
    db: Arc<DatabaseConnection>,
}

impl SessionStateDao {
    /// 创建 DAO 实例
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    /// 追加一条上下文条目
    ///
    /// 每次调用都会插入一行新记录，物理顺序由数据库自增 `id` 保证。
    ///
    /// # Parameters
    ///
    /// * `scope` - 作用域（决定写入哪张表）
    /// * `content_json` - 完整业务 JSON 字符串，None 表示写入 null
    /// * `expires_at` - 过期时间（可空表示不过期）
    pub async fn append(
        &self,
        scope: ContextScope<'_>,
        content_json: Option<String>,
        expires_at: Option<DateTime>,
    ) -> PlatformResult<()> {
        match scope {
            ContextScope::Session {
                session_id,
                agent_id,
                user_id,
            } => {
                let active = session_state::ActiveModel {
                    id: NotSet,
                    session_id: Set(session_id.to_string()),
                    agent_id: Set(agent_id),
                    user_id: Set(user_id.to_string()),
                    content_json: Set(content_json),
                    expires_at: Set(expires_at),
                    deleted: Set(false),
                    ..Default::default()
                };
                session_state::Entity::insert(active)
                    .exec(self.db.as_ref())
                    .await?;
            }
            ContextScope::User { user_id } => {
                let active = session_user_state::ActiveModel {
                    id: NotSet,
                    user_id: Set(user_id.to_string()),
                    content_json: Set(content_json),
                    expires_at: Set(expires_at),
                    deleted: Set(false),
                    ..Default::default()
                };
                session_user_state::Entity::insert(active)
                    .exec(self.db.as_ref())
                    .await?;
            }
            ContextScope::Agent { agent_id, user_id } => {
                let active = session_agent_state::ActiveModel {
                    id: NotSet,
                    agent_id: Set(agent_id),
                    user_id: Set(user_id.to_string()),
                    content_json: Set(content_json),
                    expires_at: Set(expires_at),
                    deleted: Set(false),
                    ..Default::default()
                };
                session_agent_state::Entity::insert(active)
                    .exec(self.db.as_ref())
                    .await?;
            }
        }
        Ok(())
    }

    /// 列出指定作用域下的全部条目（按 `id` 升序，即写入顺序；不分页，调用方按需自行控制规模）
    pub async fn list(&self, scope: ContextScope<'_>) -> PlatformResult<Vec<SessionStateEntry>> {
        match scope {
            ContextScope::Session { session_id, .. } => {
                let models = session_state::Entity::find()
                    .filter(session_state::Column::SessionId.eq(session_id))
                    .filter(session_state::Column::Deleted.eq(false))
                    .order_by_asc(session_state::Column::Id)
                    .all(self.db.as_ref())
                    .await?;
                Ok(models.into_iter().map(SessionStateEntry::from).collect())
            }
            ContextScope::User { user_id } => {
                let models = session_user_state::Entity::find()
                    .filter(session_user_state::Column::UserId.eq(user_id.to_string()))
                    .filter(session_user_state::Column::Deleted.eq(false))
                    .order_by_asc(session_user_state::Column::Id)
                    .all(self.db.as_ref())
                    .await?;
                Ok(models.into_iter().map(SessionStateEntry::from).collect())
            }
            ContextScope::Agent { agent_id, user_id } => {
                let models = session_agent_state::Entity::find()
                    .filter(session_agent_state::Column::AgentId.eq(agent_id))
                    .filter(session_agent_state::Column::UserId.eq(user_id.to_string()))
                    .filter(session_agent_state::Column::Deleted.eq(false))
                    .order_by_asc(session_agent_state::Column::Id)
                    .all(self.db.as_ref())
                    .await?;
                Ok(models.into_iter().map(SessionStateEntry::from).collect())
            }
        }
    }

    /// 增量读取指定作用域下 id 大于 `after_id` 的条目
    ///
    /// 用于 `drive_react_loop` 在循环过程中动态加载新注入的消息，
    /// 避免每轮都全量读取整个上下文表。仅支持 `Session` 作用域。
    ///
    /// # Parameters
    ///
    /// * `scope` - 作用域（仅 `Session` 有效，其他作用域返回空列表）
    /// * `after_id` - 上次已加载的最大 id，仅返回 id > after_id 的记录
    ///
    /// # Returns
    ///
    /// 新增条目列表（按 id 升序）
    pub async fn list_after(
        &self,
        scope: ContextScope<'_>,
        after_id: i64,
    ) -> PlatformResult<Vec<SessionStateEntry>> {
        match scope {
            ContextScope::Session { session_id, .. } => {
                let models = session_state::Entity::find()
                    .filter(session_state::Column::SessionId.eq(session_id))
                    .filter(session_state::Column::Id.gt(after_id))
                    .filter(session_state::Column::Deleted.eq(false))
                    .order_by_asc(session_state::Column::Id)
                    .all(self.db.as_ref())
                    .await?;
                Ok(models.into_iter().map(SessionStateEntry::from).collect())
            }
            // User / Agent 作用域不支持增量读取（无此场景需求）
            _ => Ok(Vec::new()),
        }
    }

    /// 获取指定 Session 作用域下当前上下文的最大 id
    ///
    /// 用于记录"基线 id"，后续通过 `list_after` 增量读取。
    ///
    /// # Parameters
    ///
    /// * `scope` - 作用域（仅 `Session` 有效，其他作用域返回 0）
    ///
    /// # Returns
    ///
    /// 当前最大 id（无记录时返回 0）
    pub async fn get_max_id(&self, scope: ContextScope<'_>) -> PlatformResult<i64> {
        match scope {
            ContextScope::Session { session_id, .. } => {
                let model = session_state::Entity::find()
                    .filter(session_state::Column::SessionId.eq(session_id))
                    .filter(session_state::Column::Deleted.eq(false))
                    .order_by_desc(session_state::Column::Id)
                    .one(self.db.as_ref())
                    .await?;
                Ok(model.map(|m| m.id).unwrap_or(0))
            }
            _ => Ok(0),
        }
    }
}
