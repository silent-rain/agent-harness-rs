//! 智能体命令注册表实体
//!
//! 存储平台内建命令与用户自定义命令。
//! 命令（Command）是一种面向开发场景的指令式交互能力，用于将常见、高频、
//! 结构化的开发任务封装为可复用的命令。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体命令注册表
///
/// 与 Skill 的核心区别：
/// - **Skill**：模型自主调用，注入 System Prompt，LLM 按需触发
/// - **Command**：用户 / 宿主显式触发，一条明确指令执行特定任务
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_command_registry")]
pub struct Model {
    /// 命令ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID
    pub user_id: i64,
    /// 命令名称（同一用户下唯一，对应本地文件名）
    pub name: String,
    /// 命令描述
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// 命令提示词正文（完整的 Markdown 内容）
    #[sea_orm(column_type = "Text")]
    pub content: String,
    /// 是否启用
    pub enabled: bool,
    /// 是否删除
    pub deleted: bool,
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
