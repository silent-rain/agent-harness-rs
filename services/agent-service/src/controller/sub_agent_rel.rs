//! 智能体子智能体绑定关系 HTTP 控制器

use axum::Extension;
use axum_context::Context;
use axum_response::{Responder, Response};
use axum_validator::{Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::sub_agent_rel::{
        AddSubAgentRelReq, DeleteSubAgentRelReq, GetSubAgentRelsReq, GetSubAgentRelsResp,
        UpdateSubAgentRelEnabledReq, UpdateSubAgentRelReq,
    },
    service::sub_agent_rel::SubAgentRelService,
};

/// 控制器
pub struct SubAgentRelController;

impl SubAgentRelController {
    /// 获取智能体的子智能体绑定列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetSubAgentRelsReq>,
    ) -> Responder<GetSubAgentRelsResp> {
        let service: SubAgentRelService = provider.provide();
        let (results, total) = service.list(ctx.get_user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 新增子智能体绑定
    pub async fn add(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<AddSubAgentRelReq>,
    ) -> Responder<()> {
        let service: SubAgentRelService = provider.provide();
        let _result = service.add(ctx.get_user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新子智能体绑定
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateSubAgentRelReq>,
    ) -> Responder<()> {
        let service: SubAgentRelService = provider.provide();
        let _result = service.update(ctx.get_user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新子智能体绑定启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateSubAgentRelEnabledReq>,
    ) -> Responder<()> {
        let service: SubAgentRelService = provider.provide();
        let _result = service.update_enabled(ctx.get_user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除子智能体绑定
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteSubAgentRelReq>,
    ) -> Responder<()> {
        let service: SubAgentRelService = provider.provide();
        let _result = service.delete(ctx.get_user_id(), req).await?;

        Ok(Response::ok())
    }
}
