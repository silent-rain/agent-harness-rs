//! 智能体技能绑定关系管理

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::skill_rel;
use err_code::Error;

use crate::{
    dao::skill_rel::SkillRelDao,
    dto::skill_rel::{
        CreateSkillRelReq, CreateSkillRelResp, DeleteSkillRelReq, GetSkillRelReq, GetSkillRelsReq,
        UpdateSkillRelEnabledReq, UpdateSkillRelReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct SkillRelService {
    skill_rel_dao: SkillRelDao,
    permission: PermissionService,
}

impl SkillRelService {
    /// 获取指定智能体的技能绑定列表
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetSkillRelsReq,
    ) -> anyhow::Result<(Vec<skill_rel::Model>, u64)> {
        // Verify agent ownership
        self.permission
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let (items, total) = self.skill_rel_dao.list(user_id, req).await.map_err(|err| {
            error!("查询技能绑定列表失败, err: {:#?}", err);
            Error::DbQueryError("查询技能绑定列表失败".to_string())
        })?;

        Ok((items, total))
    }

    /// 获取技能绑定详情
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 详情查询请求
    pub async fn info(
        &self,
        user_id: i64,
        req: GetSkillRelReq,
    ) -> anyhow::Result<skill_rel::Model> {
        let result = self
            .skill_rel_dao
            .info(req.id)
            .await
            .map_err(|err| {
                error!("查询技能绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询技能绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("技能绑定不存在, id: {}", req.id);
                Error::DbQueryEmptyError("技能绑定不存在".to_string())
            })?;

        // Verify ownership in Service layer
        if result.user_id != user_id {
            error!(
                "无权操作该技能绑定, id: {}, owner: {}, current: {}",
                req.id, result.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("技能绑定不存在或无权访问".to_string()).into());
        }

        Ok(result)
    }

    /// 创建技能绑定
    ///
    /// 通过 `skill_package_id` 关联 `t_agent_skill_package`，
    /// 元数据（name/description/skill_code）从包表获取，无需重复传入。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 创建请求
    pub async fn create(
        &self,
        user_id: i64,
        req: CreateSkillRelReq,
    ) -> anyhow::Result<CreateSkillRelResp> {
        // Verify agent ownership
        self.permission
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        // Check skill_package_id uniqueness within the same agent
        self.check_package_exist(req.agent_id, req.skill_package_id, None)
            .await?;

        let model = skill_rel::ActiveModel {
            user_id: Set(user_id),
            agent_id: Set(req.agent_id),
            skill_package_id: Set(req.skill_package_id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.skill_rel_dao.create(model).await.map_err(|err| {
            error!("创建技能绑定失败, err: {:#?}", err);
            Error::DbAddError("创建技能绑定失败".to_string())
        })?;

        Ok(CreateSkillRelResp { id: result.id })
    }

    /// 更新技能绑定启用状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateSkillRelReq) -> anyhow::Result<u64> {
        // Verify ownership via PermissionService
        self.permission
            .verify_skill_rel_ownership(user_id, req.id)
            .await?;

        let model = skill_rel::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.skill_rel_dao.update(model).await.map_err(|err| {
            error!("更新技能绑定失败, err: {:#?}", err);
            Error::DbUpdateError("更新技能绑定失败".to_string())
        })?;

        Ok(result)
    }

    /// 更新技能绑定启用状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新启用状态请求
    pub async fn update_enabled(
        &self,
        user_id: i64,
        req: UpdateSkillRelEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership via PermissionService
        self.permission
            .verify_skill_rel_ownership(user_id, req.id)
            .await?;

        let model = skill_rel::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.skill_rel_dao.update(model).await.map_err(|err| {
            error!("更新技能绑定启用状态失败, err: {:#?}", err);
            Error::DbUpdateError("更新技能绑定启用状态失败".to_string())
        })?;

        Ok(result)
    }

    /// 删除技能绑定
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 删除请求
    pub async fn delete(&self, user_id: i64, req: DeleteSkillRelReq) -> anyhow::Result<u64> {
        // Verify ownership via PermissionService
        self.permission
            .verify_skill_rel_ownership(user_id, req.id)
            .await?;

        let result = self.skill_rel_dao.delete(req.id).await.map_err(|err| {
            error!("删除技能绑定失败, err: {:#?}", err);
            Error::DbDeleteError("删除技能绑定失败".to_string())
        })?;

        Ok(result)
    }

    /// 检查技能包在同一智能体下是否已绑定
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `skill_package_id` - 技能安装包 ID
    /// * `current_id` - 当前记录 ID（更新时排除自身）
    async fn check_package_exist(
        &self,
        agent_id: i64,
        skill_package_id: i64,
        current_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let result = self
            .skill_rel_dao
            .info_by_agent_and_package(agent_id, skill_package_id)
            .await
            .map_err(|err| {
                error!("查询技能绑定失败, err: {:#?}", err);
                Error::DbQueryError("查询技能绑定失败".to_string())
            })?;

        if let Some(model) = result
            && (current_id.is_none() || Some(model.id) != current_id)
        {
            error!("技能包已绑定, skill_package_id: {}", skill_package_id);
            return Err(Error::DbDataExistError("该智能体下技能包已绑定".to_string()).into());
        }

        Ok(())
    }
}
