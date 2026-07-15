//! A2A 服务注册表路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::a2a_service_registry::A2aServiceRegistryController;

/// 路由器
pub struct A2aServiceRegistryRouter;

impl A2aServiceRegistryRouter {
    /// 注册`A2A 服务注册表`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/a2a_service_registries",
            Router::new()
                .route("/list", get(A2aServiceRegistryController::list))
                .route("/info", get(A2aServiceRegistryController::info))
                .route("/create", post(A2aServiceRegistryController::create))
                .route("/update", put(A2aServiceRegistryController::update))
                .route(
                    "/enabled",
                    put(A2aServiceRegistryController::update_enabled),
                )
                .route("/delete", delete(A2aServiceRegistryController::delete)),
        )
    }
}
