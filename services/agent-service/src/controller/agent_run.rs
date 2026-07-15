//! 智能体运行 HTTP 控制器

use std::convert::Infallible;

use axum::{
    Extension, Json,
    response::sse::{Event, Sse},
};
use axum_context::Context;
use axum_response::{Responder, Response, ResponseErr};
use inject::AInjectProvider;

use agent_base::{openai::ChatCompletion, session::RunOutcome};

use crate::{
    dto::agent_run::{AgentConfigRunReq, AgentDebugRunReq, AgentRunReq},
    service::agent_run::AgentRunService,
};

/// 控制器
pub struct AgentRunController;

impl AgentRunController {
    /// 标准非流式运行（OpenAI 兼容）
    ///
    /// 使用 `AgentOpenAI::run` 推理，返回 OpenAI Chat Completions 格式响应。
    pub async fn run(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<AgentRunReq>,
    ) -> Responder<ChatCompletion> {
        let service: AgentRunService = provider.provide();
        let resp = service.run(ctx.get_user_id(), req).await?;
        Ok(Response::data(resp).to_json()?)
    }

    /// 标准流式运行（OpenAI 兼容，SSE）
    ///
    /// 使用 `AgentOpenAI::run_stream` 推理，通过 SSE 推送 OpenAI Chat Completions stream 分片。
    pub async fn run_stream(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<AgentRunReq>,
    ) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ResponseErr> {
        let service: AgentRunService = provider.provide();
        let sse_stream = service.run_stream(ctx.get_user_id(), req).await?;
        Ok(Sse::new(sse_stream))
    }

    /// 调试非流式运行（Agent 内部事件）
    ///
    /// 使用 `Agent::run` 推理，返回完整的 `RunOutcome`，包含最终回答与步骤统计。
    pub async fn debug_run(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<AgentDebugRunReq>,
    ) -> Responder<RunOutcome> {
        let service: AgentRunService = provider.provide();
        let outcome = service.debug_run(ctx.get_user_id(), req).await?;
        Ok(Response::data(outcome).to_json()?)
    }

    /// 调试流式运行（Agent 内部事件流，SSE）
    ///
    /// 使用 `Agent::run_stream` 推理，通过 SSE 推送内部事件，包含详细的调试信息。
    pub async fn debug_run_stream(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<AgentDebugRunReq>,
    ) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ResponseErr> {
        let service: AgentRunService = provider.provide();
        let sse_stream = service.debug_run_stream(ctx.get_user_id(), req).await?;

        Ok(Sse::new(sse_stream))
    }

    /// 通用非流式运行（OpenAI 兼容，配置从请求传入）
    ///
    /// 使用 `AgentOpenAI::run` 推理，`agent_config` 与 `model_config` 直接从请求中获取。
    pub async fn run_with_config(
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<AgentConfigRunReq>,
    ) -> Responder<ChatCompletion> {
        let service: AgentRunService = provider.provide();
        let resp = service.run_with_config(req).await?;
        Ok(Response::data(resp).to_json()?)
    }

    /// 通用流式运行（OpenAI 兼容，配置从请求传入，SSE）
    ///
    /// 使用 `AgentOpenAI::run_stream` 推理，`agent_config` 与 `model_config` 直接从请求中获取，
    /// 通过 SSE 推送 OpenAI Chat Completions stream 分片。
    pub async fn run_stream_with_config(
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<AgentConfigRunReq>,
    ) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ResponseErr> {
        let service: AgentRunService = provider.provide();
        let sse_stream = service.run_stream_with_config(req).await?;
        Ok(Sse::new(sse_stream))
    }
}
