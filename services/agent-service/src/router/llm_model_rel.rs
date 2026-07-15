//! 智能体 LLM 模型绑定关系路由

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::controller::llm_model_rel::LlmModelRelController;

/// 路由器
pub struct LlmModelRelRouter;

impl LlmModelRelRouter {
    /// 注册`LLM 模型绑定关系`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/llm_model_rels",
            Router::new()
                .route("/list", get(LlmModelRelController::list))
                .route("/batch_save", post(LlmModelRelController::batch_save))
                .route("/update", put(LlmModelRelController::update))
                .route("/enabled", put(LlmModelRelController::update_enabled)),
        )
    }
}
