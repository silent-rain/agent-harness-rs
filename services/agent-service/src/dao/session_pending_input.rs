//! 会话注入消息队列数据访问层
//!
//! 封装 `t_agent_session_pending_input` 表的写入与查询操作，
//! 供 `AgentSessionService` 等上层服务调用。

use nject::injectable;
use sea_orm::{DbErr, EntityTrait, NotSet, Set};

use entity::agent::session_pending_input;
use inject::PrimaryDb;

/// 会话注入消息队列数据访问
#[injectable]
pub struct SessionPendingInputDao {
    pub db: PrimaryDb,
}

impl SessionPendingInputDao {
    /// 向会话注入队列追加一条待消费消息
    ///
    /// 将已序列化的 JSON 字符串写入 `t_agent_session_pending_input` 队列表，
    /// Agent 运行循环在下一轮通过 `drain_pending_input` 消费新消息。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `agent_id` - 智能体 ID
    /// * `user_id` - 用户 ID
    /// * `content_json` - 已序列化的消息 JSON 字符串
    pub async fn append_pending_input(
        &self,
        session_id: &str,
        agent_id: i64,
        user_id: &str,
        content_json: String,
    ) -> Result<(), DbErr> {
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
            .exec(self.db.db())
            .await?;
        Ok(())
    }
}
