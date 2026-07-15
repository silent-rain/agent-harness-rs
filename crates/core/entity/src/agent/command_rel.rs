//! 智能体与命令绑定关系实体

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, FromQueryResult, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体与命令绑定关系
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_command_rel")]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 智能体ID
    pub agent_id: i64,
    /// 命令ID
    pub command_id: i64,
    /// 排序优先级（数值越小越靠前）
    pub priority: i32,
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

/// 命令绑定关联查询结果（JOIN command_registry）
///
/// 用于 `command_rel` 与 `command_registry` 关联查询，
/// 一次 SQL 获取绑定关系及其关联命令的名称、描述和启用状态。
/// 可直接作为 API 响应体序列化输出。
#[derive(Clone, Debug, Serialize, Deserialize, FromQueryResult)]
pub struct CommandRelWithCommand {
    /// 绑定关系基础字段
    #[sea_orm(nested)]
    #[serde(flatten)]
    pub rel: Model,
    /// 命令名称
    pub command_name: String,
    /// 命令描述
    pub command_description: String,
    /// 命令自身是否启用
    pub command_enabled: bool,
}
