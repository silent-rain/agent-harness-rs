//! 工具集
#![allow(unused)]
use sea_orm::{
    ConnectionTrait, DatabaseBackend, Iden,
    sea_query::{Index, IntoIden, IntoIndexColumn, IntoTableRef},
};
use sea_orm_migration::{DbErr, SchemaManager};

/// 设置索引
///
/// 如果不存在则设置索引
pub async fn if_not_exists_create_index<T, C>(
    manager: &SchemaManager<'_>,
    table: T,
    cols: Vec<C>,
) -> Result<(), DbErr>
where
    T: IntoTableRef + Iden,
    C: IntoIndexColumn + Iden + IntoIden,
{
    if cols.is_empty() {
        return Ok(());
    }

    let mut names = vec!["idx".to_string(), table.to_string()];
    for col in &cols {
        names.push(col.to_string());
    }
    let name_str = names.join("_");

    let mut stmt = Index::create();
    stmt.if_not_exists().name(name_str).table(table);
    for col in cols.into_iter() {
        stmt.col(col);
    }

    manager.create_index(stmt.to_owned()).await?;

    Ok(())
}

/// 设置联合索引
///
/// 如果不存在则设置索引
pub async fn if_not_exists_create_unique_index<T, C>(
    manager: &SchemaManager<'_>,
    table: T,
    cols: Vec<C>,
) -> Result<(), DbErr>
where
    T: IntoTableRef + Iden,
    C: IntoIndexColumn + Iden + IntoIden,
{
    if cols.is_empty() {
        return Ok(());
    }

    let mut names = vec!["uni_".to_string(), table.to_string()];
    for col in &cols {
        names.push(col.to_string());
    }
    let name_str = names.join("_");

    let mut stmt = Index::create();
    stmt.if_not_exists().name(name_str).table(table).unique();
    for col in cols.into_iter() {
        stmt.col(col);
    }

    manager.create_index(stmt.to_owned()).await?;

    Ok(())
}

/// 创建触发器
///
/// 如果不存在则创建触发器
///
/// MySQL/SQLite, 使用 NEW 和 OLD 关键字访问新/旧数据。
/// SQLite, 不支持直接修改 NEW 的值，需要通过 UPDATE 语句修改数据。
pub async fn if_not_exists_create_trigger_for_updated_at<T, C>(
    manager: &SchemaManager<'_>,
    table: T,
    updated_at_column: C,
) -> Result<(), DbErr>
where
    T: IntoTableRef + Iden,
    C: IntoIndexColumn + Iden + IntoIden,
{
    let db = manager.get_connection();

    let table_name = table.to_string();
    let updated_at_column = updated_at_column.to_string();

    let trigger_sql = match db.get_database_backend() {
        DatabaseBackend::MySql | DatabaseBackend::Postgres => {
            format!(
                r#"
                CREATE TRIGGER IF NOT EXISTS trg_update_{}_updated_at
                BEFORE UPDATE ON {}
                FOR EACH ROW
                BEGIN
                    SET NEW.{} = CURRENT_TIMESTAMP;
                END;
                "#,
                table_name, table_name, updated_at_column
            )
        }
        DatabaseBackend::Sqlite => {
            format!(
                r#"
                CREATE TRIGGER IF NOT EXISTS trg_update_{}_updated_at
                BEFORE UPDATE ON {}
                BEGIN
                    UPDATE {} SET {} = CURRENT_TIMESTAMP WHERE id = NEW.id;
                END;
                "#,
                table_name, table_name, table_name, updated_at_column
            )
        }
    };

    db.execute_unprepared(&trigger_sql).await?;
    Ok(())
}
