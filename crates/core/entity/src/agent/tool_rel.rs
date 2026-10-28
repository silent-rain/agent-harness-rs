//! 智能体工具绑定表（dense 格式）

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

use super::agent_config;
use super::tool_registry;

/// 智能体工具绑定表
///
/// 对应数据库表 `t_agent_tool_rel`，外键由 `belongs_to` 关系字段派生。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_tool_rel", comment = "智能体工具绑定表")]
pub struct Model {
    /// 绑定ID
    #[sea_orm(primary_key, auto_increment = true, comment = "绑定ID")]
    pub id: i64,
    /// 智能体ID
    #[sea_orm(comment = "智能体ID", indexed)]
    pub agent_id: i64,
    /// 工具ID
    #[sea_orm(comment = "工具ID")]
    pub tool_id: i64,
    /// 优先级（数值越小越优先）
    #[sea_orm(default_value = 0, comment = "优先级（数值越小越优先）")]
    pub priority: i32,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 创建时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "创建时间")]
    pub created_at: DateTime,
    /// 更新时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "更新时间")]
    pub updated_at: DateTime,
    /// 所属智能体
    #[sea_orm(
        belongs_to,
        relation_enum = "Agent",
        from = "agent_id",
        to = "id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    pub agent: BelongsTo<agent_config::Entity>,
    /// 绑定的工具
    #[sea_orm(
        belongs_to,
        from = "tool_id",
        to = "id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    pub tool: BelongsTo<tool_registry::Entity>,
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
