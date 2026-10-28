//! 智能体技能绑定关系 HTTP 控制器

use axum::Extension;
use axum::extract::{Json, Query};
use axum_context::Context;
use axum_response::{Responder, Response};
use inject::AInjectProvider;

use crate::{
    dto::skill_rel::{
        CreateSkillRelReq, CreateSkillRelResp, DeleteSkillRelReq, GetSkillRelReq, GetSkillRelResp,
        GetSkillRelsReq, GetSkillRelsResp, UpdateSkillRelEnabledReq, UpdateSkillRelReq,
    },
    service::skill_rel::SkillRelService,
};

/// 控制器
pub struct SkillRelController;

impl SkillRelController {
    /// 获取智能体的技能绑定列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetSkillRelsReq>,
    ) -> Responder<GetSkillRelsResp> {
        let service: SkillRelService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取技能绑定详情
    pub async fn info(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetSkillRelReq>,
    ) -> Responder<GetSkillRelResp> {
        let service: SkillRelService = provider.provide();
        let result = service.info(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建技能绑定
    pub async fn create(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateSkillRelReq>,
    ) -> Responder<CreateSkillRelResp> {
        let service: SkillRelService = provider.provide();
        let result = service.create(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新技能绑定
    pub async fn update(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateSkillRelReq>,
    ) -> Responder<()> {
        let service: SkillRelService = provider.provide();
        let _result = service.update(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 更新技能绑定启用状态
    pub async fn update_enabled(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateSkillRelEnabledReq>,
    ) -> Responder<()> {
        let service: SkillRelService = provider.provide();
        let _result = service.update_enabled(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除技能绑定
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteSkillRelReq>,
    ) -> Responder<()> {
        let service: SkillRelService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
