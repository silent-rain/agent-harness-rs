//! 智能体配置主表
//!
//! 存储智能体的核心元数据，包括基础信息与 Prompt 配置。

use chrono::Local;
use sea_orm::{
    IntoActiveModel, Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体配置主表（dense 格式）
///
/// 对应数据库表 `t_agent_config`，表结构由字段上的 `#[sea_orm(...)]`
/// 元数据派生（entity-first 迁移的唯一事实源）。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_config", comment = "智能体配置主表")]
pub struct Model {
    /// 智能体ID
    #[sea_orm(primary_key, auto_increment = true, comment = "智能体ID")]
    pub id: i64,
    /// 所属用户ID
    #[sea_orm(comment = "所属用户ID", indexed)]
    pub user_id: i64,
    /// 智能体名称
    #[sea_orm(column_type = "String(StringLen::N(128))", comment = "智能体名称")]
    pub name: String,
    /// 智能体描述
    #[sea_orm(column_type = "Text", comment = "智能体描述")]
    pub description: String,
    /// 系统 Prompt
    #[sea_orm(column_type = "Text", comment = "系统 Prompt")]
    pub system_prompt: String,
    /// 欢迎语（可选）
    #[sea_orm(column_type = "Text", comment = "欢迎语（可选）")]
    pub welcome_message: Option<String>,
    /// 标签列表（JSON 数组，默认为空数组）
    #[sea_orm(
        column_type = "Text",
        default_value = "[]",
        comment = "标签列表（JSON 数组，默认为空数组）"
    )]
    pub tags_json: String,
    /// 最大子智能体递归委派深度（默认 3）
    #[sea_orm(default_value = 3, comment = "最大子智能体递归委派深度（默认 3）")]
    pub max_delegation_depth: i32,
    /// 状态: 0=draft, 1=active, 2=disabled
    #[sea_orm(
        column_type = "SmallInteger",
        default_value = 0,
        comment = "状态: 0=draft, 1=active, 2=disabled",
        indexed
    )]
    pub status: i16,
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

impl Model {
    /// 执行软删除
    ///
    /// # Parameters
    ///
    /// * `db` - 数据库连接
    pub async fn soft_delete<C>(self, db: &C) -> Result<(), DbErr>
    where
        C: ConnectionTrait,
    {
        let now = Local::now().naive_local();
        let mut active: ActiveModel = self.into_active_model();
        active.deleted = Set(true);
        active.updated_at = Set(now);
        active.update(db).await?;
        Ok(())
    }

    /// 恢复软删除
    ///
    /// # Parameters
    ///
    /// * `db` - 数据库连接
    pub async fn restore<C>(self, db: &C) -> Result<(), DbErr>
    where
        C: ConnectionTrait,
    {
        let now = Local::now().naive_local();
        let mut active: ActiveModel = self.into_active_model();
        active.deleted = Set(false);
        active.updated_at = Set(now);
        active.update(db).await?;
        Ok(())
    }
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
    /// 显式补齐 `created_at` / `updated_at`。
    ///
    /// `Entity::insert_many` 走 `Insert::exec`，不会触发
    /// [`ActiveModelBehavior::before_save`]，批量写入必须显式调用本方法。
    pub fn ensure_timestamps(&mut self) {
        let now = Local::now().naive_local();
        self.created_at = Set(now);
        self.updated_at = Set(now);
    }
}
