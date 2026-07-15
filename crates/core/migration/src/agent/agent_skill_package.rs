//! 智能体技能安装包迁移
//! Entity: [`entity::agent::skill_package::Model`]
//!
//! 以 zip 二进制形式存储 Skill 物理包，配合 `t_agent_skill_rel` 实现
//! "包存储 + 绑定关系"解耦。
//!
//! `(user_id, skill_code)` 联合唯一：
//! - `user_id = 0`：系统级全局包，可被所有用户引用
//! - `user_id > 0`：用户私有包，避免不同用户间的命名冲突

use sea_orm::{
    DeriveIden, DeriveMigrationName,
    sea_query::{ColumnDef, Expr, Table},
};
use sea_orm_migration::{DbErr, MigrationTrait, SchemaManager, async_trait};

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SkillPackage::Table)
                    .comment("智能体技能安装包表（zip 二进制存储）")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SkillPackage::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("主键ID"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::UserId)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("所属用户ID（0 = 系统级全局包，>0 = 用户私有包）"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::SkillCode)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("技能唯一编码（= 解压后目录名）"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::Name)
                            .string()
                            .string_len(256)
                            .not_null()
                            .default("")
                            .comment("技能展示名"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::Description)
                            .text()
                            .not_null()
                            .default("")
                            .comment("技能描述"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::Version)
                            .string()
                            .string_len(64)
                            .not_null()
                            .default("0.0.0")
                            .comment("版本号"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::SourceType)
                            .small_integer()
                            .not_null()
                            .default(1)
                            .comment("来源类型: 0=builtin, 1=agent_installed, 2=user_manual"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::SourceUrl)
                            .text()
                            .not_null()
                            .default("")
                            .comment("原始安装来源标识"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::ContentZip)
                            .blob()
                            .not_null()
                            .comment("zip 二进制内容"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::ContentSha256)
                            .string()
                            .string_len(64)
                            .not_null()
                            .comment("zip 内容的 sha256（hex）"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::SizeBytes)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("zip 字节大小"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::InstalledBy)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("安装者用户ID（0 表示系统）"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::Status)
                            .small_integer()
                            .not_null()
                            .default(0)
                            .comment("状态: 0=Installed, 1=Disabled, 2=Broken"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::Deleted)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("是否删除"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(SkillPackage::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, SkillPackage::Table, vec![SkillPackage::UserId])
            .await?;
        if_not_exists_create_index(manager, SkillPackage::Table, vec![SkillPackage::SkillCode])
            .await?;
        // (user_id, skill_code) 联合唯一索引
        if_not_exists_create_index(
            manager,
            SkillPackage::Table,
            vec![SkillPackage::UserId, SkillPackage::SkillCode],
        )
        .await?;
        if_not_exists_create_index(manager, SkillPackage::Table, vec![SkillPackage::Status])
            .await?;
        if_not_exists_create_index(
            manager,
            SkillPackage::Table,
            vec![SkillPackage::ContentSha256],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SkillPackage::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum SkillPackage {
    #[sea_orm(iden = "t_agent_skill_package")]
    Table,
    Id,
    UserId,
    SkillCode,
    Name,
    Description,
    Version,
    SourceType,
    SourceUrl,
    ContentZip,
    ContentSha256,
    SizeBytes,
    InstalledBy,
    Status,
    Deleted,
    CreatedAt,
    UpdatedAt,
}
