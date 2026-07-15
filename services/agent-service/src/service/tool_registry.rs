//! 工具注册表管理

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::tool_registry;
use err_code::Error;

use crate::{
    dao::tool_registry::ToolRegistryDao,
    dto::tool_registry::{
        CreateToolRegistryReq, CreateToolRegistryResp, DeleteToolRegistryReq, GetToolRegistriesReq,
        GetToolRegistryReq, UpdateToolRegistryEnabledReq, UpdateToolRegistryReq,
    },
};

/// 服务层
#[injectable]
pub struct ToolRegistryService {
    tool_registry_dao: ToolRegistryDao,
}

impl ToolRegistryService {
    /// 获取列表数据
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        req: GetToolRegistriesReq,
    ) -> anyhow::Result<(Vec<tool_registry::Model>, u64)> {
        // Return all data when requested
        if let Some(true) = req.all {
            let (results, total) = self.tool_registry_dao.all().await.map_err(|err| {
                error!("查询所有工具失败, err: {:#?}", err);
                Error::DbQueryError("查询所有工具失败".to_string())
            })?;
            return Ok((results, total));
        }

        let (results, total) = self.tool_registry_dao.list(req).await.map_err(|err| {
            error!("查询工具列表失败, err: {:#?}", err);
            Error::DbQueryError("查询工具列表失败".to_string())
        })?;

        Ok((results, total))
    }

    /// 获取详情数据
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求
    pub async fn info(&self, req: GetToolRegistryReq) -> anyhow::Result<tool_registry::Model> {
        let result = self
            .tool_registry_dao
            .info(req.id)
            .await
            .map_err(|err| {
                error!("查询工具信息失败, err: {:#?}", err);
                Error::DbQueryError("查询工具信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("工具不存在");
                Error::DbQueryEmptyError("工具不存在".to_string())
            })?;

        Ok(result)
    }

    /// 添加数据
    ///
    /// # Parameters
    ///
    /// * `req` - 创建请求
    pub async fn create(
        &self,
        req: CreateToolRegistryReq,
    ) -> anyhow::Result<CreateToolRegistryResp> {
        // Check tool_code uniqueness
        self.check_tool_code_exist(req.tool_code.clone(), None)
            .await?;

        let model = tool_registry::ActiveModel {
            tool_code: Set(req.tool_code),
            name: Set(req.name),
            description: Set(req.description),
            schema_json: Set(req.schema_json),
            enabled: Set(req.enabled.unwrap_or(true)),
            ..Default::default()
        };

        let result = self.tool_registry_dao.create(model).await.map_err(|err| {
            error!("添加工具信息失败, err: {:#?}", err);
            Error::DbAddError("添加工具信息失败".to_string())
        })?;

        Ok(CreateToolRegistryResp { id: result.id })
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `req` - 更新请求
    pub async fn update(&self, req: UpdateToolRegistryReq) -> anyhow::Result<u64> {
        // Check tool_code uniqueness excluding current record
        self.check_tool_code_exist(req.tool_code.clone(), Some(req.id))
            .await?;

        let model = tool_registry::ActiveModel {
            id: Set(req.id),
            tool_code: Set(req.tool_code),
            name: Set(req.name),
            description: Set(req.description),
            schema_json: Set(req.schema_json),
            ..Default::default()
        };

        let result = self.tool_registry_dao.update(model).await.map_err(|err| {
            error!("更新工具失败, err: {:#?}", err);
            Error::DbUpdateError("更新工具失败".to_string())
        })?;

        Ok(result)
    }

    /// 更新启用状态
    ///
    /// # Parameters
    ///
    /// * `req` - 更新启用状态请求
    pub async fn update_enabled(&self, req: UpdateToolRegistryEnabledReq) -> anyhow::Result<u64> {
        let model = tool_registry::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.tool_registry_dao.update(model).await.map_err(|err| {
            error!("更新工具启用状态失败, err: {:#?}", err);
            Error::DbUpdateError("更新工具启用状态失败".to_string())
        })?;

        Ok(result)
    }

    /// 检查工具编码是否存在
    ///
    /// # Parameters
    ///
    /// * `tool_code` - 工具唯一编码
    /// * `current_id` - 当前记录 ID（更新时排除自身）
    async fn check_tool_code_exist(
        &self,
        tool_code: String,
        current_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let result = self
            .tool_registry_dao
            .info_by_tool_code(tool_code)
            .await
            .map_err(|err| {
                error!("查询工具编码失败, err: {:#?}", err);
                Error::DbQueryError("查询工具编码失败".to_string())
            })?;

        if let Some(model) = result
            && (current_id.is_none() || Some(model.id) != current_id)
        {
            error!("工具编码已存在");
            return Err(Error::DbDataExistError("工具编码已存在".to_string()).into());
        }

        Ok(())
    }

    /// 删除数据（物理删除）
    ///
    /// # Parameters
    ///
    /// * `req` - 删除请求
    pub async fn delete(&self, req: DeleteToolRegistryReq) -> anyhow::Result<u64> {
        let result = self.tool_registry_dao.delete(req.id).await.map_err(|err| {
            error!("删除工具信息失败, err: {:#?}", err);
            Error::DbDeleteError("删除工具信息失败".to_string())
        })?;

        Ok(result)
    }
}
