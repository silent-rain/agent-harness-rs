//! 智能体运行路由

use axum::{Router, routing::post};

use crate::controller::agent_run::AgentRunController;

/// 路由器
pub struct AgentRunRouter;

impl AgentRunRouter {
    /// 注册`智能体运行`路由
    pub fn register() -> Router {
        Router::new().nest(
            "/agent",
            Router::new()
                // 标准运行（AgentOpenAI 协议，返回 OpenAI 兼容格式）
                .route("/run", post(AgentRunController::run))
                .route("/stream", post(AgentRunController::run_stream))
                // 调试运行（Agent 内部事件流，返回详细调试信息）
                .route("/debug_run", post(AgentRunController::debug_run))
                .route("/debug_stream", post(AgentRunController::debug_run_stream))
                // 通用配置运行（agent_config 与 model 从请求传入）
                .route(
                    "/run_with_config",
                    post(AgentRunController::run_with_config),
                )
                .route(
                    "/stream_with_config",
                    post(AgentRunController::run_stream_with_config),
                ),
        )
    }
}
