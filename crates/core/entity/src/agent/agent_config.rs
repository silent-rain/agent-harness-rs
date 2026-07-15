//! 智能体配置主表
//!
//! 存储智能体的核心元数据，包括基础信息与 Prompt 配置。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ConnectionTrait, DbErr, DeriveEntityModel,
    DerivePrimaryKey, DeriveRelation, EnumIter, IntoActiveModel, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体配置主表
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_config")]
pub struct Model {
    /// 智能体ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID
    pub user_id: i64,
    /// 智能体名称
    pub name: String,
    /// 智能体描述
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// 系统 Prompt
    #[sea_orm(column_type = "Text")]
    pub system_prompt: String,
    /// 欢迎语（可选）
    #[sea_orm(column_type = "Text", nullable)]
    pub welcome_message: Option<String>,
    /// 标签列表（JSON 数组，默认为空数组）
    #[sea_orm(column_type = "Text")]
    pub tags_json: String,
    /// 最大子智能体递归委派深度（默认 3）
    pub max_delegation_depth: i32,
    /// 状态: 0=draft, 1=active, 2=disabled
    pub status: i16,
    /// 是否已删除（软删除）
    pub deleted: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
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

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 在插入/更新前自动更新时间戳
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
