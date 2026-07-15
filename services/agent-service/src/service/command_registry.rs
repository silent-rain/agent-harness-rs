//! 命令注册表管理

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::command_registry;
use err_code::Error;

use crate::{
    dao::command_registry::CommandRegistryDao,
    dto::command_registry::{
        CreateCommandRegistryReq, CreateCommandRegistryResp, DeleteCommandRegistryReq,
        GetCommandRegistriesReq, GetCommandRegistryReq, UpdateCommandRegistryEnabledReq,
        UpdateCommandRegistryReq,
    },
    service::permission::PermissionService,
};

/// 服务层
#[injectable]
pub struct CommandRegistryService {
    command_registry_dao: CommandRegistryDao,
    permission: PermissionService,
}

impl CommandRegistryService {
    /// 获取列表数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetCommandRegistriesReq,
    ) -> anyhow::Result<(Vec<command_registry::Model>, u64)> {
        let (results, total) =
            self.command_registry_dao
                .list(user_id, req)
                .await
                .map_err(|err| {
                    error!("查询命令列表失败, err: {:#?}", err);
                    Error::DbQueryError("查询命令列表失败".to_string())
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
        req: GetCommandRegistryReq,
    ) -> anyhow::Result<command_registry::Model> {
        let result = self
            .command_registry_dao
            .info(req.id)
            .await
            .map_err(|err| {
                error!("查询命令信息失败, err: {:#?}", err);
                Error::DbQueryError("查询命令信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("命令不存在");
                Error::DbQueryEmptyError("命令不存在".to_string())
            })?;

        if result.user_id != user_id {
            error!(
                "无权操作该命令, command_id: {}, owner: {}, current: {}",
                req.id, result.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("无权操作命令".to_string()).into());
        }

        Ok(result)
    }

    /// 添加数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 创建请求
    pub async fn create(
        &self,
        user_id: i64,
        req: CreateCommandRegistryReq,
    ) -> anyhow::Result<CreateCommandRegistryResp> {
        // Check command name uniqueness under the same user
        self.check_name_exist(user_id, req.name.clone(), None)
            .await?;

        let model = command_registry::ActiveModel {
            user_id: Set(user_id),
            name: Set(req.name),
            description: Set(req.description),
            content: Set(req.content),
            enabled: Set(req.enabled.unwrap_or(true)),
            deleted: Set(false),
            ..Default::default()
        };

        let result = self
            .command_registry_dao
            .create(model)
            .await
            .map_err(|err| {
                error!("添加命令信息失败, err: {:#?}", err);
                Error::DbAddError("添加命令信息失败".to_string())
            })?;

        Ok(CreateCommandRegistryResp { id: result.id })
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateCommandRegistryReq) -> anyhow::Result<u64> {
        // Verify command ownership
        self.permission
            .verify_command_ownership(user_id, req.id)
            .await?;

        let model = command_registry::ActiveModel {
            id: Set(req.id),
            description: Set(req.description),
            content: Set(req.content),
            ..Default::default()
        };

        let result = self
            .command_registry_dao
            .update(model)
            .await
            .map_err(|err| {
                error!("更新命令失败, err: {:#?}", err);
                Error::DbUpdateError("更新命令失败".to_string())
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
        req: UpdateCommandRegistryEnabledReq,
    ) -> anyhow::Result<u64> {
        // Verify command ownership
        self.permission
            .verify_command_ownership(user_id, req.id)
            .await?;

        let model = command_registry::ActiveModel {
            id: Set(req.id),
            enabled: Set(req.enabled),
            ..Default::default()
        };

        let result = self
            .command_registry_dao
            .update(model)
            .await
            .map_err(|err| {
                error!("更新命令启用状态失败, err: {:#?}", err);
                Error::DbUpdateError("更新命令启用状态失败".to_string())
            })?;

        Ok(result)
    }

    /// 检查命令名称是否存在
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `name` - 命令名称
    /// * `current_id` - 当前记录 ID（更新时排除自身）
    async fn check_name_exist(
        &self,
        user_id: i64,
        name: String,
        current_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let result = self
            .command_registry_dao
            .info_by_name(user_id, name)
            .await
            .map_err(|err| {
                error!("查询命令名称失败, err: {:#?}", err);
                Error::DbQueryError("查询命令名称失败".to_string())
            })?;

        if let Some(model) = result
            && (current_id.is_none() || Some(model.id) != current_id)
        {
            error!("命令名称已存在");
            return Err(Error::DbDataExistError("命令名称已存在".to_string()).into());
        }

        Ok(())
    }

    /// 删除数据（软删除）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 删除请求
    pub async fn delete(&self, user_id: i64, req: DeleteCommandRegistryReq) -> anyhow::Result<u64> {
        // Verify command ownership
        self.permission
            .verify_command_ownership(user_id, req.id)
            .await?;

        let result = self
            .command_registry_dao
            .delete(req.id)
            .await
            .map_err(|err| {
                error!("删除命令信息失败, err: {:#?}", err);
                Error::DbDeleteError("删除命令信息失败".to_string())
            })?;

        Ok(result)
    }
}
