//! 智能体会话注入消息队列迁移（entity-first）
//! Entity: [`entity::agent::SessionPendingInput`]
//!
//! 接口层写入、ReAct 循环消费的 FIFO 队列。

use entity::agent::{SessionPendingInput, session_pending_input};
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释由实体属性派生
        db.get_schema_builder()
            .register(SessionPendingInput)
            .apply(db)
            .await?;

        // 核心查询索引：按 session 查找未消费记录
        if_not_exists_create_index(
            manager,
            SessionPendingInput,
            vec![
                session_pending_input::Column::SessionId,
                session_pending_input::Column::ConsumedAt,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(SessionPendingInput)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
