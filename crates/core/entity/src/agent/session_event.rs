//! 智能体会话事件流实体（dense 格式）
//!
//! 会话事件流（Append-only Event Log）：
//! - 仅追加写入（append-only），按 session 维度顺序读取
//! - 顺序由自增主键 `id` 保证（已天然单调），不单独维护 `seq`
//! - `payload_json` 承载完整事件内容（结构对应 `AgentStreamEvent`）
//! - `expires_at` / `deleted` 支持 TTL 与软删除

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体会话事件流表（append-only）
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(
    table_name = "t_agent_session_event",
    comment = "智能体会话事件流表（append-only）"
)]
pub struct Model {
    /// 自增主键，同时承担会话内事件顺序的职责
    #[sea_orm(
        primary_key,
        auto_increment = true,
        comment = "自增主键，同时承担会话内事件顺序"
    )]
    pub id: i64,
    /// 所属会话ID
    #[sea_orm(column_type = "String(StringLen::N(64))", comment = "所属会话ID")]
    pub session_id: String,
    /// 所属智能体ID（冗余，便于免 join 过滤）
    #[sea_orm(comment = "所属智能体ID（冗余，便于免 join 过滤）")]
    pub agent_id: i64,
    /// 所属用户ID（冗余，支持多种身份系统）
    #[sea_orm(
        column_type = "String(StringLen::N(64))",
        comment = "所属用户ID（冗余，支持多种身份系统）"
    )]
    pub user_id: String,
    /// 关联运行ID（可空，系统级事件可不绑定）
    #[sea_orm(
        column_type = "String(StringLen::N(64))",
        indexed,
        comment = "关联运行ID（可空，系统级事件可不绑定）"
    )]
    pub run_id: Option<String>,
    /// 事件类型: user_message/assistant_message/tool_call/tool_result/planning/error/system
    #[sea_orm(
        column_type = "String(StringLen::N(48))",
        indexed,
        comment = "事件类型: user_message/assistant_message/tool_call/tool_result/planning/error/system"
    )]
    pub event_type: String,
    /// 事件完整载荷（JSON，对应 `AgentStreamEvent` 序列化结果）
    #[sea_orm(
        column_type = "Text",
        comment = "事件完整载荷（JSON，对应 AgentStreamEvent 序列化结果）"
    )]
    pub payload_json: String,
    /// 过期时间（用于 TTL 清理，可空表示不过期）
    #[sea_orm(indexed, comment = "过期时间（用于 TTL 清理，可空表示不过期）")]
    pub expires_at: Option<DateTime>,
    /// 是否已删除（软删除）
    #[sea_orm(default_value = false, comment = "是否已删除（软删除）")]
    pub deleted: bool,
    /// 创建时间（事件发生时间）
    #[sea_orm(
        default_expr = "Expr::current_timestamp()",
        comment = "创建时间（事件发生时间）"
    )]
    pub created_at: DateTime,
    /// 更新时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "更新时间")]
    pub updated_at: DateTime,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 在插入/更新前自动更新时间戳
    ///
    /// 仅 `ActiveModel::insert()` / `save()` / `update()` 触发；
    /// 批量 `Entity::insert_many()` 不经过本钩子，需先调 [`ActiveModel::ensure_timestamps`]。
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

impl ActiveModel {
    /// 显式补齐 `created_at` / `updated_at`（`insert_many` 不触发 `before_save`）。
    pub fn ensure_timestamps(&mut self) {
        let now = Local::now().naive_local();
        self.created_at = Set(now);
        self.updated_at = Set(now);
    }
}
