//! 智能体会话注入消息队列实体
//!
//! 对应表 `t_agent_session_pending_input`：接口层写入的待消费注入消息。
//! `drive_react_loop` 每轮开头 drain 未消费记录后标记 `consumed_at`，
//! 同时将消息固化到 `t_agent_session_state`，实现//!
//! # 设计要点
//!
//! - 与 `t_agent_session_state`（已固化对话历史）职责分离
//! - `consumed_at IS NULL` 精确识别待处理消息，无 race condition
//! - 按 `id` 升序即 FIFO 顺序

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体会话注入消息队列条目
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_session_pending_input")]
pub struct Model {
    /// 自增主键（队列序号）
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属会话ID
    pub session_id: String,
    /// 所属智能体ID（冗余）
    pub agent_id: i64,
    /// 所属用户ID（冗余；支持多种身份系统）
    pub user_id: String,
    /// 完整业务 JSON（ConversationMessage 序列化）
    #[sea_orm(column_type = "Text")]
    pub content_json: String,
    /// 被 ReAct 循环消费的时间（NULL 表示未消费）
    pub consumed_at: Option<DateTime>,
    /// 创建时间
    pub created_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 在插入前自动设置创建时间
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            let now = Local::now().naive_local();
            self.created_at = Set(now);
        }
        Ok(self)
    }
}
