//! 智能体规则注册表（dense 格式）

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体规则注册表
///
/// 对应数据库表 `t_agent_rule_registry`，规则以文本形式注入 System Prompt。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_rule_registry", comment = "智能体规则注册表")]
pub struct Model {
    /// 规则ID
    #[sea_orm(primary_key, auto_increment = true, comment = "规则ID")]
    pub id: i64,
    /// 所属用户ID
    #[sea_orm(comment = "所属用户ID", indexed)]
    pub user_id: i64,
    /// 规则名称（同一用户下唯一，排除已删除记录）
    #[sea_orm(
        column_type = "String(StringLen::N(128))",
        comment = "规则名称（同一用户下唯一，排除已删除记录）"
    )]
    pub name: String,
    /// 规则描述
    #[sea_orm(column_type = "Text", comment = "规则描述")]
    pub description: String,
    /// 规则内容（注入 System Prompt 的文本）
    #[sea_orm(
        column_type = "Text",
        comment = "规则内容（注入 System Prompt 的文本）"
    )]
    pub content: String,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 是否删除
    #[sea_orm(default_value = false, comment = "是否删除")]
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
