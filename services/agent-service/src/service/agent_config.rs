//! 智能体配置管理

use log::error;
use nject::injectable;
use sea_orm::Set;

use entity::agent::agent_config;
use err_code::Error;

use crate::{
    dao::agent_config::AgentConfigDao,
    dto::agent_config::{
        CreateAgentConfigReq, CreateAgentConfigResp, DeleteAgentConfigReq, GetAgentConfigReq,
        GetAgentConfigsReq, UpdateAgentConfigReq,
    },
    enums::agent_config::AgentStatus,
    service::permission::PermissionService,
};

/// Default max delegation depth for new agents
const DEFAULT_MAX_DELEGATION_DEPTH: i32 = 3;

/// 服务层
#[injectable]
pub struct AgentConfigService {
    agent_config_dao: AgentConfigDao,
    permission: PermissionService,
}

impl AgentConfigService {
    /// 获取列表数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetAgentConfigsReq,
    ) -> anyhow::Result<(Vec<agent_config::Model>, u64)> {
        let (results, total) = self
            .agent_config_dao
            .list(user_id, req)
            .await
            .map_err(|err| {
                error!("查询智能体列表失败, err: {:#?}", err);
                Error::DbQueryError("查询智能体列表失败".to_string())
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
        req: GetAgentConfigReq,
    ) -> anyhow::Result<agent_config::Model> {
        let result = self
            .agent_config_dao
            .info(req.id)
            .await
            .map_err(|err| {
                error!("查询智能体信息失败, err: {:#?}", err);
                Error::DbQueryError("查询智能体信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("智能体不存在");
                Error::DbQueryEmptyError("智能体不存在".to_string())
            })?;

        if result.user_id != user_id {
            error!(
                "智能体不存在或无权访问, agent_id: {}, owner: {}, current: {}",
                req.id, result.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("智能体不存在或无权访问".to_string()).into());
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
        req: CreateAgentConfigReq,
    ) -> anyhow::Result<CreateAgentConfigResp> {
        // Check agent name uniqueness under the same user
        self.check_name_exist(user_id, req.name.clone(), None)
            .await?;

        let model = agent_config::ActiveModel {
            user_id: Set(user_id),
            name: Set(req.name),
            description: Set(req.description),
            welcome_message: Set(req.welcome_message),
            system_prompt: Set(req.system_prompt),
            max_delegation_depth: Set(req
                .max_delegation_depth
                .unwrap_or(DEFAULT_MAX_DELEGATION_DEPTH)),
            status: Set(AgentStatus::Active.into()),
            tags_json: Set(String::new()),
            deleted: Set(false),
            ..Default::default()
        };

        let result = self.agent_config_dao.create(model).await.map_err(|err| {
            error!("添加智能体信息失败, err: {:#?}", err);
            Error::DbAddError("添加智能体信息失败".to_string())
        })?;

        Ok(CreateAgentConfigResp { id: result.id })
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 更新请求
    pub async fn update(&self, user_id: i64, req: UpdateAgentConfigReq) -> anyhow::Result<u64> {
        // Verify agent ownership
        self.permission
            .verify_agent_ownership(user_id, req.id)
            .await?;

        // Check agent name uniqueness excluding current record
        self.check_name_exist(user_id, req.name.clone(), Some(req.id))
            .await?;

        let mut model = agent_config::ActiveModel {
            id: Set(req.id),
            name: Set(req.name),
            description: Set(req.description),
            welcome_message: Set(req.welcome_message),
            system_prompt: Set(req.system_prompt),
            ..Default::default()
        };

        if let Some(depth) = req.max_delegation_depth {
            model.max_delegation_depth = Set(depth);
        }

        let result = self.agent_config_dao.update(model).await.map_err(|err| {
            error!("更新智能体失败, err: {:#?}", err);
            Error::DbUpdateError("更新智能体失败".to_string())
        })?;

        Ok(result)
    }

    /// 检查智能体名称是否存在
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `name` - 智能体名称
    /// * `current_id` - 当前记录 ID（更新时排除自身）
    async fn check_name_exist(
        &self,
        user_id: i64,
        name: String,
        current_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let result = self
            .agent_config_dao
            .info_by_name(user_id, name)
            .await
            .map_err(|err| {
                error!("查询智能体名称失败, err: {:#?}", err);
                Error::DbQueryError("查询智能体名称失败".to_string())
            })?;

        if let Some(model) = result
            && (current_id.is_none() || Some(model.id) != current_id)
        {
            error!("智能体名称已存在");
            return Err(Error::DbDataExistError("智能体名称已存在".to_string()).into());
        }

        Ok(())
    }

    /// 删除数据（软删除）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 删除请求
    pub async fn delete(&self, user_id: i64, req: DeleteAgentConfigReq) -> anyhow::Result<u64> {
        // Verify agent ownership
        self.permission
            .verify_agent_ownership(user_id, req.id)
            .await?;

        let result = self.agent_config_dao.delete(req.id).await.map_err(|err| {
            error!("删除智能体信息失败, err: {:#?}", err);
            Error::DbDeleteError("删除智能体信息失败".to_string())
        })?;

        Ok(result)
    }
}
