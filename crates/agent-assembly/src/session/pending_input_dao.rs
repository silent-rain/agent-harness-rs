//! 会话注入消息队列 DAO
//!
//! 封装 `t_agent_session_pending_input` 的写入、消费（drain）操作。
//!
//! # 设计要点
//!
//! - 写入：接口层调用 [`PendingInputDao::enqueue`] 将消息写入队列
//! - 消费：`drive_react_loop` 每轮调用 [`PendingInputDao::drain`] 获取未消费记录
//!   并标记 `consumed_at`
//! - 消费后由上层负责将消息固化到 `t_agent_session_state`（"消费即固化"语义）

use std::sync::Arc;

use chrono::Local;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, NotSet, QueryFilter, QueryOrder, Set};

use entity::agent::session_pending_input;

use crate::error::PlatformResult;

/// 待消费注入消息条目 DTO
#[derive(Debug, Clone)]
pub struct PendingInputEntry {
    /// 自增 ID（队列序号）
    pub id: i64,
    /// 完整业务 JSON 字符串（ConversationMessage 序列化）
    pub content_json: String,
}

impl From<session_pending_input::Model> for PendingInputEntry {
    fn from(m: session_pending_input::Model) -> Self {
        Self {
            id: m.id,
            content_json: m.content_json,
        }
    }
}

/// 会话注入消息队列 DAO
#[derive(Clone)]
pub struct PendingInputDao {
    db: Arc<DatabaseConnection>,
}

impl PendingInputDao {
    /// 创建 DAO 实例
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    /// 向队列追加一条待消费消息
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id` - 智能体 ID
    /// * `user_id` - 用户 ID
    /// * `content_json` - 已序列化的 ConversationMessage JSON
    pub async fn enqueue(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        content_json: String,
    ) -> PlatformResult<()> {
        let active = session_pending_input::ActiveModel {
            id: NotSet,
            session_id: Set(session_id.to_string()),
            agent_id: Set(agent_id),
            user_id: Set(user_id.to_string()),
            content_json: Set(content_json),
            consumed_at: Set(None),
            ..Default::default()
        };
        session_pending_input::Entity::insert(active)
            .exec(self.db.as_ref())
            .await?;
        Ok(())
    }

    /// 消费（drain）指定会话的全部未消费消息
    ///
    /// 原子操作：读取 `consumed_at IS NULL` 的记录后立即标记 `consumed_at = NOW()`。
    /// 返回按 id 升序排列的消息列表（FIFO）。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    ///
    /// # Returns
    ///
    /// 未消费的消息列表（按 id 升序，即写入顺序）
    pub async fn drain(&self, session_id: &str) -> PlatformResult<Vec<PendingInputEntry>> {
        // 1. 读取未消费记录
        let models = session_pending_input::Entity::find()
            .filter(session_pending_input::Column::SessionId.eq(session_id))
            .filter(session_pending_input::Column::ConsumedAt.is_null())
            .order_by_asc(session_pending_input::Column::Id)
            .all(self.db.as_ref())
            .await?;

        if models.is_empty() {
            return Ok(Vec::new());
        }

        // 2. 标记为已消费
        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let now = Local::now().naive_local();

        session_pending_input::Entity::update_many()
            .filter(session_pending_input::Column::Id.is_in(ids))
            .col_expr(
                session_pending_input::Column::ConsumedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .exec(self.db.as_ref())
            .await?;

        // 3. 转换为 DTO
        Ok(models.into_iter().map(PendingInputEntry::from).collect())
    }
}
