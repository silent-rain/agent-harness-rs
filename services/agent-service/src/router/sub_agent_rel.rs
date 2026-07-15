//! 智能体子智能体绑定关系路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::sub_agent_rel::SubAgentRelController;

/// 路由器
pub struct SubAgentRelRouter;

impl SubAgentRelRouter {
    /// 注册`子智能体绑定关系`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/sub_agent_rels",
            Router::new()
                .route("/list", get(SubAgentRelController::list))
                .route("/add", post(SubAgentRelController::add))
                .route("/update", put(SubAgentRelController::update))
                .route("/enabled", put(SubAgentRelController::update_enabled))
                .route("/delete", delete(SubAgentRelController::delete)),
        )
    }
}
