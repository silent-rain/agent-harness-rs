//! Configuration module for dependency injection

use crate::{PrimaryDb, ReadDb};

/// 数据库配置结构体，用于组装 InjectProvider 所需的数据库连接
pub struct InjectProviderConfig {
    pub primary: PrimaryDb,
    pub read: ReadDb,
}
