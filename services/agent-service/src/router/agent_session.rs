//! 智能体会话管理路由

use axum::{Router, routing::post};

use crate::controller::agent_session::AgentSessionController;

/// 路由器
pub struct AgentSessionRouter;

impl AgentSessionRouter {
    /// 注册`智能体会话管理`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/session",
            Router::new()
                .route("/cancel", post(AgentSessionController::cancel))
                .route("/status", post(AgentSessionController::status))
                .route(
                    "/inject-message",
                    post(AgentSessionController::inject_message),
                ),
        )
    }
}
