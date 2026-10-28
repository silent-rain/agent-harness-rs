//! 工具注册表初始数据
//!
//! Entity: [`entity::agent::ToolRegistry`]
//!
//! 预置平台内建工具：目录列表、文件读取、文件搜索、受控命令执行。

use entity::agent::tool_registry;
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use sea_orm_migration::prelude::*;

const TOOL_CODES: [&str; 4] = ["list_dir", "read_file", "search_files", "run_command"];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // `insert_many` 不触发 `before_save`，需显式补齐时间戳
        let models = [
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
        ]
        .map(|mut model| {
            model.ensure_timestamps();
            model
        });

        tool_registry::Entity::insert_many(models).exec(db).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        tool_registry::Entity::delete_many()
            .filter(tool_registry::Column::ToolCode.is_in(TOOL_CODES))
            .exec(db)
            .await?;

        Ok(())
    }
}
