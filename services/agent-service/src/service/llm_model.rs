//! LLM 模型配置管理业务逻辑层

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::llm_model;
use err_code::Error;

use crate::{
    dao::llm_model::LlmModelDao,
    dto::llm_model::{
        CreateLlmModelReq, CreateLlmModelResp, DeleteLlmModelReq, GetLlmModelReq, GetLlmModelsReq,
        UpdateLlmModelEnabledReq, UpdateLlmModelReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct LlmModelService {
    llm_model_dao: LlmModelDao,
    permission: PermissionService,
}

impl LlmModelService {
    /// 获取当前用户的 LLM 模型列表
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetLlmModelsReq,
    ) -> anyhow::Result<(Vec<llm_model::Model>, u64)> {
        let (items, total) = self.llm_model_dao.list(user_id, req).await.map_err(|err| {
            error!("查询 LLM 模型列表失败, err: {:#?}", err);
            Error::DbQueryError("查询 LLM 模型列表失败".to_string())
        })?;

        Ok((items, total))
    }

    /// 获取 LLM 模型详情
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 详情查询请求
    pub async fn info(
        &self,
        user_id: i64,
        req: GetLlmModelReq,
    ) -> anyhow::Result<llm_model::Model> {
        let result = self
            .llm_model_dao
            .info(req.id)
            .await
            .map_err(|err| {
                error!("查询 LLM 模型信息失败, err: {:#?}", err);
                Error::DbQueryError("查询 LLM 模型信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("LLM 模型不存在, id: {}", req.id);
                Error::DbQueryEmptyError("LLM 模型不存在".to_string())
            })?;

        if result.user_id != user_id {
            error!(
                "无权访问该 LLM 模型, id: {}, owner: {}, current: {}",
                req.id, result.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("LLM 模型不存在或无权访问".to_string()).into());
        }

        Ok(result)
    }

    /// 创建 LLM 模型
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 创建请求
    pub async fn create(
        &self,
        user_id: i64,
        req: CreateLlmModelReq,
    ) -> anyhow::Result<CreateLlmModelResp> {
        // Check provider + model_code uniqueness for current user
        self.check_model_code_exist(user_id, &req.provider, &req.model_code, None)
            .await?;

        let model = llm_model::ActiveModel {
            user_id: Set(user_id),
            provider: Set(req.provider),
            model_code: Set(req.model_code),
            model_name: Set(req.model_name),
            base_url: Set(req.base_url),
            api_key: Set(req.api_key),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.llm_model_dao.create(model).await.map_err(|err| {
            error!("创建 LLM 模型失败, err: {:#?}", err);
            Error::DbAddError("创建 LLM 模型失败".to_string())
        })?;

        Ok(CreateLlmModelResp { id: result.id })
    }

    /// 更新 LLM 模型
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateLlmModelReq) -> anyhow::Result<u64> {
        // Verify ownership
        self.permission
            .verify_llm_model_ownership(user_id, req.id)
            .await?;

        let model = llm_model::ActiveModel {
            id: Set(req.id),
            base_url: Set(req.base_url),
            api_key: Set(req.api_key),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.llm_model_dao.update(model).await.map_err(|err| {
            error!("更新 LLM 模型失败, err: {:#?}", err);
            Error::DbUpdateError("更新 LLM 模型失败".to_string())
        })?;

        Ok(result)
    }

    /// 更新 LLM 模型启用状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新启用状态请求
    pub async fn update_enabled(
        &self,
        user_id: i64,
        req: UpdateLlmModelEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership
        self.permission
            .verify_llm_model_ownership(user_id, req.id)
            .await?;

        let model = llm_model::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.llm_model_dao.update(model).await.map_err(|err| {
            error!("更新 LLM 模型启用状态失败, err: {:#?}", err);
            Error::DbUpdateError("更新 LLM 模型启用状态失败".to_string())
        })?;

        Ok(result)
    }

    /// 删除 LLM 模型
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 删除请求
    pub async fn delete(&self, user_id: i64, req: DeleteLlmModelReq) -> anyhow::Result<u64> {
        // Verify ownership
        self.permission
            .verify_llm_model_ownership(user_id, req.id)
            .await?;

        let result = self.llm_model_dao.delete(req.id).await.map_err(|err| {
            error!("删除 LLM 模型失败, err: {:#?}", err);
            Error::DbDeleteError("删除 LLM 模型失败".to_string())
        })?;

        Ok(result)
    }

    /// 检查同一用户下供应商 + 模型编码是否已存在
    ///
    /// # Parameters
    ///
    /// * `user_id` - 用户 ID
    /// * `provider` - 供应商
    /// * `model_code` - 模型编码
    /// * `current_id` - 当前记录 ID（更新时排除自身）
    async fn check_model_code_exist(
        &self,
        user_id: i64,
        provider: &str,
        model_code: &str,
        current_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let result = self
            .llm_model_dao
            .info_by_user_provider_code(user_id, provider, model_code)
            .await
            .map_err(|err| {
                error!("查询 LLM 模型编码失败, err: {:#?}", err);
                Error::DbQueryError("查询 LLM 模型编码失败".to_string())
            })?;

        if let Some(model) = result
            && (current_id.is_none() || Some(model.id) != current_id)
        {
            error!(
                "LLM 模型编码已存在, provider: {}, model_code: {}",
                provider, model_code
            );
            return Err(Error::DbDataExistError("该供应商下模型编码已存在".to_string()).into());
        }

        Ok(())
    }
}
