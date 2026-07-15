//! 智能体用户级上下文实体
//!
//! 对应表 `t_agent_session_user_state`：在 `user_id` 范围内**跨 agent**、跨 session
//! 持久化的用户级上下文/蒸馏知识条目。每行存储一条完整业务 JSON
//! （由蒸馏器决定其结构），按 `id` 升序即写入顺序。
//!
//! # 跨 Agent 记忆
//!
//! 同一个用户在多个 agent 之间共享的偏好/画像信息存放于此表，
//! 因此**不携带 `agent_id` 字段**。如需 agent 私有的用户级记忆，
//! 请使用 `t_agent_session_agent_state`。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体用户级上下文条目（跨 agent）
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_session_user_state")]
pub struct Model {
    /// 自增主键（同时承载写入顺序）
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID（支持多种身份系统）
    pub user_id: String,
    /// 完整业务 JSON（蒸馏后的用户偏好/画像/事实，可空表示 null）
    #[sea_orm(column_type = "Text", nullable)]
    pub content_json: Option<String>,
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
