//! 智能体技能安装包实体（dense 格式）
//!
//! 记录通过内置安装通道（Agent 自助 / CLI）下载并入库的 Skill 物理包。
//! 包内容以 zip 二进制存储，运行时按需惰性解压到本地缓存目录使用。
//!
//! 本表与 [`skill_rel`](super::skill_rel) 解耦：
//! - `t_agent_skill_package`：物理包（`user_id + skill_code` 联合唯一）
//! - `t_agent_skill_rel`：Agent 与 skill 的绑定关系（通过 `skill_package_id` 外键关联）
//!
//! ## `user_id` 语义
//!
//! - `user_id = 0`：系统级预置包，全局共享，所有用户可引用
//! - `user_id > 0`：用户私有包，仅属于该用户，避免全局命名冲突

use chrono::Local;
use sea_orm::{
    IntoActiveModel, Set,
    entity::prelude::{async_trait::async_trait, *},
};
use serde::{Deserialize, Serialize};

/// 智能体技能安装包
///
/// 以 zip 字节流形式持久化 skill 目录的完整内容。
#[sea_orm::model]
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(
    table_name = "t_agent_skill_package",
    comment = "智能体技能安装包表（zip 二进制存储）"
)]
pub struct Model {
    /// 主键ID
    #[sea_orm(primary_key, auto_increment = true, comment = "主键ID")]
    pub id: i64,
    /// 所属用户ID（0 = 系统级全局包，>0 = 用户私有包）
    #[sea_orm(
        default_value = 0,
        indexed,
        comment = "所属用户ID（0 = 系统级全局包，>0 = 用户私有包）"
    )]
    pub user_id: i64,
    /// 技能唯一编码（= 解压后目录名 = SKILL.md 中 name 的 slug）
    #[sea_orm(
        column_type = "String(StringLen::N(128))",
        indexed,
        comment = "技能唯一编码（= 解压后目录名）"
    )]
    pub skill_code: String,
    /// 技能展示名（来自 SKILL.md frontmatter `name`）
    #[sea_orm(
        column_type = "String(StringLen::N(256))",
        default_value = "",
        comment = "技能展示名"
    )]
    pub name: String,
    /// 技能描述（来自 SKILL.md frontmatter `description`）
    #[sea_orm(column_type = "Text", default_value = "", comment = "技能描述")]
    pub description: String,
    /// 版本号（来自 SKILL.md frontmatter `version` 或 git tag，缺省 "0.0.0"）
    #[sea_orm(
        column_type = "String(StringLen::N(64))",
        default_value = "0.0.0",
        comment = "版本号"
    )]
    pub version: String,
    /// 来源类型：0=builtin, 1=agent_installed, 2=user_manual
    #[sea_orm(
        column_type = "SmallInteger",
        default_value = 1,
        comment = "来源类型: 0=builtin, 1=agent_installed, 2=user_manual"
    )]
    pub source_type: i16,
    /// 原始安装来源标识（git URL / npm 包名 / zip URL / file:// 路径）
    #[sea_orm(column_type = "Text", default_value = "", comment = "原始安装来源标识")]
    pub source_url: String,
    /// 包的 zip 二进制内容
    #[serde(skip)]
    #[sea_orm(column_type = "Blob", comment = "zip 二进制内容")]
    pub content_zip: Vec<u8>,
    /// zip 内容的 sha256（hex，用于完整性校验与缓存命中判断）
    #[sea_orm(
        column_type = "String(StringLen::N(64))",
        indexed,
        comment = "zip 内容的 sha256（hex）"
    )]
    pub content_sha256: String,
    /// zip 字节大小（冗余字段，便于过滤大包）
    #[sea_orm(default_value = 0, comment = "zip 字节大小")]
    pub size_bytes: i64,
    /// 安装者用户 ID（0 表示系统/启动时安装）
    #[sea_orm(default_value = 0, comment = "安装者用户ID（0 表示系统）")]
    pub installed_by: i64,
    /// 状态：0=Installed，1=Disabled，2=Broken
    #[sea_orm(
        column_type = "SmallInteger",
        default_value = 0,
        indexed,
        comment = "状态: 0=Installed, 1=Disabled, 2=Broken"
    )]
    pub status: i16,
    /// 是否已删除（软删除）
    #[sea_orm(default_value = false, comment = "是否删除")]
    pub deleted: bool,
    /// 创建时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "创建时间")]
    pub created_at: DateTime,
    /// 更新时间
    #[sea_orm(default_expr = "Expr::current_timestamp()", comment = "更新时间")]
    pub updated_at: DateTime,
}

impl Model {
    /// 执行软删除。
    ///
    /// # Parameters
    ///
    /// * `db` - 数据库连接
    pub async fn soft_delete<C>(self, db: &C) -> Result<(), DbErr>
    where
        C: ConnectionTrait,
    {
        let mut active: ActiveModel = self.into_active_model();
        active.deleted = Set(true);
        active.updated_at = Set(Local::now().naive_local());
        active.update(db).await?;
        Ok(())
    }

    /// 恢复软删除。
    ///
    /// # Parameters
    ///
    /// * `db` - 数据库连接
    pub async fn restore<C>(self, db: &C) -> Result<(), DbErr>
    where
        C: ConnectionTrait,
    {
        let mut active: ActiveModel = self.into_active_model();
        active.deleted = Set(false);
        active.updated_at = Set(Local::now().naive_local());
        active.update(db).await?;
        Ok(())
    }
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
