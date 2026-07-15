//! LLM 模型配置路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::llm_model::LlmModelController;

/// 路由器
pub struct LlmModelRouter;

impl LlmModelRouter {
    /// 注册`LLM 模型配置`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/llm_models",
            Router::new()
                .route("/list", get(LlmModelController::list))
                .route("/info", get(LlmModelController::info))
                .route("/create", post(LlmModelController::create))
                .route("/update", put(LlmModelController::update))
                .route("/enabled", put(LlmModelController::update_enabled))
                .route("/delete", delete(LlmModelController::delete)),
        )
    }
}
