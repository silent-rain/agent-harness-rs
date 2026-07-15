//! 智能体会话实体
//!
//! 对应表 `t_agent_session`：会话（Session）作为运行时一等公民，
//! 关联 agent / user，并为事件流与三层状态（global/user/session）提供归属维度。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体会话主表
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_session")]
pub struct Model {
    /// 自增主键
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 会话ID（UUID，业务主键，全局唯一）
    pub session_id: String,
    /// 所属智能体ID
    pub agent_id: i64,
    /// 所属用户ID（支持多种身份系统）
    pub user_id: String,
    /// 会话标题（可选，用户可改）
    pub title: Option<String>,
    /// 状态: 0=未运行(Idle), 1=运行中(Running), 2=已取消(Cancelled), 3=已完成(Completed)
    pub status: i16,
    /// 最近事件时间
    pub last_event_at: Option<DateTime>,
    /// 过期时间（用于 TTL 清理，可空表示不过期）
    pub expires_at: Option<DateTime>,
    /// 是否已删除（软删除）
    pub deleted: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 在插入/更新前自动设置时间戳
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = Local::now().naive_local();
        if insert {
            self.created_at = Set(now);
        }
        self.updated_at = Set(now);
        Ok(self)
    }
}
