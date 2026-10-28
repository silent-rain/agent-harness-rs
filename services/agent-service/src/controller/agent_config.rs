//! 智能体配置 HTTP 控制器

use axum::Extension;
use axum_context::Context;
use axum_response::{Responder, Response};
use axum_validator::{Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::agent_config::{
        CreateAgentConfigReq, CreateAgentConfigResp, DeleteAgentConfigReq, GetAgentConfigReq,
        GetAgentConfigResp, GetAgentConfigsReq, GetAgentConfigsResp, UpdateAgentConfigReq,
    },
    service::agent_config::AgentConfigService,
};

/// 控制器
pub struct AgentConfigController;

impl AgentConfigController {
    /// 获取智能体列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetAgentConfigsReq>,
    ) -> Responder<GetAgentConfigsResp> {
        let service: AgentConfigService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取智能体详情
    pub async fn info(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetAgentConfigReq>,
    ) -> Responder<GetAgentConfigResp> {
        let service: AgentConfigService = provider.provide();
        let result = service.info(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建智能体配置
    pub async fn create(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateAgentConfigReq>,
    ) -> Responder<CreateAgentConfigResp> {
        let service: AgentConfigService = provider.provide();
        let result = service.create(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新智能体配置
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateAgentConfigReq>,
    ) -> Responder<()> {
        let service: AgentConfigService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除智能体配置
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteAgentConfigReq>,
    ) -> Responder<()> {
        let service: AgentConfigService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
