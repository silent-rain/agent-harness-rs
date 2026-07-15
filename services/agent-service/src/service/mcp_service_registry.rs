//! MCP 服务注册表管理

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::mcp_service_registry;
use err_code::Error;

use crate::{
    dao::mcp_service_registry::McpServiceRegistryDao,
    dto::mcp_service_registry::{
        CreateMcpServiceRegistryReq, CreateMcpServiceRegistryResp, DeleteMcpServiceRegistryReq,
        GetMcpServiceRegistriesReq, GetMcpServiceRegistryReq, UpdateMcpServiceRegistryEnabledReq,
        UpdateMcpServiceRegistryReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct McpServiceRegistryService {
    mcp_service_registry_dao: McpServiceRegistryDao,
    permission: PermissionService,
}

impl McpServiceRegistryService {
    /// 获取列表数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetMcpServiceRegistriesReq,
    ) -> anyhow::Result<(Vec<mcp_service_registry::Model>, u64)> {
        let (results, total) = self
            .mcp_service_registry_dao
            .list(user_id, req)
            .await
            .map_err(|err| {
                error!("查询 MCP 服务注册表列表失败, err: {:#?}", err);
                Error::DbQueryError("查询 MCP 服务注册表列表失败".to_string())
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
        req: GetMcpServiceRegistryReq,
    ) -> anyhow::Result<mcp_service_registry::Model> {
        let result = self
            .mcp_service_registry_dao
            .info(req.id)
            .await
            .map_err(|err| {
                error!("查询 MCP 服务注册表信息失败, err: {:#?}", err);
                Error::DbQueryError("查询 MCP 服务注册表信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("MCP 服务注册表不存在");
                Error::DbQueryEmptyError("MCP 服务注册表不存在".to_string())
            })?;

        if result.user_id != user_id {
            error!(
                "无权操作该 MCP 服务注册表, id: {}, owner: {}, current: {}",
                req.id, result.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("无权操作 MCP 服务注册表".to_string()).into());
        }

        Ok(result)
    }

    /// 创建 MCP 服务注册表
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 创建请求
    pub async fn create(
        &self,
        user_id: i64,
        req: CreateMcpServiceRegistryReq,
    ) -> anyhow::Result<CreateMcpServiceRegistryResp> {
        // Check service name uniqueness under the same user
        self.check_name_exist(user_id, req.name.clone(), None)
            .await?;

        let model = mcp_service_registry::ActiveModel {
            user_id: Set(user_id),
            name: Set(req.name),
            description: Set(req.description),
            mcp_service_url: Set(req.mcp_service_url),
            transport_type: Set(req.transport_type),
            enabled_tools: Set(req.enabled_tools.unwrap_or_default()),
            config_json: Set(req.config_json),
            enabled: Set(req.enabled.unwrap_or(true)),
            deleted: Set(false),
            ..Default::default()
        };

        let result = self
            .mcp_service_registry_dao
            .create(model)
            .await
            .map_err(|err| {
                error!("创建 MCP 服务注册表失败, err: {:#?}", err);
                Error::DbAddError("创建 MCP 服务注册表失败".to_string())
            })?;

        Ok(CreateMcpServiceRegistryResp { id: result.id })
    }

    /// 更新 MCP 服务注册表
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(
        &self,
        user_id: i64,
        req: UpdateMcpServiceRegistryReq,
    ) -> anyhow::Result<u64> {
        // Verify registry ownership
        self.permission
            .verify_mcp_registry_ownership(user_id, req.id)
            .await?;

        // Check service name uniqueness excluding current record
        self.check_name_exist(user_id, req.name.clone(), Some(req.id))
            .await?;

        let model = mcp_service_registry::ActiveModel {
            id: Set(req.id),
            name: Set(req.name),
            description: Set(req.description),
            mcp_service_url: Set(req.mcp_service_url),
            transport_type: Set(req.transport_type),
            enabled_tools: Set(req.enabled_tools.unwrap_or_default()),
            config_json: Set(req.config_json),
            ..Default::default()
        };

        let result = self
            .mcp_service_registry_dao
            .update(model)
            .await
            .map_err(|err| {
                error!("更新 MCP 服务注册表失败, err: {:#?}", err);
                Error::DbUpdateError("更新 MCP 服务注册表失败".to_string())
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
        req: UpdateMcpServiceRegistryEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify registry ownership
        self.permission
            .verify_mcp_registry_ownership(user_id, req.id)
            .await?;

        let model = mcp_service_registry::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self
            .mcp_service_registry_dao
            .update(model)
            .await
            .map_err(|err| {
                error!("更新 MCP 服务注册表启用状态失败, err: {:#?}", err);
                Error::DbUpdateError("更新 MCP 服务注册表启用状态失败".to_string())
            })?;

        Ok(result)
    }

    /// 删除 MCP 服务注册表（软删除）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 删除请求
    pub async fn delete(
        &self,
        user_id: i64,
        req: DeleteMcpServiceRegistryReq,
    ) -> anyhow::Result<u64> {
        // Verify registry ownership
        self.permission
            .verify_mcp_registry_ownership(user_id, req.id)
            .await?;

        let result = self
            .mcp_service_registry_dao
            .delete(req.id)
            .await
            .map_err(|err| {
                error!("删除 MCP 服务注册表失败, err: {:#?}", err);
                Error::DbDeleteError("删除 MCP 服务注册表失败".to_string())
            })?;

        Ok(result)
    }

    /// 检查服务名称是否已存在（用户维度唯一性）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `name` - 服务名称
    /// * `current_id` - 当前记录 ID（更新时排除自身）
    async fn check_name_exist(
        &self,
        user_id: i64,
        name: String,
        current_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let result = self
            .mcp_service_registry_dao
            .info_by_name(user_id, name)
            .await
            .map_err(|err| {
                error!("查询 MCP 服务名称失败, err: {:#?}", err);
                Error::DbQueryError("查询 MCP 服务名称失败".to_string())
            })?;

        if let Some(model) = result
            && (current_id.is_none() || Some(model.id) != current_id)
        {
            error!("MCP 服务名称已存在");
            return Err(Error::DbDataExistError("MCP 服务名称已存在".to_string()).into());
        }

        Ok(())
    }
}
