//! 智能体工具绑定关系路由

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::controller::tool_rel::ToolRelController;

/// 路由器
pub struct ToolRelRouter;

impl ToolRelRouter {
    /// 注册`工具绑定关系`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/tool_rels",
            Router::new()
                .route("/list", get(ToolRelController::list))
                .route("/batch_save", post(ToolRelController::batch_save))
                .route("/update", put(ToolRelController::update))
                .route("/enabled", put(ToolRelController::update_enabled)),
        )
    }
}
