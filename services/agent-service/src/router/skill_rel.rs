//! 智能体技能绑定关系路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::skill_rel::SkillRelController;

/// 路由器
pub struct SkillRelRouter;

impl SkillRelRouter {
    /// 注册`技能绑定关系`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/skill_rels",
            Router::new()
                .route("/list", get(SkillRelController::list))
                .route("/info", get(SkillRelController::info))
                .route("/create", post(SkillRelController::create))
                .route("/update", put(SkillRelController::update))
                .route("/enabled", put(SkillRelController::update_enabled))
                .route("/delete", delete(SkillRelController::delete)),
        )
    }
}
