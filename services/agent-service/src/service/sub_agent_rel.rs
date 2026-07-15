//! 智能体子智能体绑定关系管理

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::sub_agent_rel;
use err_code::Error;

use crate::{
    dao::sub_agent_rel::SubAgentRelDao,
    dto::sub_agent_rel::{
        AddSubAgentRelReq, DeleteSubAgentRelReq, GetSubAgentRelsReq, UpdateSubAgentRelEnabledReq,
        UpdateSubAgentRelReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct SubAgentRelService {
    sub_agent_rel_dao: SubAgentRelDao,
    permission_service: PermissionService,
}

impl SubAgentRelService {
    /// 获取指定智能体的子智能体绑定列表（关联子智能体详情）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetSubAgentRelsReq,
    ) -> anyhow::Result<(Vec<sub_agent_rel::SubAgentRelWithAgent>, u64)> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let (items, total) = self.sub_agent_rel_dao.list(&req).await.map_err(|err| {
            error!("查询子智能体绑定列表失败, err: {:#?}", err);
            Error::DbQueryError("查询子智能体绑定列表失败".to_string())
        })?;

        Ok((items, total))
    }

    /// 新增子智能体绑定
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 新增请求
    pub async fn add(
        &self,
        user_id: i64,
        req: AddSubAgentRelReq,
    ) -> anyhow::Result<sub_agent_rel::Model> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let model = sub_agent_rel::ActiveModel {
            agent_id: Set(req.agent_id),
            sub_agent_id: Set(req.sub_agent_id),
            alias: Set(req.alias),
            share_context: Set(req.share_context),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.sub_agent_rel_dao.add(model).await.map_err(|err| {
            error!("新增子智能体绑定失败, err: {:#?}", err);
            Error::DbAddError("新增子智能体绑定失败".to_string())
        })?;

        Ok(result)
    }

    /// 更新子智能体绑定
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateSubAgentRelReq) -> anyhow::Result<u64> {
        // Verify ownership via agent_id on the binding record
        self.permission_service
            .verify_sub_agent_rel_ownership(user_id, req.id)
            .await?;

        let model = sub_agent_rel::ActiveModel {
            id: Set(req.id),
            alias: Set(req.alias),
            share_context: Set(req.share_context),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.sub_agent_rel_dao.update(model).await.map_err(|err| {
            error!("更新子智能体绑定失败, err: {:#?}", err);
            Error::DbUpdateError("更新子智能体绑定失败".to_string())
        })?;

        Ok(result)
    }

    /// 更新子智能体绑定启用状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新启用状态请求
    pub async fn update_enabled(
        &self,
        user_id: i64,
        req: UpdateSubAgentRelEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership via agent_id on the binding record
        self.permission_service
            .verify_sub_agent_rel_ownership(user_id, req.id)
            .await?;

        let model = sub_agent_rel::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.sub_agent_rel_dao.update(model).await.map_err(|err| {
            error!("更新子智能体绑定启用状态失败, err: {:#?}", err);
            Error::DbUpdateError("更新子智能体绑定启用状态失败".to_string())
        })?;

        Ok(result)
    }

    /// 删除子智能体绑定
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 删除请求
    pub async fn delete(&self, user_id: i64, req: DeleteSubAgentRelReq) -> anyhow::Result<u64> {
        // Verify ownership via agent_id on the binding record
        self.permission_service
            .verify_sub_agent_rel_ownership(user_id, req.id)
            .await?;

        let result = self.sub_agent_rel_dao.delete(req.id).await.map_err(|err| {
            error!("删除子智能体绑定失败, err: {:#?}", err);
            Error::DbDeleteError("删除子智能体绑定失败".to_string())
        })?;

        Ok(result)
    }
}
