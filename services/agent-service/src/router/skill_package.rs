//! 技能安装包路由

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::controller::skill_package::SkillPackageController;

/// 路由器
pub struct SkillPackageRouter;

impl SkillPackageRouter {
    /// 注册`技能安装包`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/skill_packages",
            Router::new()
                .route("/list", get(SkillPackageController::list))
                .route("/create", post(SkillPackageController::create))
                .route("/info", get(SkillPackageController::info))
                .route("/status", put(SkillPackageController::update_status))
                .route("/delete", delete(SkillPackageController::delete)),
        )
    }
}
