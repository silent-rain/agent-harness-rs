//! 规则注册表路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::rule_registry::RuleRegistryController;

/// 路由器
pub struct RuleRegistryRouter;

impl RuleRegistryRouter {
    /// 注册`规则注册表`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/rule_registries",
            Router::new()
                .route("/list", get(RuleRegistryController::list))
                .route("/create", post(RuleRegistryController::create))
                .route("/info", get(RuleRegistryController::info))
                .route("/update", put(RuleRegistryController::update))
                .route("/enabled", put(RuleRegistryController::update_enabled))
                .route("/delete", delete(RuleRegistryController::delete)),
        )
    }
}
