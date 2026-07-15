//! MCP 服务注册表路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::mcp_service_registry::McpServiceRegistryController;

/// 路由器
pub struct McpServiceRegistryRouter;

impl McpServiceRegistryRouter {
    /// 注册`MCP 服务注册表`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/mcp_service_registries",
            Router::new()
                .route("/list", get(McpServiceRegistryController::list))
                .route("/info", get(McpServiceRegistryController::info))
                .route("/create", post(McpServiceRegistryController::create))
                .route("/update", put(McpServiceRegistryController::update))
                .route(
                    "/enabled",
                    put(McpServiceRegistryController::update_enabled),
                )
                .route("/delete", delete(McpServiceRegistryController::delete)),
        )
    }
}
