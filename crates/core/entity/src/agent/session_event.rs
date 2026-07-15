//! 智能体会话事件流实体
//!
//! 对应表 `t_agent_session_event`：会话内的 append-only 事件日志。
//! 整合并取代 `t_agent_memory_stream`、`t_agent_tool_audit_log` 的事件存储职责。
//!
//! # 设计要点
//!
//! - 事件按 `(session_id, id)` 顺序读取：自增主键 `id` 已经天然单调递增，
//!   故不再单独维护 `seq` 字段。
//! - 子智能体的归属通过事件 payload 中的 `agent_id` 表达，
//!   不再单独保留 `author` 字段。
//! - 同一 session 在业务上通常对应单一运行流程，但仍允许系统级事件
//!   （如 session 创建、定时摘要）在没有 `run_id` 的情况下落库。
//! - `expires_at` / `deleted` 用于 TTL 与软删除，配合后台清理任务。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体会话事件流（append-only）
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_session_event")]
pub struct Model {
    /// 自增主键，同时承担会话内事件顺序的职责
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属会话ID
    pub session_id: String,
    /// 所属智能体ID（冗余）
    pub agent_id: i64,
    /// 所属用户ID（冗余，支持多种身份系统）
    pub user_id: String,
    /// 关联运行ID（可空，系统级事件可不绑定）
    pub run_id: Option<String>,
    /// 事件类型: user_message/assistant_message/tool_call/tool_result/planning/error/system
    pub event_type: String,
    /// 事件完整载荷（JSON 文本，对应 `AgentStreamEvent` 序列化结果）
    #[sea_orm(column_type = "Text")]
    pub payload_json: String,
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
