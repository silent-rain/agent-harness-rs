//! 工具注册表 HTTP 控制器

use axum_response::{Responder, Response};
use axum_validator::{Extension, Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::tool_registry::{
        CreateToolRegistryReq, CreateToolRegistryResp, DeleteToolRegistryReq, GetToolRegistriesReq,
        GetToolRegistriesResp, GetToolRegistryReq, GetToolRegistryResp,
        UpdateToolRegistryEnabledReq, UpdateToolRegistryReq,
    },
    service::tool_registry::ToolRegistryService,
};

/// 控制器
pub struct ToolRegistryController;

impl ToolRegistryController {
    /// 获取工具列表
    pub async fn list(
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetToolRegistriesReq>,
    ) -> Responder<GetToolRegistriesResp> {
        let service: ToolRegistryService = provider.provide();
        let (results, total) = service.list(req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取工具详情
    pub async fn info(
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetToolRegistryReq>,
    ) -> Responder<GetToolRegistryResp> {
        let service: ToolRegistryService = provider.provide();
        let result = service.info(req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建工具
    pub async fn create(
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateToolRegistryReq>,
    ) -> Responder<CreateToolRegistryResp> {
        let service: ToolRegistryService = provider.provide();
        let result = service.create(req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新工具
    pub async fn update(
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateToolRegistryReq>,
    ) -> Responder<()> {
        let service: ToolRegistryService = provider.provide();
        let _result = service.update(req).await?;

        Ok(Response::ok())
    }

    /// 更新工具启用状态
    pub async fn update_enabled(
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateToolRegistryEnabledReq>,
    ) -> Responder<()> {
        let service: ToolRegistryService = provider.provide();
        let _result = service.update_enabled(req).await?;

        Ok(Response::ok())
    }

    /// 删除工具
    pub async fn delete(
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteToolRegistryReq>,
    ) -> Responder<()> {
        let service: ToolRegistryService = provider.provide();
        let _result = service.delete(req).await?;

        Ok(Response::ok())
    }
}
