//! MCP 服务注册表 HTTP 控制器

use axum::Extension;
use axum_context::Context;
use axum_response::{Responder, Response};
use axum_validator::{Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::mcp_service_registry::{
        CreateMcpServiceRegistryReq, CreateMcpServiceRegistryResp, DeleteMcpServiceRegistryReq,
        GetMcpServiceRegistriesReq, GetMcpServiceRegistryReq, UpdateMcpServiceRegistryEnabledReq,
        UpdateMcpServiceRegistryReq,
    },
    service::mcp_service_registry::McpServiceRegistryService,
};

/// 控制器
pub struct McpServiceRegistryController;

impl McpServiceRegistryController {
    /// 获取 MCP 服务注册表列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetMcpServiceRegistriesReq>,
    ) -> Responder<()> {
        let service: McpServiceRegistryService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取 MCP 服务注册表详情
    pub async fn info(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetMcpServiceRegistryReq>,
    ) -> Responder<()> {
        let service: McpServiceRegistryService = provider.provide();
        let result = service.info(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建 MCP 服务注册表
    pub async fn create(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateMcpServiceRegistryReq>,
    ) -> Responder<CreateMcpServiceRegistryResp> {
        let service: McpServiceRegistryService = provider.provide();
        let result = service.create(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新 MCP 服务注册表
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateMcpServiceRegistryReq>,
    ) -> Responder<()> {
        let service: McpServiceRegistryService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新 MCP 服务注册表启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateMcpServiceRegistryEnabledReq>,
    ) -> Responder<()> {
        let service: McpServiceRegistryService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除 MCP 服务注册表
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteMcpServiceRegistryReq>,
    ) -> Responder<()> {
        let service: McpServiceRegistryService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
