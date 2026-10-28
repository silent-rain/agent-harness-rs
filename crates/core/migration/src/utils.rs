//! 工具集
use sea_orm::sea_query::{Index, IntoIden, IntoIndexColumn, IntoTableRef};
use sea_orm::{ConnectionTrait, DatabaseBackend, Iden};
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
    let trigger_name = format!("trg_update_{}_updated_at", table_name);

    match db.get_database_backend() {
        DatabaseBackend::Postgres => {
            // Postgres 不支持 `CREATE TRIGGER IF NOT EXISTS`，且触发器不能 inline body，
            // 必须引用已存在的函数。因此先幂等创建函数，再 DROP 已存在触发器后重建。
            // 注：触发器函数固定更新 `updated_at` 列，要求目标表时间列名为 `updated_at`。
            db.execute_unprepared(
                r#"CREATE OR REPLACE FUNCTION set_updated_at() RETURNS TRIGGER AS $$
                   BEGIN
                       NEW.updated_at = NOW();
                       RETURN NEW;
                   END;
                   $$ LANGUAGE plpgsql;"#,
            )
            .await?;

            db.execute_unprepared(&format!(
                "DROP TRIGGER IF EXISTS {} ON {};",
                trigger_name, table_name
            ))
            .await?;

            db.execute_unprepared(&format!(
                "CREATE TRIGGER {} BEFORE UPDATE ON {} FOR EACH ROW EXECUTE FUNCTION set_updated_at();",
                trigger_name, table_name
            ))
            .await?;
        }
        DatabaseBackend::MySql => {
            db.execute_unprepared(&format!(
                r#"
                CREATE TRIGGER IF NOT EXISTS {}
                BEFORE UPDATE ON {}
                FOR EACH ROW
                BEGIN
                    SET NEW.{} = CURRENT_TIMESTAMP;
                END;
                "#,
                trigger_name, table_name, updated_at_column
            ))
            .await?;
        }
        _ => {
            // SQLite 不支持直接修改 NEW，需通过 UPDATE 语句回写
            db.execute_unprepared(&format!(
                r#"
                CREATE TRIGGER IF NOT EXISTS {}
                BEFORE UPDATE ON {}
                BEGIN
                    UPDATE {} SET {} = CURRENT_TIMESTAMP WHERE id = NEW.id;
                END;
                "#,
                trigger_name, table_name, table_name, updated_at_column
            ))
            .await?;
        }
    }

    Ok(())
}
