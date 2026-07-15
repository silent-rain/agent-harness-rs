//! 技能安装包管理

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::skill_package;
use err_code::Error;

use crate::{
    dao::skill_package::SkillPackageDao,
    dto::skill_package::{
        CreateSkillPackageReq, CreateSkillPackageResp, DeleteSkillPackageReq, GetSkillPackageReq,
        GetSkillPackagesReq, UpdateSkillPackageStatusReq,
    },
    enums::SkillPackageStatus,
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct SkillPackageService {
    skill_package_dao: SkillPackageDao,
    permission: PermissionService,
}

impl SkillPackageService {
    /// 获取列表数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetSkillPackagesReq,
    ) -> anyhow::Result<(Vec<skill_package::Model>, u64)> {
        let (results, total) = self
            .skill_package_dao
            .list(user_id, req)
            .await
            .map_err(|err| {
                error!("查询技能安装包列表失败, err: {:#?}", err);
                Error::DbQueryError("查询技能安装包列表失败".to_string())
            })?;

        Ok((results, total))
    }

    /// 获取详情数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn info(
        &self,
        user_id: i64,
        req: GetSkillPackageReq,
    ) -> anyhow::Result<skill_package::Model> {
        let result = self
            .skill_package_dao
            .info(req.id)
            .await
            .map_err(|err| {
                error!("查询技能安装包信息失败, err: {:#?}", err);
                Error::DbQueryError("查询技能安装包信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("技能安装包不存在, id: {}", req.id);
                Error::DbQueryEmptyError("技能安装包不存在".to_string())
            })?;

        if result.user_id != user_id {
            error!(
                "无权操作该技能安装包, id: {}, owner: {}, current: {}",
                req.id, result.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("技能安装包不存在或无权访问".to_string()).into());
        }

        Ok(result)
    }

    /// 添加数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 创建请求
    pub async fn create(
        &self,
        user_id: i64,
        req: CreateSkillPackageReq,
    ) -> anyhow::Result<CreateSkillPackageResp> {
        // Check skill_code uniqueness under the same user
        self.check_skill_code_exist(user_id, req.skill_code.clone(), None)
            .await?;

        let model = skill_package::ActiveModel {
            user_id: Set(user_id),
            skill_code: Set(req.skill_code),
            name: Set(req.name),
            description: Set(req.description),
            version: Set(req.version),
            source_type: Set(req.source_type.into()),
            source_url: Set(req.source_url),
            content_zip: Set(vec![]),
            content_sha256: Set(String::new()),
            size_bytes: Set(0),
            installed_by: Set(user_id),
            status: Set(SkillPackageStatus::Installed.into()),
            ..Default::default()
        };

        let result = self.skill_package_dao.create(model).await.map_err(|err| {
            error!("添加技能安装包失败, err: {:#?}", err);
            Error::DbAddError("添加技能安装包失败".to_string())
        })?;

        Ok(CreateSkillPackageResp { id: result.id })
    }

    /// 更新安装包状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新状态请求
    pub async fn update_status(
        &self,
        user_id: i64,
        req: UpdateSkillPackageStatusReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership
        self.permission
            .verify_skill_package_ownership(user_id, req.id)
            .await?;

        let model = skill_package::ActiveModel {
            id: Set(req.id),
            status: Set(req.status.into()),
            ..Default::default()
        };

        let result = self.skill_package_dao.update(model).await.map_err(|err| {
            error!("更新技能安装包状态失败, err: {:#?}", err);
            Error::DbUpdateError("更新技能安装包状态失败".to_string())
        })?;

        Ok(result)
    }

    /// 删除数据（物理删除）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 删除请求
    pub async fn delete(&self, user_id: i64, req: DeleteSkillPackageReq) -> anyhow::Result<u64> {
        // Verify ownership
        self.permission
            .verify_skill_package_ownership(user_id, req.id)
            .await?;

        let result = self.skill_package_dao.delete(req.id).await.map_err(|err| {
            error!("删除技能安装包失败, err: {:#?}", err);
            Error::DbDeleteError("删除技能安装包失败".to_string())
        })?;

        Ok(result)
    }

    /// 检查技能编码是否已存在（用户维度唯一性）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `skill_code` - 技能编码
    /// * `current_id` - 当前记录 ID（更新时排除自身）
    async fn check_skill_code_exist(
        &self,
        user_id: i64,
        skill_code: String,
        current_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let result = self
            .skill_package_dao
            .info_by_code(user_id, &skill_code)
            .await
            .map_err(|err| {
                error!("查询技能编码失败, err: {:#?}", err);
                Error::DbQueryError("查询技能编码失败".to_string())
            })?;

        if let Some(model) = result
            && (current_id.is_none() || Some(model.id) != current_id)
        {
            error!("技能编码已存在, skill_code: {}", skill_code);
            return Err(Error::DbDataExistError("技能编码已存在".to_string()).into());
        }

        Ok(())
    }
}
