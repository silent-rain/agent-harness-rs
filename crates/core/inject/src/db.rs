//! 多数据库连接类型定义
use database::DatabaseConnection;

/// 主库连接（读写）
#[derive(Clone, Debug)]
pub struct PrimaryDb(pub DatabaseConnection);

impl PrimaryDb {
    pub fn db(&self) -> &DatabaseConnection {
        &self.0
    }
}

impl From<DatabaseConnection> for PrimaryDb {
    fn from(db: DatabaseConnection) -> Self {
        PrimaryDb(db)
    }
}

/// 从库连接（只读）
#[derive(Clone, Debug)]
pub struct ReadDb(pub DatabaseConnection);

impl ReadDb {
    pub fn db(&self) -> &DatabaseConnection {
        &self.0
    }
}

impl From<DatabaseConnection> for ReadDb {
    fn from(db: DatabaseConnection) -> Self {
        ReadDb(db)
    }
}
