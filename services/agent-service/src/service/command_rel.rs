//! 智能体命令绑定关系管理

use std::collections::HashSet;

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::{command_rel, command_rel::CommandRelWithCommand};
use err_code::Error;

use crate::{
    dao::command_rel::CommandRelDao,
    dto::command_rel::{
        BatchSaveCommandRelsReq, BatchSaveCommandRelsResp, GetCommandRelsReq,
        UpdateCommandRelEnabledReq, UpdateCommandRelReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct CommandRelService {
    command_rel_dao: CommandRelDao,
    permission_service: PermissionService,
}

impl CommandRelService {
    /// 获取指定智能体的命令绑定列表（含命令详情）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetCommandRelsReq,
    ) -> anyhow::Result<(Vec<CommandRelWithCommand>, u64)> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let (items, total) = self.command_rel_dao.list(&req).await.map_err(|err| {
            error!("查询命令绑定列表失败, err: {:#?}", err);
            Error::DbQueryError("查询命令绑定列表失败".to_string())
        })?;

        Ok((items, total))
    }

    /// 批量同步智能体命令绑定（差异处理）
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
        req: BatchSaveCommandRelsReq,
    ) -> anyhow::Result<BatchSaveCommandRelsResp> {
        if req.command_ids.is_empty() {
            return Ok(BatchSaveCommandRelsResp {
                added: 0,
                removed: 0,
            });
        }

        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        // Verify all command_ids belong to current user
        self.permission_service
            .verify_commands_ownership(user_id, &req.command_ids)
            .await?;

        // Get current bindings
        let current_command_ids = self
            .command_rel_dao
            .list_command_ids_by_agent(req.agent_id)
            .await
            .map_err(|err| {
                error!("查询当前绑定列表失败, err: {:#?}", err);
                Error::DbQueryError("查询当前绑定列表失败".to_string())
            })?;

        let current_set: HashSet<i64> = current_command_ids.into_iter().collect();
        let target_set: HashSet<i64> = req.command_ids.into_iter().collect();

        // Calculate diff
        let to_add: Vec<i64> = target_set.difference(&current_set).copied().collect();
        let to_remove: Vec<i64> = current_set.difference(&target_set).copied().collect();

        let added = to_add.len();
        let removed = to_remove.len();

        // Build ActiveModels for new bindings
        let models: Vec<command_rel::ActiveModel> = to_add
            .into_iter()
            .map(|command_id| command_rel::ActiveModel {
                agent_id: Set(req.agent_id),
                command_id: Set(command_id),
                priority: Set(0),
                enabled: Set(true),
                ..Default::default()
            })
            .collect();

        // Execute batch insert and delete in a single transaction
        self.command_rel_dao
            .batch_save(req.agent_id, models, &to_remove)
            .await
            .map_err(|err| {
                error!("批量同步命令绑定失败, err: {:#?}", err);
                Error::DbUpdateError("批量同步命令绑定失败".to_string())
            })?;

        Ok(BatchSaveCommandRelsResp { added, removed })
    }

    /// 更新绑定数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateCommandRelReq) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_command_rel_ownership(user_id, req.id)
            .await?;

        let model = command_rel::ActiveModel {
            id: Set(req.id),
            priority: Set(req.priority),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.command_rel_dao.update(model).await.map_err(|err| {
            error!("更新命令绑定失败, err: {:#?}", err);
            Error::DbUpdateError("更新命令绑定失败".to_string())
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
        req: UpdateCommandRelEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_command_rel_ownership(user_id, req.id)
            .await?;

        let model = command_rel::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.command_rel_dao.update(model).await.map_err(|err| {
            error!("更新命令绑定启用状态失败, err: {:#?}", err);
            Error::DbUpdateError("更新命令绑定启用状态失败".to_string())
        })?;

        Ok(result)
    }
}
