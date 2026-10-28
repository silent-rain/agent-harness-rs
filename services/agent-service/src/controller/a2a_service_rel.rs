//! 智能体 A2A 服务绑定关系 HTTP 控制器

use axum::Extension;
use axum::extract::{Json, Query};
use axum_context::Context;
use axum_response::{Responder, Response};
use inject::AInjectProvider;

use crate::{
    dto::a2a_service_rel::{
        BatchSaveA2aServiceRelsReq, BatchSaveA2aServiceRelsResp, GetA2aServiceRelsReq,
        UpdateA2aServiceRelEnabledReq,
    },
    service::a2a_service_rel::A2aServiceRelService,
};

/// 控制器
pub struct A2aServiceRelController;

impl A2aServiceRelController {
    /// 获取智能体的 A2A 服务绑定列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetA2aServiceRelsReq>,
    ) -> Responder<()> {
        let service: A2aServiceRelService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 批量同步 A2A 服务绑定
    pub async fn batch_save(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<BatchSaveA2aServiceRelsReq>,
    ) -> Responder<BatchSaveA2aServiceRelsResp> {
        let service: A2aServiceRelService = provider.provide();
        let result = service.batch_save(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新 A2A 服务绑定启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateA2aServiceRelEnabledReq>,
    ) -> Responder<()> {
        let service: A2aServiceRelService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
