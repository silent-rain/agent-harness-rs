//! MCP 服务注册表实体
//!
//! 存储平台内建 MCP 服务与用户自定义 MCP 服务，
//! 支持在线市场分发与用户私有服务的统一管理。

use chrono::Local;
use sea_orm::{
    ActiveModelBehavior, ConnectionTrait, DbErr, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EnumIter, PrimaryKeyTrait, Set,
    prelude::{DateTime, async_trait::async_trait},
};
use serde::{Deserialize, Serialize};

/// MCP 服务注册表
///
/// 记录可供智能体绑定的 MCP 服务元数据与连接配置。
/// 通过 `t_agent_mcp_service_rel` 与智能体建立多对多关联。
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_agent_mcp_service_registry")]
pub struct Model {
    /// 服务ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属用户ID（0 表示平台内建）
    pub user_id: i64,
    /// 服务名称（同一用户下唯一）
    pub name: String,
    /// 服务描述
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// MCP 服务地址（stdio 类型为空）
    #[sea_orm(column_type = "Text", nullable)]
    pub mcp_service_url: Option<String>,
    /// 传输类型: stdio, sse, streamable-http
    pub transport_type: String,
    /// 允许使用的工具列表（JSON 数组，空=全部可用）
    #[sea_orm(column_type = "Text")]
    pub enabled_tools: String,
    /// MCP 服务配置（JSON），使用与本地 `mcp.json` 相同的 `mcpServers` 包裹格式，
    /// 便于用户直接拷贝粘贴。不同传输类型的示例如下：
    ///
    /// **stdio** — 本地子进程，通过 `command` + `args` 启动，可注入环境变量：
    /// ```json
    /// {
    ///   "mcpServers": {
    ///     "fetch-mcp": {
    ///       "command": "uvx",
    ///       "args": ["mcp-server-fetch"],
    ///       "transportType": "stdio",
    ///       "env": { "API_KEY": "your-api-key" }
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// **sse** — 基于 Server-Sent Events 的远程服务：
    /// ```json
    /// {
    ///   "mcpServers": {
    ///     "sse-service": {
    ///       "url": "http://127.0.0.1:8080/sse",
    ///       "transportType": "sse",
    ///       "timeout": 30000,
    ///       "headers": { "Authorization": "Bearer your-token" }
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// **streamable-http** — 基于 Streamable HTTP 的远程服务：
    /// ```json
    /// {
    ///   "mcpServers": {
    ///     "http-service": {
    ///       "url": "http://127.0.0.1:8081/mcp",
    ///       "transportType": "streamable-http",
    ///       "timeout": 30000,
    ///       "headers": { "X-Api-Key": "your-api-key" }
    ///     }
    ///   }
    /// }
    /// ```
    #[sea_orm(column_type = "Text", nullable)]
    pub config_json: Option<String>,
    /// 是否启用
    pub enabled: bool,
    /// 是否删除
    pub deleted: bool,
    /// 创建时间
    pub created_at: DateTime,
    /// 更新时间
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = Local::now().naive_local();
        if insert {
            self.created_at = Set(now);
        }
        self.updated_at = Set(now);
        Ok(self)
    }
}
