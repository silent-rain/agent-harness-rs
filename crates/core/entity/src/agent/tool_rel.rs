//! 智能体工具绑定表

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体工具绑定表
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_tool_rel")]
pub struct Model {
    /// 绑定ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 智能体ID
    pub agent_id: i64,
    /// 工具ID
    pub tool_id: i64,
    /// 优先级（数值越小越优先）
    pub priority: i32,
    /// 是否启用
    pub enabled: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::agent_config::Entity",
        from = "Column::AgentId",
        to = "super::agent_config::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Agent,
    #[sea_orm(
        belongs_to = "super::tool_registry::Entity",
        from = "Column::ToolId",
        to = "super::tool_registry::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    ToolRegistry,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 在插入前自动设置创建时间
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
