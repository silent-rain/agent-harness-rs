//! LLM 模型定义表（dense 格式）

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// LLM 模型定义表
///
/// 对应数据库表 `t_agent_llm_model`。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_llm_model", comment = "LLM 模型配置表")]
pub struct Model {
    /// 模型ID
    #[sea_orm(primary_key, auto_increment = true, comment = "模型ID")]
    pub id: i64,
    /// 所属用户ID
    #[sea_orm(comment = "所属用户ID", indexed)]
    pub user_id: i64,
    /// 供应商，如 openai / qwen / glm
    #[sea_orm(
        column_type = "String(StringLen::N(64))",
        comment = "供应商，如 openai / qwen / glm",
        indexed
    )]
    pub provider: String,
    /// 模型编码，如 gpt-4o / qwen-plus
    #[sea_orm(
        column_type = "String(StringLen::N(128))",
        comment = "模型编码，如 gpt-4o / qwen-plus",
        indexed
    )]
    pub model_code: String,
    /// 模型显示名称
    #[sea_orm(column_type = "String(StringLen::N(128))", comment = "模型显示名称")]
    pub model_name: String,
    /// API 基础地址
    #[sea_orm(column_type = "String(StringLen::N(512))", comment = "API 基础地址")]
    pub base_url: String,
    /// API Key
    #[sea_orm(column_type = "String(StringLen::N(256))", comment = "API Key")]
    pub api_key: String,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用", indexed)]
    pub enabled: bool,
    /// 是否已删除
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
