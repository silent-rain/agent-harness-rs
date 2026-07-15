//! 智能体 A2A 服务绑定关系路由

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::controller::a2a_service_rel::A2aServiceRelController;

/// 路由器
pub struct A2aServiceRelRouter;

impl A2aServiceRelRouter {
    /// 注册`A2A 服务绑定关系`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/a2a_service_rels",
            Router::new()
                .route("/list", get(A2aServiceRelController::list))
                .route("/batch_save", post(A2aServiceRelController::batch_save))
                .route("/enabled", put(A2aServiceRelController::update_enabled)),
        )
    }
}
