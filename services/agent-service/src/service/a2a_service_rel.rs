//! 智能体 A2A 服务绑定关系管理

use std::collections::HashSet;

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::{a2a_service_rel, a2a_service_rel::A2aServiceRelWithRegistry};
use err_code::Error;

use crate::{
    dao::a2a_service_rel::A2aServiceRelDao,
    dto::a2a_service_rel::{
        BatchSaveA2aServiceRelsReq, BatchSaveA2aServiceRelsResp, GetA2aServiceRelsReq,
        UpdateA2aServiceRelEnabledReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct A2aServiceRelService {
    a2a_service_rel_dao: A2aServiceRelDao,
    permission_service: PermissionService,
}

impl A2aServiceRelService {
    /// 获取指定智能体的 A2A 服务绑定列表（含注册表详情）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetA2aServiceRelsReq,
    ) -> anyhow::Result<(Vec<A2aServiceRelWithRegistry>, u64)> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let (items, total) = self.a2a_service_rel_dao.list(&req).await.map_err(|err| {
            error!("查询 A2A 服务绑定列表失败, err: {:#?}", err);
            Error::DbQueryError("查询 A2A 服务绑定列表失败".to_string())
        })?;

        Ok((items, total))
    }

    /// 批量同步智能体 A2A 服务绑定（差异处理）
    ///
    /// 自动计算当前绑定与目标绑定的差异，执行新增和删除操作。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 批量同步请求
    pub async fn batch_save(
        &self,
        user_id: i64,
        req: BatchSaveA2aServiceRelsReq,
    ) -> anyhow::Result<BatchSaveA2aServiceRelsResp> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        // Get current bindings
        let current_registry_ids = self
            .a2a_service_rel_dao
            .list_registry_ids_by_agent(req.agent_id)
            .await
            .map_err(|err| {
                error!("查询当前 A2A 服务绑定列表失败, err: {:#?}", err);
                Error::DbQueryError("查询当前 A2A 服务绑定列表失败".to_string())
            })?;

        let current_set: HashSet<i64> = current_registry_ids.into_iter().collect();
        let target_set: HashSet<i64> = req.registry_ids.into_iter().collect();

        // Calculate diff
        let to_add: Vec<i64> = target_set.difference(&current_set).copied().collect();
        let to_remove: Vec<i64> = current_set.difference(&target_set).copied().collect();

        let added = to_add.len();
        let removed = to_remove.len();

        // Build ActiveModels for new bindings
        let models: Vec<a2a_service_rel::ActiveModel> = to_add
            .into_iter()
            .map(|registry_id| a2a_service_rel::ActiveModel {
                agent_id: Set(req.agent_id),
                registry_id: Set(registry_id),
                enabled: Set(true),
                ..Default::default()
            })
            .collect();

        // Execute batch insert and delete in a single transaction
        self.a2a_service_rel_dao
            .batch_save(req.agent_id, models, &to_remove)
            .await
            .map_err(|err| {
                error!("批量同步 A2A 服务绑定失败, err: {:#?}", err);
                Error::DbUpdateError("批量同步 A2A 服务绑定失败".to_string())
            })?;

        Ok(BatchSaveA2aServiceRelsResp { added, removed })
    }

    /// 更新 A2A 服务绑定启用状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新启用状态请求
    pub async fn update_enabled(
        &self,
        user_id: i64,
        req: UpdateA2aServiceRelEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_a2a_service_rel_ownership(user_id, req.id)
            .await?;

        let model = a2a_service_rel::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self
            .a2a_service_rel_dao
            .update(model)
            .await
            .map_err(|err| {
                error!("更新 A2A 服务绑定启用状态失败, err: {:#?}", err);
                Error::DbUpdateError("更新 A2A 服务绑定启用状态失败".to_string())
            })?;

        Ok(result)
    }
}
