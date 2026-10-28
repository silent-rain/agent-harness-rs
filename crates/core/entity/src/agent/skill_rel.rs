//! 智能体与技能绑定关系实体（dense 格式）
//!
//! 记录智能体启用了哪些技能。
//! 通过 `skill_package_id` 外键关联 `t_agent_skill_package`，
//! 与 `t_agent_command_rel` 的设计模式保持一致。

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

use super::skill_package;

/// 智能体与技能绑定关系
///
/// 作为"已安装 skill 的索引表"，元数据（name/description/skill_code）
/// 从包表 JOIN 获取。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_skill_rel", comment = "智能体与技能绑定关系表")]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key, auto_increment = true, comment = "关系ID")]
    pub id: i64,
    /// 所属用户ID
    #[sea_orm(comment = "所属用户ID", indexed)]
    pub user_id: i64,
    /// 智能体ID
    #[sea_orm(comment = "智能体ID", indexed)]
    pub agent_id: i64,
    /// 关联的技能安装包ID（外键 → t_agent_skill_package.id）
    #[sea_orm(
        indexed,
        comment = "关联的技能安装包ID（外键 → t_agent_skill_package.id）"
    )]
    pub skill_package_id: i64,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 创建时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "创建时间")]
    pub created_at: DateTime,
    /// 更新时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "更新时间")]
    pub updated_at: DateTime,
    /// 关联的技能安装包
    #[sea_orm(
        belongs_to,
        from = "skill_package_id",
        to = "id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    pub skill_package: BelongsTo<skill_package::Entity>,
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
