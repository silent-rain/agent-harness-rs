//! 智能体（agent 私有）级上下文实体（dense 格式）
//!
//! agent 私有上下文流：按写入顺序持久化蒸馏后的 agent 经验 / 工作模式。

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体（agent 私有）级上下文表（按序追加）
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(
    table_name = "t_agent_session_agent_state",
    comment = "智能体（agent 私有）级上下文表（按序追加）"
)]
pub struct Model {
    /// 自增主键（同时承载写入顺序）
    #[sea_orm(
        primary_key,
        auto_increment = true,
        comment = "自增主键（同时承载写入顺序）"
    )]
    pub id: i64,
    /// 所属智能体ID
    #[sea_orm(comment = "所属智能体ID")]
    pub agent_id: i64,
    /// 所属用户ID（支持多种身份系统）
    #[sea_orm(
        column_type = "String(StringLen::N(64))",
        comment = "所属用户ID（支持多种身份系统）"
    )]
    pub user_id: String,
    /// 完整业务 JSON（蒸馏后的 agent 经验/工作模式，可空表示 null）
    #[sea_orm(
        column_type = "Text",
        comment = "完整业务 JSON（蒸馏后的 agent 经验/工作模式，可空表示 null）"
    )]
    pub content_json: Option<String>,
    /// 过期时间（用于 TTL 清理，可空表示不过期）
    #[sea_orm(indexed, comment = "过期时间（用于 TTL 清理，可空表示不过期）")]
    pub expires_at: Option<DateTime>,
    /// 是否已删除（软删除）
    #[sea_orm(default_value = false, comment = "是否已删除（软删除）")]
    pub deleted: bool,
    /// 创建时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "创建时间")]
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
