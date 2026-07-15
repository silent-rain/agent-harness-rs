//! 智能体技能安装包实体
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
    ActiveModelBehavior, ActiveModelTrait, ConnectionTrait, DbErr, DeriveEntityModel,
    DerivePrimaryKey, DeriveRelation, EnumIter, IntoActiveModel, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// 智能体技能安装包
///
/// 以 zip 字节流形式持久化 skill 目录的完整内容。
/// `(user_id, skill_code)` 联合唯一：
/// - `user_id = 0` 为系统级全局包，可被所有用户引用
/// - `user_id > 0` 为用户私有包，避免不同用户间的命名冲突
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_skill_package")]
pub struct Model {
    /// 主键ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID（0 = 系统级全局包，>0 = 用户私有包）
    pub user_id: i64,
    /// 技能唯一编码（= 解压后目录名 = SKILL.md 中 name 的 slug）
    pub skill_code: String,
    /// 技能展示名（来自 SKILL.md frontmatter `name`）
    pub name: String,
    /// 技能描述（来自 SKILL.md frontmatter `description`）
    pub description: String,
    /// 版本号（来自 SKILL.md frontmatter `version` 或 git tag，缺省 "0.0.0"）
    pub version: String,
    /// 来源类型：复用 `enums::skill_rel::SkillSourceType`
    /// （0=builtin, 1=agent_installed, 2=user_manual）
    pub source_type: i16,
    /// 原始安装来源标识（git URL / npm 包名 / zip URL / file:// 路径）
    pub source_url: String,
    /// 包的 zip 二进制内容
    #[serde(skip)]
    pub content_zip: Vec<u8>,
    /// zip 内容的 sha256（hex，用于完整性校验与缓存命中判断）
    pub content_sha256: String,
    /// zip 字节大小（冗余字段，便于过滤大包）
    pub size_bytes: i64,
    /// 安装者用户 ID（0 表示系统/启动时安装）
    pub installed_by: i64,
    /// 状态：0=Installed，1=Disabled，2=Broken
    pub status: i16,
    /// 是否已删除（软删除）
    pub deleted: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
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
        active.updated_at = Set(chrono::Local::now().naive_local());
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
        active.updated_at = Set(chrono::Local::now().naive_local());
        active.update(db).await?;
        Ok(())
    }
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
