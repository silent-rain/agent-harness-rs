//! 智能体配置路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::agent_config::AgentConfigController;

/// 路由器
pub struct AgentConfigRouter;

impl AgentConfigRouter {
    /// 注册`智能体配置`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/agent_configs",
            Router::new()
                .route("/list", get(AgentConfigController::list))
                .route("/create", post(AgentConfigController::create))
                .route("/info", get(AgentConfigController::info))
                .route("/update", put(AgentConfigController::update))
                .route("/delete", delete(AgentConfigController::delete)),
        )
    }
}
