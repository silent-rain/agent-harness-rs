//! 智能体 MCP 服务绑定关系管理

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::mcp_service_rel::{self, McpServiceRelWithRegistry};
use err_code::Error;

use crate::{
    dao::mcp_service_rel::McpServiceRelDao,
    dto::mcp_service_rel::{
        AddMcpServiceRelReq, DeleteMcpServiceRelReq, GetMcpServiceRelsReq,
        UpdateMcpServiceRelEnabledReq, UpdateMcpServiceRelReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct McpServiceRelService {
    mcp_service_rel_dao: McpServiceRelDao,
    permission_service: PermissionService,
}

impl McpServiceRelService {
    /// 获取指定智能体的 MCP 服务绑定列表（含注册表详情）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetMcpServiceRelsReq,
    ) -> anyhow::Result<(Vec<McpServiceRelWithRegistry>, u64)> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let (items, total) = self.mcp_service_rel_dao.list(&req).await.map_err(|err| {
            error!("查询 MCP 服务绑定列表失败, err: {:#?}", err);
            Error::DbQueryError("查询 MCP 服务绑定列表失败".to_string())
        })?;

        Ok((items, total))
    }

    /// 新增 MCP 服务绑定
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 新增请求
    pub async fn add(
        &self,
        user_id: i64,
        req: AddMcpServiceRelReq,
    ) -> anyhow::Result<mcp_service_rel::Model> {
        // Verify agent ownership
        self.permission_service
            .verify_agent_ownership(user_id, req.agent_id)
            .await?;

        let model = mcp_service_rel::ActiveModel {
            agent_id: Set(req.agent_id),
            registry_id: Set(req.registry_id),
            enabled_tools: Set(req.enabled_tools),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self.mcp_service_rel_dao.add(model).await.map_err(|err| {
            error!("新增 MCP 服务绑定失败, err: {:#?}", err);
            Error::DbAddError("新增 MCP 服务绑定失败".to_string())
        })?;

        Ok(result)
    }

    /// 更新 MCP 服务绑定
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateMcpServiceRelReq) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_mcp_service_rel_ownership(user_id, req.id)
            .await?;

        let model = mcp_service_rel::ActiveModel {
            id: Set(req.id),
            enabled_tools: Set(req.enabled_tools),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self
            .mcp_service_rel_dao
            .update(model)
            .await
            .map_err(|err| {
                error!("更新 MCP 服务绑定失败, err: {:#?}", err);
                Error::DbUpdateError("更新 MCP 服务绑定失败".to_string())
            })?;

        Ok(result)
    }

    /// 更新 MCP 服务绑定启用状态
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新启用状态请求
    pub async fn update_enabled(
        &self,
        user_id: i64,
        req: UpdateMcpServiceRelEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_mcp_service_rel_ownership(user_id, req.id)
            .await?;

        let model = mcp_service_rel::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self
            .mcp_service_rel_dao
            .update(model)
            .await
            .map_err(|err| {
                error!("更新 MCP 服务绑定启用状态失败, err: {:#?}", err);
                Error::DbUpdateError("更新 MCP 服务绑定启用状态失败".to_string())
            })?;

        Ok(result)
    }

    /// 删除 MCP 服务绑定
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 删除请求
    pub async fn delete(&self, user_id: i64, req: DeleteMcpServiceRelReq) -> anyhow::Result<u64> {
        // Verify ownership via the binding's agent_id
        self.permission_service
            .verify_mcp_service_rel_ownership(user_id, req.id)
            .await?;

        let result = self
            .mcp_service_rel_dao
            .delete(req.id)
            .await
            .map_err(|err| {
                error!("删除 MCP 服务绑定失败, err: {:#?}", err);
                Error::DbDeleteError("删除 MCP 服务绑定失败".to_string())
            })?;

        Ok(result)
    }
}
