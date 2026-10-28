//! 技能安装包 HTTP 控制器

use axum::Extension;
use axum::extract::{Json, Query};
use axum_context::Context;
use axum_response::{Responder, Response};
use inject::AInjectProvider;

use crate::{
    dto::skill_package::{
        CreateSkillPackageReq, CreateSkillPackageResp, DeleteSkillPackageReq, GetSkillPackageReq,
        GetSkillPackageResp, GetSkillPackagesReq, GetSkillPackagesResp,
        UpdateSkillPackageStatusReq,
    },
    service::skill_package::SkillPackageService,
};

/// 控制器
pub struct SkillPackageController;

impl SkillPackageController {
    /// 获取技能安装包列表
    pub async fn list(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetSkillPackagesReq>,
    ) -> Responder<GetSkillPackagesResp> {
        let service: SkillPackageService = provider.provide();
        let (results, total) = service.list(ctx.user_id(), req).await?;

        let resp = Response::data_list(results, total).to_json()?;
        Ok(resp)
    }

    /// 获取技能安装包详情
    pub async fn info(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Query(req): Query<GetSkillPackageReq>,
    ) -> Responder<GetSkillPackageResp> {
        let service: SkillPackageService = provider.provide();
        let result = service.info(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 创建技能安装包
    pub async fn create(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<CreateSkillPackageReq>,
    ) -> Responder<CreateSkillPackageResp> {
        let service: SkillPackageService = provider.provide();
        let result = service.create(ctx.user_id(), req).await?;

        let resp = Response::data(result).to_json()?;
        Ok(resp)
    }

    /// 更新技能安装包状态
    pub async fn update_status(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<UpdateSkillPackageStatusReq>,
    ) -> Responder<()> {
        let service: SkillPackageService = provider.provide();
        let _result = service.update_status(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }

    /// 删除技能安装包
    pub async fn delete(
        ctx: Context,
        Extension(provider): Extension<AInjectProvider>,
        Json(req): Json<DeleteSkillPackageReq>,
    ) -> Responder<()> {
        let service: SkillPackageService = provider.provide();
        let _result = service.delete(ctx.user_id(), req).await?;

        Ok(Response::ok())
    }
}
