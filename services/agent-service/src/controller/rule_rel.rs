//! 智能体规则绑定关系 HTTP 控制器

use axum::Extension;
use axum_context::Context;
use axum_response::{Responder, Response};
use axum_validator::{Json, Query};
use inject::AInjectProvider;

use crate::{
    dto::rule_rel::{
        BatchSaveRuleRelsReq, BatchSaveRuleRelsResp, GetRuleRelsReq, GetRuleRelsResp,
        UpdateRuleRelEnabledReq, UpdateRuleRelReq,
    },
    service::rule_rel::RuleRelService,
};

/// 控制器
pub struct RuleRelController;

impl RuleRelController {
    /// 获取智能体的规则绑定列表（含规则详情）
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetRuleRelsReq>,
    ) -> Responder<GetRuleRelsResp> {
        let service: RuleRelService = provider.provide();
        let (results, total) = service.list(ctx.get_user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 批量同步智能体规则绑定（差异处理）
    pub async fn batch_save(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<BatchSaveRuleRelsReq>,
    ) -> Responder<BatchSaveRuleRelsResp> {
        let service: RuleRelService = provider.provide();
        let result = service.batch_save(ctx.get_user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新规则绑定
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateRuleRelReq>,
    ) -> Responder<()> {
        let service: RuleRelService = provider.provide();
        let _result = service.update(ctx.get_user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新规则绑定启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateRuleRelEnabledReq>,
    ) -> Responder<()> {
        let service: RuleRelService = provider.provide();
        let _result = service.update_enabled(ctx.get_user_id(), req).await?;

        Ok(Response::ok())
    }
}
