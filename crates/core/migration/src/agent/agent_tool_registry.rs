//! 工具注册表迁移
//! Entity: [`entity::agent::tool_registry::Model`]
//!
//! 平台内建工具与受控工具定义，包含函数 Schema。

use entity::agent::tool_registry;
use sea_orm::{
    ActiveValue::Set,
    DeriveIden, DeriveMigrationName, EntityTrait,
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
                    .table(ToolRegistry::Table)
                    .comment("工具注册表")
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ToolRegistry::Id)
                            .big_integer()
                            .primary_key()
                            .auto_increment()
                            .not_null()
                            .comment("工具ID"),
                    )
                    .col(
                        ColumnDef::new(ToolRegistry::ToolCode)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("工具唯一编码"),
                    )
                    .col(
                        ColumnDef::new(ToolRegistry::Name)
                            .string()
                            .string_len(128)
                            .not_null()
                            .comment("工具名称"),
                    )
                    .col(
                        ColumnDef::new(ToolRegistry::Description)
                            .text()
                            .not_null()
                            .comment("工具描述"),
                    )
                    .col(
                        ColumnDef::new(ToolRegistry::SchemaJson)
                            .text()
                            .null()
                            .comment("函数 Schema（JSON Schema 格式）"),
                    )
                    .col(
                        ColumnDef::new(ToolRegistry::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用"),
                    )
                    .col(
                        ColumnDef::new(ToolRegistry::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(ToolRegistry::UpdatedAt)
                            .date_time()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        if_not_exists_create_index(manager, ToolRegistry::Table, vec![ToolRegistry::ToolCode])
            .await?;

        // 预设数据
        {
            let db = manager.get_connection();

            // 使用 SeaORM 插入数据
            // password: 123456
            tool_registry::Entity::insert_many([
                tool_registry::ActiveModel {
                    tool_code: Set("list_dir".to_string()),
                    name: Set("目录列表工具".to_string()),
                    description: Set(
                        "列出指定目录下的文件和子目录信息，包括文件大小和类型。".to_string()
                    ),
                    schema_json: Set(None),
                    enabled: Set(true),
                    ..Default::default()
                },
                tool_registry::ActiveModel {
                    tool_code: Set("read_file".to_string()),
                    name: Set("文件读取工具".to_string()),
                    description: Set("读取指定路径的文件内容。支持按行范围读取。".to_string()),
                    schema_json: Set(None),
                    enabled: Set(true),
                    ..Default::default()
                },
                tool_registry::ActiveModel {
                    tool_code: Set("search_files".to_string()),
                    name: Set("文件搜索工具".to_string()),
                    description: Set("在指定目录中递归搜索匹配名称模式的文件。".to_string()),
                    schema_json: Set(None),
                    enabled: Set(true),
                    ..Default::default()
                },
                tool_registry::ActiveModel {
                    tool_code: Set("run_command".to_string()),
                    name: Set("受控终端命令执行工具".to_string()),
                    description: Set("执行 Shell 命令。".to_string()),
                    schema_json: Set(None),
                    enabled: Set(true),
                    ..Default::default()
                },
            ])
            .exec(db)
            .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ToolRegistry::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum ToolRegistry {
    #[sea_orm(iden = "t_agent_tool_registry")]
    Table,
    Id,
    ToolCode,
    Name,
    Description,
    SchemaJson,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
