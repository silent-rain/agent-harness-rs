//! 智能体工具绑定关系管理

use std::collections::HashSet;

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::tool_rel;
use err_code::Error;

use crate::{
    dao::tool_rel::ToolRelDao,
    dto::tool_rel::{
        BatchSaveToolRelsReq, BatchSaveToolRelsResp, GetToolRelsReq, UpdateToolRelEnabledReq,
        UpdateToolRelReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct ToolRelService {
    tool_rel_dao: ToolRelDao,
    permission_service: PermissionService,
}

impl ToolRelService {
    /// 获取指定智能体的工具绑定列表
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetToolRelsReq,
    ) -> anyhow::Result<(Vec<tool_rel::Model>, u64)> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let (items, total) = self.tool_rel_dao.list(&req).await.map_err(|err| {
            error!("查询工具绑定列表失败, err: {:#?}", err);
            Error::DbQueryError("查询工具绑定列表失败".to_string())
        })?;

        Ok((items, total))
    }

    /// 批量同步智能体工具绑定（差异处理）
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
        req: BatchSaveToolRelsReq,
    ) -> anyhow::Result<BatchSaveToolRelsResp> {
        if req.tool_ids.is_empty() {
            return Ok(BatchSaveToolRelsResp {
                added: 0,
                removed: 0,
            });
        }

        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        // Get current bindings
        let current_tool_ids = self
            .tool_rel_dao
            .list_tool_ids_by_agent(req.agent_id)
            .await
            .map_err(|err| {
                error!("查询当前工具绑定列表失败, err: {:#?}", err);
                Error::DbQueryError("查询当前工具绑定列表失败".to_string())
            })?;

        let current_set: HashSet<i64> = current_tool_ids.into_iter().collect();
        let target_set: HashSet<i64> = req.tool_ids.into_iter().collect();

        // Calculate diff
        let to_add: Vec<i64> = target_set.difference(&current_set).copied().collect();
        let to_remove: Vec<i64> = current_set.difference(&target_set).copied().collect();

        let added = to_add.len();
        let removed = to_remove.len();

        // Build ActiveModels for new bindings
        let models: Vec<tool_rel::ActiveModel> = to_add
            .into_iter()
            .map(|tool_id| tool_rel::ActiveModel {
                agent_id: Set(req.agent_id),
                tool_id: Set(tool_id),
                priority: Set(0),
                enabled: Set(true),
                ..Default::default()
            })
            .collect();

        // Execute batch insert and delete in a single transaction
        self.tool_rel_dao
            .batch_save(req.agent_id, models, &to_remove)
            .await
            .map_err(|err| {
                error!("批量同步工具绑定失败, err: {:#?}", err);
                Error::DbUpdateError("批量同步工具绑定失败".to_string())
            })?;

        Ok(BatchSaveToolRelsResp { added, removed })
    }

    /// 更新绑定数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateToolRelReq) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_tool_rel_ownership(user_id, req.id)
            .await?;

        let model = tool_rel::ActiveModel {
            id: Set(req.id),
            priority: Set(req.priority),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.tool_rel_dao.update(model).await.map_err(|err| {
            error!("更新工具绑定失败, err: {:#?}", err);
            Error::DbUpdateError("更新工具绑定失败".to_string())
        })?;

        Ok(result)
    }

    /// 更新启用状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新启用状态请求
    pub async fn update_enabled(
        &self,
        user_id: i64,
        req: UpdateToolRelEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_tool_rel_ownership(user_id, req.id)
            .await?;

        let model = tool_rel::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.tool_rel_dao.update(model).await.map_err(|err| {
            error!("更新工具绑定启用状态失败, err: {:#?}", err);
            Error::DbUpdateError("更新工具绑定启用状态失败".to_string())
        })?;

        Ok(result)
    }
}
