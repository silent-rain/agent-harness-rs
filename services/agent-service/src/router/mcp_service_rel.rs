//! 智能体 MCP 服务绑定关系路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::mcp_service_rel::McpServiceRelController;

/// 路由器
pub struct McpServiceRelRouter;

impl McpServiceRelRouter {
    /// 注册`MCP 服务绑定关系`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/mcp_service_rels",
            Router::new()
                .route("/list", get(McpServiceRelController::list))
                .route("/add", post(McpServiceRelController::add))
                .route("/update", put(McpServiceRelController::update))
                .route("/enabled", put(McpServiceRelController::update_enabled))
                .route("/delete", delete(McpServiceRelController::delete)),
        )
    }
}
