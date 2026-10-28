//! LLM 模型配置 HTTP 控制器

use axum::Extension;
use axum::extract::{Json, Query};
use axum_context::Context;
use axum_response::{Responder, Response};
use inject::AInjectProvider;

use crate::{
    dto::llm_model::{
        CreateLlmModelReq, CreateLlmModelResp, DeleteLlmModelReq, GetLlmModelReq, GetLlmModelResp,
        GetLlmModelsReq, GetLlmModelsResp, UpdateLlmModelEnabledReq, UpdateLlmModelReq,
    },
    service::llm_model::LlmModelService,
};

/// 控制器
pub struct LlmModelController;

impl LlmModelController {
    /// 获取当前用户的 LLM 模型列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetLlmModelsReq>,
    ) -> Responder<GetLlmModelsResp> {
        let service: LlmModelService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取 LLM 模型详情
    pub async fn info(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetLlmModelReq>,
    ) -> Responder<GetLlmModelResp> {
        let service: LlmModelService = provider.provide();
        let result = service.info(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建 LLM 模型
    pub async fn create(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateLlmModelReq>,
    ) -> Responder<CreateLlmModelResp> {
        let service: LlmModelService = provider.provide();
        let result = service.create(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新 LLM 模型
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateLlmModelReq>,
    ) -> Responder<()> {
        let service: LlmModelService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新 LLM 模型启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateLlmModelEnabledReq>,
    ) -> Responder<()> {
        let service: LlmModelService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除 LLM 模型
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteLlmModelReq>,
    ) -> Responder<()> {
        let service: LlmModelService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
