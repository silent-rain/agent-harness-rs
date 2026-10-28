//! 智能体 MCP 服务绑定关系 HTTP 控制器

use axum::Extension;
use axum_context::Context;
use axum_response::{Responder, Response};
use axum_validator::{Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::mcp_service_rel::{
        AddMcpServiceRelReq, DeleteMcpServiceRelReq, GetMcpServiceRelsReq, GetMcpServiceRelsResp,
        UpdateMcpServiceRelEnabledReq, UpdateMcpServiceRelReq,
    },
    service::mcp_service_rel::McpServiceRelService,
};

/// 控制器
pub struct McpServiceRelController;

impl McpServiceRelController {
    /// 获取智能体的 MCP 服务绑定列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetMcpServiceRelsReq>,
    ) -> Responder<GetMcpServiceRelsResp> {
        let service: McpServiceRelService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 新增 MCP 服务绑定
    pub async fn add(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<AddMcpServiceRelReq>,
    ) -> Responder<()> {
        let service: McpServiceRelService = provider.provide();
        let _result = service.add(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新 MCP 服务绑定
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateMcpServiceRelReq>,
    ) -> Responder<()> {
        let service: McpServiceRelService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新 MCP 服务绑定启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateMcpServiceRelEnabledReq>,
    ) -> Responder<()> {
        let service: McpServiceRelService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除 MCP 服务绑定
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteMcpServiceRelReq>,
    ) -> Responder<()> {
        let service: McpServiceRelService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
