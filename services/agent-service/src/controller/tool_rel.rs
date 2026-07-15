//! 智能体工具绑定关系 HTTP 控制器

use axum::Extension;
use axum_context::Context;
use axum_response::{Responder, Response};
use axum_validator::{Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::tool_rel::{
        BatchSaveToolRelsReq, BatchSaveToolRelsResp, GetToolRelsReq, GetToolRelsResp,
        UpdateToolRelEnabledReq, UpdateToolRelReq,
    },
    service::tool_rel::ToolRelService,
};

/// 控制器
pub struct ToolRelController;

impl ToolRelController {
    /// 获取智能体的工具绑定列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetToolRelsReq>,
    ) -> Responder<GetToolRelsResp> {
        let service: ToolRelService = provider.provide();
        let (results, total) = service.list(ctx.get_user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 批量同步智能体工具绑定（差异处理）
    pub async fn batch_save(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<BatchSaveToolRelsReq>,
    ) -> Responder<BatchSaveToolRelsResp> {
        let service: ToolRelService = provider.provide();
        let result = service.batch_save(ctx.get_user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新工具绑定
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateToolRelReq>,
    ) -> Responder<()> {
        let service: ToolRelService = provider.provide();
        let _result = service.update(ctx.get_user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新工具绑定启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateToolRelEnabledReq>,
    ) -> Responder<()> {
        let service: ToolRelService = provider.provide();
        let _result = service.update_enabled(ctx.get_user_id(), req).await?;

        Ok(Response::ok())
    }
}
