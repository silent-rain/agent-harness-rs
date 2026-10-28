//! 智能体命令绑定关系 HTTP 控制器

use axum::Extension;
use axum::extract::{Json, Query};
use axum_context::Context;
use axum_response::{Responder, Response};
use inject::AInjectProvider;

use crate::{
    dto::command_rel::{
        BatchSaveCommandRelsReq, BatchSaveCommandRelsResp, GetCommandRelsReq, GetCommandRelsResp,
        UpdateCommandRelEnabledReq, UpdateCommandRelReq,
    },
    service::command_rel::CommandRelService,
};

/// 控制器
pub struct CommandRelController;

impl CommandRelController {
    /// 获取智能体的命令绑定列表（含命令详情）
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetCommandRelsReq>,
    ) -> Responder<GetCommandRelsResp> {
        let service: CommandRelService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 批量同步智能体命令绑定（差异处理）
    pub async fn batch_save(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<BatchSaveCommandRelsReq>,
    ) -> Responder<BatchSaveCommandRelsResp> {
        let service: CommandRelService = provider.provide();
        let result = service.batch_save(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新命令绑定
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateCommandRelReq>,
    ) -> Responder<()> {
        let service: CommandRelService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新命令绑定启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateCommandRelEnabledReq>,
    ) -> Responder<()> {
        let service: CommandRelService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
