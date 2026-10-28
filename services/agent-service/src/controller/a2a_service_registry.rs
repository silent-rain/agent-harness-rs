//! A2A 服务注册表 HTTP 控制器

use axum::Extension;
use axum::extract::{Json, Query};
use axum_context::Context;
use axum_response::{Responder, Response};
use inject::AInjectProvider;

use crate::{
    dto::a2a_service_registry::{
        CreateA2aServiceRegistryReq, CreateA2aServiceRegistryResp, DeleteA2aServiceRegistryReq,
        GetA2aServiceRegistriesReq, GetA2aServiceRegistryReq, UpdateA2aServiceRegistryEnabledReq,
        UpdateA2aServiceRegistryReq,
    },
    service::a2a_service_registry::A2aServiceRegistryService,
};

/// 控制器
pub struct A2aServiceRegistryController;

impl A2aServiceRegistryController {
    /// 获取 A2A 服务注册表列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetA2aServiceRegistriesReq>,
    ) -> Responder<()> {
        let service: A2aServiceRegistryService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取 A2A 服务注册表详情
    pub async fn info(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetA2aServiceRegistryReq>,
    ) -> Responder<()> {
        let service: A2aServiceRegistryService = provider.provide();
        let result = service.info(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建 A2A 服务注册表
    pub async fn create(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateA2aServiceRegistryReq>,
    ) -> Responder<CreateA2aServiceRegistryResp> {
        let service: A2aServiceRegistryService = provider.provide();
        let result = service.create(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新 A2A 服务注册表
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateA2aServiceRegistryReq>,
    ) -> Responder<()> {
        let service: A2aServiceRegistryService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新 A2A 服务注册表启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateA2aServiceRegistryEnabledReq>,
    ) -> Responder<()> {
        let service: A2aServiceRegistryService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除 A2A 服务注册表
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteA2aServiceRegistryReq>,
    ) -> Responder<()> {
        let service: A2aServiceRegistryService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
