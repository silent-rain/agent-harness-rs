//! 工具注册表路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::tool_registry::ToolRegistryController;

/// 路由器
pub struct ToolRegistryRouter;

impl ToolRegistryRouter {
    /// 注册`工具注册表`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/tool_registries",
            Router::new()
                .route("/list", get(ToolRegistryController::list))
                .route("/create", post(ToolRegistryController::create))
                .route("/info", get(ToolRegistryController::info))
                .route("/update", put(ToolRegistryController::update))
                .route("/enabled", put(ToolRegistryController::update_enabled))
                .route("/delete", delete(ToolRegistryController::delete)),
        )
    }
}
