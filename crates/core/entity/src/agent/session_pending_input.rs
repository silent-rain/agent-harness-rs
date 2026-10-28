//! 智能体会话注入消息队列实体（dense 格式）
//!
//! 接口层写入、ReAct 循环消费的 FIFO 队列。

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体会话注入消息队列（FIFO + 消费标记）
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(
    table_name = "t_agent_session_pending_input",
    comment = "智能体会话注入消息队列（FIFO + 消费标记）"
)]
pub struct Model {
    /// 自增主键（队列序号）
    #[sea_orm(primary_key, auto_increment = true, comment = "自增主键（队列序号）")]
    pub id: i64,
    /// 所属会话ID
    #[sea_orm(column_type = "String(StringLen::N(64))", comment = "所属会话ID")]
    pub session_id: String,
    /// 所属智能体ID（冗余）
    #[sea_orm(comment = "所属智能体ID（冗余）")]
    pub agent_id: i64,
    /// 所属用户ID（冗余；支持多种身份系统）
    #[sea_orm(
        column_type = "String(StringLen::N(64))",
        comment = "所属用户ID（冗余；支持多种身份系统）"
    )]
    pub user_id: String,
    /// 完整业务 JSON（ConversationMessage 序列化）
    #[sea_orm(
        column_type = "Text",
        comment = "完整业务 JSON（ConversationMessage 序列化）"
    )]
    pub content_json: String,
    /// 被 ReAct 循环消费的时间（NULL 表示未消费）
    #[sea_orm(comment = "被 ReAct 循环消费的时间（NULL 表示未消费）")]
    pub consumed_at: Option<DateTime>,
    /// 创建时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "创建时间")]
    pub created_at: DateTime,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 在插入前自动设置创建时间
    ///
    /// 仅 `ActiveModel::insert()` / `save()` 触发；
    /// 批量 `Entity::insert_many()` 不经过本钩子，需先调 [`ActiveModel::ensure_timestamps`]。
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            self.created_at = Set(Local::now().naive_local());
        }
        Ok(self)
    }
}

impl ActiveModel {
    /// 显式补齐 `created_at`（`insert_many` 不触发 `before_save`）。
    pub fn ensure_timestamps(&mut self) {
        self.created_at = Set(Local::now().naive_local());
    }
}
