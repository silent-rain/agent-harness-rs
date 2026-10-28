//! 命令注册表 HTTP 控制器

use axum::Extension;
use axum_context::Context;
use axum_response::{Responder, Response};
use axum_validator::{Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::command_registry::{
        CreateCommandRegistryReq, CreateCommandRegistryResp, DeleteCommandRegistryReq,
        GetCommandRegistriesReq, GetCommandRegistriesResp, GetCommandRegistryReq,
        GetCommandRegistryResp, UpdateCommandRegistryEnabledReq, UpdateCommandRegistryReq,
    },
    service::command_registry::CommandRegistryService,
};

/// 控制器
pub struct CommandRegistryController;

impl CommandRegistryController {
    /// 获取命令列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetCommandRegistriesReq>,
    ) -> Responder<GetCommandRegistriesResp> {
        let service: CommandRegistryService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取命令详情
    pub async fn info(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetCommandRegistryReq>,
    ) -> Responder<GetCommandRegistryResp> {
        let service: CommandRegistryService = provider.provide();
        let result = service.info(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建命令
    pub async fn create(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateCommandRegistryReq>,
    ) -> Responder<CreateCommandRegistryResp> {
        let service: CommandRegistryService = provider.provide();
        let result = service.create(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新命令
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateCommandRegistryReq>,
    ) -> Responder<()> {
        let service: CommandRegistryService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新命令启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateCommandRegistryEnabledReq>,
    ) -> Responder<()> {
        let service: CommandRegistryService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除命令
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteCommandRegistryReq>,
    ) -> Responder<()> {
        let service: CommandRegistryService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
