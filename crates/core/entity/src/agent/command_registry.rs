//! 智能体命令注册表实体（dense 格式）
//!
//! 存储平台内建命令与用户自定义命令。
//! 命令（Command）是一种面向开发场景的指令式交互能力，用于将常见、高频、
//! 结构化的开发任务封装为可复用的命令。
//!
//! 与 Skill 的核心区别：
//! - **Skill**：模型自主调用，注入 System Prompt，LLM 按需触发
//! - **Command**：用户 / 宿主显式触发，一条明确指令执行特定任务

use chrono::Local;
use sea_orm::{
    Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体命令注册表
///
/// 对应数据库表 `t_agent_command_registry`。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_command_registry", comment = "智能体命令注册表")]
pub struct Model {
    /// 命令ID
    #[sea_orm(primary_key, auto_increment = true, comment = "命令ID")]
    pub id: i64,
    /// 所属用户ID
    #[sea_orm(comment = "所属用户ID", indexed)]
    pub user_id: i64,
    /// 命令名称（同一用户下唯一，排除已删除记录）
    #[sea_orm(
        column_type = "String(StringLen::N(128))",
        comment = "命令名称（同一用户下唯一，排除已删除记录）"
    )]
    pub name: String,
    /// 命令描述
    #[sea_orm(column_type = "Text", comment = "命令描述")]
    pub description: String,
    /// 命令提示词正文（完整的 Markdown 内容）
    #[sea_orm(
        column_type = "Text",
        comment = "命令提示词正文（完整的 Markdown 内容）"
    )]
    pub content: String,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 是否删除
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
