//! 命令注册表路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::command_registry::CommandRegistryController;

/// 路由器
pub struct CommandRegistryRouter;

impl CommandRegistryRouter {
    /// 注册`命令注册表`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/command_registries",
            Router::new()
                .route("/list", get(CommandRegistryController::list))
                .route("/create", post(CommandRegistryController::create))
                .route("/info", get(CommandRegistryController::info))
                .route("/update", put(CommandRegistryController::update))
                .route("/enabled", put(CommandRegistryController::update_enabled))
                .route("/delete", delete(CommandRegistryController::delete)),
        )
    }
}
