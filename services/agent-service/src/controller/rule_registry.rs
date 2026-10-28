//! 规则注册表 HTTP 控制器

use axum::Extension;
use axum::extract::{Json, Query};
use axum_context::Context;
use axum_response::{Responder, Response};
use inject::AInjectProvider;

use crate::{
    dto::rule_registry::{
        CreateRuleRegistryReq, CreateRuleRegistryResp, DeleteRuleRegistryReq, GetRuleRegistriesReq,
        GetRuleRegistriesResp, GetRuleRegistryReq, GetRuleRegistryResp,
        UpdateRuleRegistryEnabledReq, UpdateRuleRegistryReq,
    },
    service::rule_registry::RuleRegistryService,
};

/// 控制器
pub struct RuleRegistryController;

impl RuleRegistryController {
    /// 获取规则列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetRuleRegistriesReq>,
    ) -> Responder<GetRuleRegistriesResp> {
        let service: RuleRegistryService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取规则详情
    pub async fn info(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetRuleRegistryReq>,
    ) -> Responder<GetRuleRegistryResp> {
        let service: RuleRegistryService = provider.provide();
        let result = service.info(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建规则
    pub async fn create(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateRuleRegistryReq>,
    ) -> Responder<CreateRuleRegistryResp> {
        let service: RuleRegistryService = provider.provide();
        let result = service.create(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新规则
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateRuleRegistryReq>,
    ) -> Responder<()> {
        let service: RuleRegistryService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新规则启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateRuleRegistryEnabledReq>,
    ) -> Responder<()> {
        let service: RuleRegistryService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除规则
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteRuleRegistryReq>,
    ) -> Responder<()> {
        let service: RuleRegistryService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
