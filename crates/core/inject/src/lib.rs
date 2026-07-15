//! 依赖注入
use std::sync::Arc;

use nject::Injectable;
pub use nject::{injectable, provider};

use database::DatabaseConnection;

mod db;
pub use db::{PrimaryDb, ReadDb};

mod config;
pub use config::InjectProviderConfig;

pub type AInjectProvider = Arc<InjectProvider>;

impl<'a> Injectable<'a, Arc<DatabaseConnection>, InjectProvider> for Arc<DatabaseConnection> {
    fn inject(provider: &'a InjectProvider) -> Self {
        Arc::new(provider.primary.db().clone())
    }
}

/// Application dependency provider
///
/// Holds all database connections and application services.
/// Multiple databases can be added as separate fields when needed.
#[provider]
pub struct InjectProvider {
    #[provide(PrimaryDb, |db| db.clone())]
    pub(crate) primary: PrimaryDb,

    #[provide(ReadDb, |db| db.clone())]
    pub(crate) read: ReadDb,
}

impl InjectProvider {
    /// 从 DatabaseConfig 创建 InjectProvider
    ///
    /// # Example
    /// ```rust,no_run
    /// use database::DatabaseConnection;
    /// use inject::{InjectProvider, InjectProviderConfig, PrimaryDb, ReadDb};
    ///
    /// # async fn example() {
    /// let db = DatabaseConnection::default();
    /// let config = InjectProviderConfig {
    ///     primary: PrimaryDb::from(db.clone()),
    ///     read: ReadDb::from(db),
    /// };
    /// let provider = InjectProvider::new(config);
    /// # }
    /// ```
    pub fn new(config: InjectProviderConfig) -> Self {
        InjectProvider {
            primary: config.primary,
            read: config.read,
        }
    }
}
