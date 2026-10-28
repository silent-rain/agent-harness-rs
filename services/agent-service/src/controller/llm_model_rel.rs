//! 智能体 LLM 模型绑定关系 HTTP 控制器

use axum::Extension;
use axum_context::Context;
use axum_response::{Responder, Response};
use axum_validator::{Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::llm_model_rel::{
        BatchSaveLlmModelRelsReq, BatchSaveLlmModelRelsResp, GetLlmModelRelsReq,
        GetLlmModelRelsResp, UpdateLlmModelRelEnabledReq, UpdateLlmModelRelReq,
    },
    service::llm_model_rel::LlmModelRelService,
};

/// 控制器
pub struct LlmModelRelController;

impl LlmModelRelController {
    /// 获取智能体的 LLM 模型绑定列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetLlmModelRelsReq>,
    ) -> Responder<GetLlmModelRelsResp> {
        let service: LlmModelRelService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 批量同步智能体 LLM 模型绑定（差异处理）
    pub async fn batch_save(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<BatchSaveLlmModelRelsReq>,
    ) -> Responder<BatchSaveLlmModelRelsResp> {
        let service: LlmModelRelService = provider.provide();
        let result = service.batch_save(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新 LLM 模型绑定
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateLlmModelRelReq>,
    ) -> Responder<()> {
        let service: LlmModelRelService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新 LLM 模型绑定启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateLlmModelRelEnabledReq>,
    ) -> Responder<()> {
        let service: LlmModelRelService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
