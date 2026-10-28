//! 智能体与 A2A 服务绑定关系实体（dense 格式）
//!
//! 记录智能体绑定了哪些 A2A 服务注册表中的服务。

use chrono::Local;
use sea_orm::{
    FromQueryResult, Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

use super::a2a_service_registry;

/// 智能体与 A2A 服务绑定关系
///
/// 通过 `registry_id` 引用 `t_agent_a2a_service_registry`，
/// 支持多个智能体共享同一 A2A 服务注册项。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(
    table_name = "t_agent_a2a_service_rel",
    comment = "智能体与 A2A 服务绑定关系表"
)]
pub struct Model {
    /// 关系ID
    #[sea_orm(primary_key, auto_increment = true, comment = "关系ID")]
    pub id: i64,
    /// 智能体ID
    #[sea_orm(comment = "智能体ID", indexed)]
    pub agent_id: i64,
    /// A2A 服务注册表ID
    #[sea_orm(comment = "A2A 服务注册表ID", indexed)]
    pub registry_id: i64,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 创建时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "创建时间")]
    pub created_at: DateTime,
    /// 更新时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "更新时间")]
    pub updated_at: DateTime,
    /// 关联的 A2A 服务注册项（历史表未建外键，`skip_fk` 保持 DDL 不变，仅生成关系）
    #[sea_orm(
        belongs_to,
        skip_fk,
        from = "registry_id",
        to = "id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    pub a2a_service_registry: BelongsTo<a2a_service_registry::Entity>,
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
