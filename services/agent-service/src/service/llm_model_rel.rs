//! 智能体 LLM 模型绑定关系管理

use std::collections::HashSet;

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::llm_model_rel;
use err_code::Error;

use crate::{
    dao::llm_model_rel::LlmModelRelDao,
    dto::llm_model_rel::{
        BatchSaveLlmModelRelsReq, BatchSaveLlmModelRelsResp, GetLlmModelRelsReq,
        UpdateLlmModelRelEnabledReq, UpdateLlmModelRelReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct LlmModelRelService {
    llm_model_rel_dao: LlmModelRelDao,
    permission_service: PermissionService,
}

impl LlmModelRelService {
    /// 获取指定智能体的 LLM 模型绑定列表
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetLlmModelRelsReq,
    ) -> anyhow::Result<(Vec<llm_model_rel::Model>, u64)> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let (items, total) = self.llm_model_rel_dao.list(&req).await.map_err(|err| {
            error!("查询 LLM 模型绑定列表失败, err: {:#?}", err);
            Error::DbQueryError("查询 LLM 模型绑定列表失败".to_string())
        })?;

        Ok((items, total))
    }

    /// 批量同步智能体 LLM 模型绑定（差异处理）
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
        req: BatchSaveLlmModelRelsReq,
    ) -> anyhow::Result<BatchSaveLlmModelRelsResp> {
        if req.model_ids.is_empty() {
            return Ok(BatchSaveLlmModelRelsResp {
                added: 0,
                removed: 0,
            });
        }

        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        // Get current bindings
        let current_model_ids = self
            .llm_model_rel_dao
            .list_model_ids_by_agent(req.agent_id)
            .await
            .map_err(|err| {
                error!("查询当前 LLM 模型绑定列表失败, err: {:#?}", err);
                Error::DbQueryError("查询当前 LLM 模型绑定列表失败".to_string())
            })?;

        let current_set: HashSet<i64> = current_model_ids.into_iter().collect();
        let target_set: HashSet<i64> = req.model_ids.into_iter().collect();

        // Calculate diff
        let to_add: Vec<i64> = target_set.difference(&current_set).copied().collect();
        let to_remove: Vec<i64> = current_set.difference(&target_set).copied().collect();

        let added = to_add.len();
        let removed = to_remove.len();

        // Build ActiveModels for new bindings
        let models: Vec<llm_model_rel::ActiveModel> = to_add
            .into_iter()
            .map(|model_id| llm_model_rel::ActiveModel {
                agent_id: Set(req.agent_id),
                model_id: Set(model_id),
                enabled: Set(true),
                ..Default::default()
            })
            .collect();

        // Execute batch insert and delete in a single transaction
        self.llm_model_rel_dao
            .batch_save(req.agent_id, models, &to_remove)
            .await
            .map_err(|err| {
                error!("批量同步 LLM 模型绑定失败, err: {:#?}", err);
                Error::DbUpdateError("批量同步 LLM 模型绑定失败".to_string())
            })?;

        Ok(BatchSaveLlmModelRelsResp { added, removed })
    }

    /// 更新 LLM 模型绑定数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateLlmModelRelReq) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_llm_model_rel_ownership(user_id, req.id)
            .await?;

        let model = llm_model_rel::ActiveModel {
            id: Set(req.id),
            temperature: Set(req.temperature),
            max_tokens: Set(req.max_tokens.map(|v| v.min(i32::MAX as u64) as i32)),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.llm_model_rel_dao.update(model).await.map_err(|err| {
            error!("更新 LLM 模型绑定失败, err: {:#?}", err);
            Error::DbUpdateError("更新 LLM 模型绑定失败".to_string())
        })?;

        Ok(result)
    }

    /// 更新 LLM 模型绑定启用状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新启用状态请求
    pub async fn update_enabled(
        &self,
        user_id: i64,
        req: UpdateLlmModelRelEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_llm_model_rel_ownership(user_id, req.id)
            .await?;

        let model = llm_model_rel::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.llm_model_rel_dao.update(model).await.map_err(|err| {
            error!("更新 LLM 模型绑定启用状态失败, err: {:#?}", err);
            Error::DbUpdateError("更新 LLM 模型绑定启用状态失败".to_string())
        })?;

        Ok(result)
    }
}
