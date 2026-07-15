//! 智能体规则绑定关系路由

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::controller::rule_rel::RuleRelController;

/// 路由器
pub struct RuleRelRouter;

impl RuleRelRouter {
    /// 注册`规则绑定关系`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/rule_rels",
            Router::new()
                .route("/list", get(RuleRelController::list))
                .route("/batch_save", post(RuleRelController::batch_save))
                .route("/update", put(RuleRelController::update))
                .route("/enabled", put(RuleRelController::update_enabled)),
        )
    }
}
