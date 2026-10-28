//! 临时校验：SQLite 内存库上跑全量 up/down（验证后删除）

use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, EntityTrait, Statement};

#[tokio::test]
async fn up_down() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();

    let tools = entity::agent::tool_registry::Entity::find()
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tools.len(), 4);
    println!(
        "tool: {} created_at={}",
        tools[0].tool_code, tools[0].created_at
    );

    let stmt = Statement::from_string(
        db.get_database_backend(),
        "SELECT count(*) AS c FROM sqlite_master WHERE type = 'table' AND name LIKE 't_agent_%'",
    );
    let row = db.query_one_raw(stmt).await.unwrap().unwrap();
    let count: i64 = row.try_get("", "c").unwrap();
    println!("t_agent_* 表数量 = {count}");

    Migrator::down(&db, None).await.unwrap();
    let stmt = Statement::from_string(
        db.get_database_backend(),
        "SELECT count(*) AS c FROM sqlite_master WHERE type = 'table' AND name LIKE 't_agent_%'",
    );
    let row = db.query_one_raw(stmt).await.unwrap().unwrap();
    let count: i64 = row.try_get("", "c").unwrap();
    println!("回滚后 t_agent_* 表数量 = {count}");
    assert_eq!(count, 0);
}
