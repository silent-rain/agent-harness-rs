//! 智能体命令绑定关系路由

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::controller::command_rel::CommandRelController;

/// 路由器
pub struct CommandRelRouter;

impl CommandRelRouter {
    /// 注册`命令绑定关系`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/command_rels",
            Router::new()
                .route("/list", get(CommandRelController::list))
                .route("/batch_save", post(CommandRelController::batch_save))
                .route("/update", put(CommandRelController::update))
                .route("/enabled", put(CommandRelController::update_enabled)),
        )
    }
}
