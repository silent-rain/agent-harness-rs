//! 智能体会话事件流表迁移（entity-first）
//! Entity: [`entity::agent::SessionEvent`]
//!
//! 设计要点：
//! - 仅追加写入（append-only），按 session 维度顺序读取
//! - 顺序由自增主键 `id` 保证（已天然单调），不再单独维护 `seq`
//! - `payload_json` 承载完整事件内容（结构对应 `AgentStreamEvent`）
//! - 通过 `event_type` 二级筛选
//! - `expires_at` / `deleted` 支持 TTL 与软删除

use entity::agent::{SessionEvent, session_event};
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;

use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 实体优先：列类型/默认值/注释/索引由实体属性派生
        db.get_schema_builder()
            .register(SessionEvent)
            .apply(db)
            .await?;

        // 实体属性无法表达的联合索引
        // 按 session 顺序回放（依靠自增 id 单调性）
        if_not_exists_create_index(
            manager,
            SessionEvent,
            vec![session_event::Column::SessionId, session_event::Column::Id],
        )
        .await?;
        // 按 session + 时间扫描
        if_not_exists_create_index(
            manager,
            SessionEvent,
            vec![
                session_event::Column::SessionId,
                session_event::Column::CreatedAt,
            ],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SessionEvent).if_exists().to_owned())
            .await
    }
}
