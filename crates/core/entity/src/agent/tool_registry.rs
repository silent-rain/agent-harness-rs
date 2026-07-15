//! 工具注册表

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 工具注册表
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_tool_registry")]
pub struct Model {
    /// 工具ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 工具唯一编码
    pub tool_code: String,
    /// 工具名称
    pub name: String,
    /// 工具描述
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// 函数 Schema（JSON Schema 格式）
    #[sea_orm(column_type = "Text", nullable)]
    pub schema_json: Option<String>,
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
