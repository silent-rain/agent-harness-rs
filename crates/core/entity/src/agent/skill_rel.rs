//! 智能体与技能绑定关系实体
//!
//! 记录智能体启用了哪些技能。
//! 通过 `skill_package_id` 外键关联 `t_agent_skill_package`，
//! 与 `t_agent_command_rel` 的设计模式保持一致。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体与技能绑定关系
///
/// 作为"已安装 skill 的索引表"，记录 agent 启用了哪些 skill。
/// 通过 `skill_package_id` 关联 `t_agent_skill_package`，
/// 元数据（name/description/skill_code）从包表 JOIN 获取。
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_skill_rel")]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID
    pub user_id: i64,
    /// 智能体ID
    pub agent_id: i64,
    /// 关联的技能安装包ID（外键 → t_agent_skill_package.id）
    pub skill_package_id: i64,
    /// 是否启用
    pub enabled: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
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
