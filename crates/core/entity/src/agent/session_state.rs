//! 智能体会话级上下文实体
//!
//! 对应表 `t_agent_session_state`：在单个 `session_id` 范围内按写入顺序持久化
//! 的对话上下文条目（消息流）。每行存储一条完整的业务 JSON
//! （含 `role` / `content` / `tool` 等字段），由调用方负责序列化/反序列化。
//!
//! # 设计要点
//!
//! - 不再使用 KV 模型，按 `id` 升序即为写入顺序。
//! - 与 `t_agent_session_user_state` / `t_agent_session_agent_state`
//!   共同构成三层上下文体系（优先级：session > user > agent）。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体会话级上下文条目
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_session_state")]
pub struct Model {
    /// 自增主键（同时承载写入顺序）
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属会话ID
    pub session_id: String,
    /// 所属智能体ID（冗余）
    pub agent_id: i64,
    /// 所属用户ID（冗余；支持多种身份系统）
    pub user_id: String,
    /// 完整业务 JSON（可包含 role / content / tool 等字段，可空表示 null）
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
