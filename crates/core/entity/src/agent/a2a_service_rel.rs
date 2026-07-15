//! 智能体与 A2A 服务绑定关系实体
//!
//! 记录智能体绑定了哪些 A2A 服务注册表中的服务。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EntityTrait, EnumIter, FromQueryResult, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体与 A2A 服务绑定关系
///
/// 通过 `registry_id` 引用 `t_agent_a2a_service_registry`，
/// 支持多个智能体共享同一 A2A 服务注册项。
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_a2a_service_rel")]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 智能体ID
    pub agent_id: i64,
    /// A2A 服务注册表ID
    pub registry_id: i64,
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
        belongs_to = "super::a2a_service_registry::Entity",
        from = "Column::RegistryId",
        to = "super::a2a_service_registry::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    A2aServiceRegistry,
}

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

/// A2A 服务绑定关联查询结果（JOIN a2a_service_registry）
///
/// 用于 `a2a_service_rel` 与 `a2a_service_registry` 关联查询，
/// 一次 SQL 获取绑定关系及其关联服务的名称、描述和连接配置。
/// 可直接作为 API 响应体序列化输出。
#[derive(Clone, Debug, Serialize, Deserialize, FromQueryResult)]
pub struct A2aServiceRelWithRegistry {
    /// 绑定关系基础字段
    #[sea_orm(nested)]
    #[serde(flatten)]
    pub rel: Model,
    /// 服务名称
    pub registry_name: String,
    /// 服务描述
    pub registry_description: String,
    /// 服务基础 URL
    pub registry_base_url: String,
    /// 连接超时（秒）
    pub registry_timeout_secs: Option<i32>,
    /// 服务配置 JSON
    pub registry_config_json: Option<String>,
    /// 注册表自身是否启用
    pub registry_enabled: bool,
}
