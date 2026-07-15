//! 智能体会话管理 HTTP 控制器
use axum::{Extension, Json};
use axum_context::Context;
use axum_response::{Responder, Response};
use inject::AInjectProvider;

use crate::{
    dto::agent_session::{
        InjectMessageReq, InjectMessageResp, SessionCancelReq, SessionCancelResp, SessionStatusReq,
        SessionStatusResp,
    },
    service::agent_session::AgentSessionService,
};

/// 控制器
pub struct AgentSessionController;

impl AgentSessionController {
    /// 取消会话运行
    ///
    /// 将指定会话的状态更新为 Cancelled，Agent 运行循环检测到后退出。
    pub async fn cancel(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<SessionCancelReq>,
    ) -> Responder<SessionCancelResp> {
        let service: AgentSessionService = provider.provide();
        let user_id = ctx.get_user_id().to_string();
        let resp = service.cancel(&user_id, &req.session_id).await?;
        Ok(Response::data(resp).to_json()?)
    }

    /// 查询会话状态
    ///
    /// 从数据库读取会话当前状态。
    pub async fn status(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<SessionStatusReq>,
    ) -> Responder<SessionStatusResp> {
        let service: AgentSessionService = provider.provide();
        let user_id = ctx.get_user_id().to_string();
        let resp = service.status(&user_id, &req.session_id).await?;
        Ok(Response::data(resp).to_json()?)
    }

    /// 向运行中的会话注入用户消息
    ///
    /// 消息写入 `t_agent_session_pending_input` 队列表，Agent 运行循环在下一轮自动消费。
    pub async fn inject_message(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<InjectMessageReq>,
    ) -> Responder<InjectMessageResp> {
        let service: AgentSessionService = provider.provide();
        let user_id = ctx.get_user_id().to_string();
        let resp = service.inject_message(&user_id, req).await?;
        Ok(Response::data(resp).to_json()?)
    }
}
