//! 权限校验服务
//!
//! 封装通用的资源归属校验逻辑，供多个业务服务复用。
//!
//! 提供的主要内容：
//! - `PermissionService`: 权限校验服务，注入已有 DAO 实现归属校验

use std::collections::HashSet;

use log::error;
use nject::injectable;

use err_code::Error;

use crate::dao::{
    a2a_service_registry::A2aServiceRegistryDao, a2a_service_rel::A2aServiceRelDao,
    agent_config::AgentConfigDao, command_registry::CommandRegistryDao, command_rel::CommandRelDao,
    llm_model::LlmModelDao, llm_model_rel::LlmModelRelDao,
    mcp_service_registry::McpServiceRegistryDao, mcp_service_rel::McpServiceRelDao,
    rule_registry::RuleRegistryDao, rule_rel::RuleRelDao, skill_package::SkillPackageDao,
    skill_rel::SkillRelDao, sub_agent_rel::SubAgentRelDao, tool_rel::ToolRelDao,
};
/// 封装通用的资源归属校验逻辑，供多个业务服务复用。
/// 复用已有的 DAO 层，不新增独立的权限 DAO。
#[injectable]
pub struct PermissionService {
    a2a_service_registry_dao: A2aServiceRegistryDao,
    a2a_service_rel_dao: A2aServiceRelDao,
    agent_config_dao: AgentConfigDao,
    mcp_service_registry_dao: McpServiceRegistryDao,
    command_registry_dao: CommandRegistryDao,
    command_rel_dao: CommandRelDao,
    llm_model_dao: LlmModelDao,
    llm_model_rel_dao: LlmModelRelDao,
    mcp_service_rel_dao: McpServiceRelDao,
    rule_registry_dao: RuleRegistryDao,
    rule_rel_dao: RuleRelDao,
    skill_package_dao: SkillPackageDao,
    skill_rel_dao: SkillRelDao,
    sub_agent_rel_dao: SubAgentRelDao,
    tool_rel_dao: ToolRelDao,
}

impl PermissionService {
    /// 校验智能体归属当前用户
    ///
    /// 通过 `AgentConfigDao.info(agent_id)` 查询记录，
    /// 再比对 `user_id` 是否匹配，若不存在或不属于当前用户则返回错误。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `agent_id` - 智能体 ID
    pub async fn verify_agent_ownership(&self, user_id: i64, agent_id: i64) -> anyhow::Result<()> {
        let agent = self
            .agent_config_dao
            .info(agent_id)
            .await
            .map_err(|err| {
                error!("查询智能体信息失败, err: {:#?}", err);
                Error::DbQueryError("查询智能体信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("智能体不存在或无权访问, agent_id: {}", agent_id);
                Error::DbQueryEmptyError("智能体不存在或无权访问".to_string())
            })?;

        if agent.user_id != user_id {
            error!(
                "无权操作该智能体, agent_id: {}, owner: {}, current: {}",
                agent_id, agent.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("智能体不存在或无权访问".to_string()).into());
        }

        Ok(())
    }

    /// 校验单条命令归属当前用户
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `command_id` - 命令 ID
    pub async fn verify_command_ownership(
        &self,
        user_id: i64,
        command_id: i64,
    ) -> anyhow::Result<()> {
        let command = self
            .command_registry_dao
            .info(command_id)
            .await
            .map_err(|err| {
                error!("查询命令信息失败, err: {:#?}", err);
                Error::DbQueryError("查询命令信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("命令不存在, command_id: {}", command_id);
                Error::DbQueryEmptyError("命令不存在".to_string())
            })?;

        if command.user_id != user_id {
            error!(
                "无权操作该命令, command_id: {}, owner: {}, current: {}",
                command_id, command.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("无权操作命令".to_string()).into());
        }

        Ok(())
    }

    /// 校验命令列表归属当前用户
    ///
    /// 批量查询命令信息，校验所有命令是否存在且属于当前用户。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `command_ids` - 命令 ID 列表
    pub async fn verify_commands_ownership(
        &self,
        user_id: i64,
        command_ids: &[i64],
    ) -> anyhow::Result<()> {
        let commands = self
            .command_registry_dao
            .list_by_ids(command_ids)
            .await
            .map_err(|err| {
                error!("批量查询命令信息失败, err: {:#?}", err);
                Error::DbQueryError("批量查询命令信息失败".to_string())
            })?;

        // Check all command_ids exist
        let found_ids: HashSet<i64> = commands.iter().map(|c| c.id).collect();
        for &command_id in command_ids {
            if !found_ids.contains(&command_id) {
                error!("命令不存在, command_id: {}", command_id);
                return Err(Error::DbQueryEmptyError(format!(
                    "命令不存在, command_id: {}",
                    command_id
                ))
                .into());
            }
        }

        // Check all commands belong to current user
        for command in &commands {
            if command.user_id != user_id {
                error!(
                    "无权操作该命令, command_id: {}, owner: {}, current: {}",
                    command.id, command.user_id, user_id
                );
                return Err(Error::DbQueryEmptyError(format!(
                    "无权操作命令, command_id: {}",
                    command.id
                ))
                .into());
            }
        }

        Ok(())
    }

    /// 校验命令绑定关系归属当前用户（通过 agent_id 间接校验）
    ///
    /// 先查询命令绑定关系获取 `agent_id`，再校验智能体归属。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rel_id` - 命令绑定关系 ID
    pub async fn verify_command_rel_ownership(
        &self,
        user_id: i64,
        rel_id: i64,
    ) -> anyhow::Result<()> {
        let rel = self
            .command_rel_dao
            .info(rel_id)
            .await
            .map_err(|err| {
                error!("查询命令绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询命令绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("命令绑定不存在, id: {}", rel_id);
                Error::DbQueryEmptyError("命令绑定不存在".to_string())
            })?;

        self.verify_agent_ownership(user_id, rel.agent_id).await
    }

    /// 校验单条规则归属当前用户
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rule_id` - 规则 ID
    pub async fn verify_rule_ownership(&self, user_id: i64, rule_id: i64) -> anyhow::Result<()> {
        let rule = self
            .rule_registry_dao
            .info(rule_id)
            .await
            .map_err(|err| {
                error!("查询规则信息失败, err: {:#?}", err);
                Error::DbQueryError("查询规则信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("规则不存在, rule_id: {}", rule_id);
                Error::DbQueryEmptyError("规则不存在".to_string())
            })?;

        if rule.user_id != user_id {
            error!(
                "无权操作该规则, rule_id: {}, owner: {}, current: {}",
                rule_id, rule.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("无权操作规则".to_string()).into());
        }

        Ok(())
    }

    /// 校验规则列表归属当前用户
    ///
    /// 批量查询规则信息，校验所有规则是否存在且属于当前用户。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rule_ids` - 规则 ID 列表
    pub async fn verify_rules_ownership(
        &self,
        user_id: i64,
        rule_ids: &[i64],
    ) -> anyhow::Result<()> {
        let rules = self
            .rule_registry_dao
            .list_by_ids(rule_ids)
            .await
            .map_err(|err| {
                error!("批量查询规则信息失败, err: {:#?}", err);
                Error::DbQueryError("批量查询规则信息失败".to_string())
            })?;

        // Check all rule_ids exist
        let found_ids: HashSet<i64> = rules.iter().map(|r| r.id).collect();
        for &rule_id in rule_ids {
            if !found_ids.contains(&rule_id) {
                error!("规则不存在, rule_id: {}", rule_id);
                return Err(
                    Error::DbQueryEmptyError(format!("规则不存在, rule_id: {}", rule_id)).into(),
                );
            }
        }

        // Check all rules belong to current user
        for rule in &rules {
            if rule.user_id != user_id {
                error!(
                    "无权操作该规则, rule_id: {}, owner: {}, current: {}",
                    rule.id, rule.user_id, user_id
                );
                return Err(Error::DbQueryEmptyError(format!(
                    "无权操作规则, rule_id: {}",
                    rule.id
                ))
                .into());
            }
        }

        Ok(())
    }

    /// 校验绑定关系归属当前用户（通过 agent_id 间接校验）
    ///
    /// 先查询绑定关系获取 `agent_id`，再校验智能体归属。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rel_id` - 绑定关系 ID
    pub async fn verify_rel_ownership(&self, user_id: i64, rel_id: i64) -> anyhow::Result<()> {
        let rel = self
            .rule_rel_dao
            .info(rel_id)
            .await
            .map_err(|err| {
                error!("查询规则绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询规则绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("规则绑定不存在, id: {}", rel_id);
                Error::DbQueryEmptyError("规则绑定不存在".to_string())
            })?;

        self.verify_agent_ownership(user_id, rel.agent_id).await
    }

    /// 校验工具绑定关系归属当前用户（通过 agent_id 间接校验）
    ///
    /// 先查询工具绑定关系获取 `agent_id`，再校验智能体归属。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rel_id` - 工具绑定关系 ID
    pub async fn verify_tool_rel_ownership(&self, user_id: i64, rel_id: i64) -> anyhow::Result<()> {
        let rel = self
            .tool_rel_dao
            .info(rel_id)
            .await
            .map_err(|err| {
                error!("查询工具绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询工具绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("工具绑定不存在, id: {}", rel_id);
                Error::DbQueryEmptyError("工具绑定不存在".to_string())
            })?;

        self.verify_agent_ownership(user_id, rel.agent_id).await
    }

    /// 校验 LLM 模型绑定关系归属当前用户（通过 agent_id 间接校验）
    ///
    /// 先查询 LLM 模型绑定关系获取 `agent_id`，再校验智能体归属。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rel_id` - LLM 模型绑定关系 ID
    pub async fn verify_llm_model_rel_ownership(
        &self,
        user_id: i64,
        rel_id: i64,
    ) -> anyhow::Result<()> {
        let rel = self
            .llm_model_rel_dao
            .info(rel_id)
            .await
            .map_err(|err| {
                error!("查询 LLM 模型绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询 LLM 模型绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("LLM 模型绑定不存在, id: {}", rel_id);
                Error::DbQueryEmptyError("LLM 模型绑定不存在".to_string())
            })?;

        self.verify_agent_ownership(user_id, rel.agent_id).await
    }

    /// 校验 MCP 服务注册表归属当前用户
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `registry_id` - MCP 服务注册表 ID
    pub async fn verify_mcp_registry_ownership(
        &self,
        user_id: i64,
        registry_id: i64,
    ) -> anyhow::Result<()> {
        let registry = self
            .mcp_service_registry_dao
            .info(registry_id)
            .await
            .map_err(|err| {
                error!("查询 MCP 服务注册表信息失败, err: {:#?}", err);
                Error::DbQueryError("查询 MCP 服务注册表信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("MCP 服务注册表不存在, id: {}", registry_id);
                Error::DbQueryEmptyError("MCP 服务注册表不存在".to_string())
            })?;

        if registry.user_id != user_id {
            error!(
                "无权操作该 MCP 服务注册表, id: {}, owner: {}, current: {}",
                registry_id, registry.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("无权操作 MCP 服务注册表".to_string()).into());
        }

        Ok(())
    }

    /// 校验 A2A 服务注册表归属当前用户
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `registry_id` - A2A 服务注册表 ID
    pub async fn verify_a2a_registry_ownership(
        &self,
        user_id: i64,
        registry_id: i64,
    ) -> anyhow::Result<()> {
        let registry = self
            .a2a_service_registry_dao
            .info(registry_id)
            .await
            .map_err(|err| {
                error!("查询 A2A 服务注册表信息失败, err: {:#?}", err);
                Error::DbQueryError("查询 A2A 服务注册表信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("A2A 服务注册表不存在, id: {}", registry_id);
                Error::DbQueryEmptyError("A2A 服务注册表不存在".to_string())
            })?;

        if registry.user_id != user_id {
            error!(
                "无权操作该 A2A 服务注册表, id: {}, owner: {}, current: {}",
                registry_id, registry.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("无权操作 A2A 服务注册表".to_string()).into());
        }

        Ok(())
    }

    /// 校验 MCP 服务绑定关系归属当前用户（通过 agent_id 间接校验）
    ///
    /// 先查询 MCP 服务绑定关系获取 `agent_id`，再校验智能体归属。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rel_id` - MCP 服务绑定关系 ID
    pub async fn verify_mcp_service_rel_ownership(
        &self,
        user_id: i64,
        rel_id: i64,
    ) -> anyhow::Result<()> {
        let rel = self
            .mcp_service_rel_dao
            .info(rel_id)
            .await
            .map_err(|err| {
                error!("查询 MCP 服务绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询 MCP 服务绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("MCP 服务绑定不存在, id: {}", rel_id);
                Error::DbQueryEmptyError("MCP 服务绑定不存在".to_string())
            })?;

        self.verify_agent_ownership(user_id, rel.agent_id).await
    }

    /// 校验 A2A 服务绑定关系归属当前用户（通过 agent_id 间接校验）
    ///
    /// 先查询 A2A 服务绑定关系获取 `agent_id`，再校验智能体归属。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rel_id` - A2A 服务绑定关系 ID
    pub async fn verify_a2a_service_rel_ownership(
        &self,
        user_id: i64,
        rel_id: i64,
    ) -> anyhow::Result<()> {
        let rel = self
            .a2a_service_rel_dao
            .info(rel_id)
            .await
            .map_err(|err| {
                error!("查询 A2A 服务绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询 A2A 服务绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("A2A 服务绑定不存在, id: {}", rel_id);
                Error::DbQueryEmptyError("A2A 服务绑定不存在".to_string())
            })?;

        self.verify_agent_ownership(user_id, rel.agent_id).await
    }

    /// 校验技能绑定关系归属当前用户
    ///
    /// 直接比对 `skill_rel.user_id` 是否匹配当前用户。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rel_id` - 技能绑定关系 ID
    pub async fn verify_skill_rel_ownership(
        &self,
        user_id: i64,
        rel_id: i64,
    ) -> anyhow::Result<()> {
        let rel = self
            .skill_rel_dao
            .info(rel_id)
            .await
            .map_err(|err| {
                error!("查询技能绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询技能绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("技能绑定不存在, id: {}", rel_id);
                Error::DbQueryEmptyError("技能绑定不存在".to_string())
            })?;

        if rel.user_id != user_id {
            error!(
                "无权操作该技能绑定, id: {}, owner: {}, current: {}",
                rel_id, rel.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("技能绑定不存在或无权访问".to_string()).into());
        }

        Ok(())
    }

    /// 校验 LLM 模型归属当前用户
    ///
    /// 直接比对 `llm_model.user_id` 是否匹配当前用户。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `model_id` - LLM 模型 ID
    pub async fn verify_llm_model_ownership(
        &self,
        user_id: i64,
        model_id: i64,
    ) -> anyhow::Result<()> {
        let model = self
            .llm_model_dao
            .info(model_id)
            .await
            .map_err(|err| {
                error!("查询 LLM 模型信息失败, err: {:#?}", err);
                Error::DbQueryError("查询 LLM 模型信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("LLM 模型不存在, id: {}", model_id);
                Error::DbQueryEmptyError("LLM 模型不存在".to_string())
            })?;

        if model.user_id != user_id {
            error!(
                "无权操作该 LLM 模型, id: {}, owner: {}, current: {}",
                model_id, model.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("LLM 模型不存在或无权访问".to_string()).into());
        }

        Ok(())
    }

    /// 校验子智能体绑定关系归属当前用户（通过 agent_id 间接校验）
    ///
    /// 先查询子智能体绑定关系获取 `agent_id`，再校验智能体归属。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `rel_id` - 子智能体绑定关系 ID
    pub async fn verify_sub_agent_rel_ownership(
        &self,
        user_id: i64,
        rel_id: i64,
    ) -> anyhow::Result<()> {
        let rel = self
            .sub_agent_rel_dao
            .info(rel_id)
            .await
            .map_err(|err| {
                error!("查询子智能体绑定信息失败, err: {:#?}", err);
                Error::DbQueryError("查询子智能体绑定信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("子智能体绑定不存在, id: {}", rel_id);
                Error::DbQueryEmptyError("子智能体绑定不存在".to_string())
            })?;

        self.verify_agent_ownership(user_id, rel.agent_id).await
    }

    /// 校验技能安装包归属当前用户
    ///
    /// 通过 `SkillPackageDao.info(id)` 查询记录，
    /// 再比对 `user_id` 是否匹配，若不存在或不属于当前用户则返回错误。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `package_id` - 技能安装包 ID
    pub async fn verify_skill_package_ownership(
        &self,
        user_id: i64,
        package_id: i64,
    ) -> anyhow::Result<()> {
        let record = self
            .skill_package_dao
            .info(package_id)
            .await
            .map_err(|err| {
                error!("查询技能安装包信息失败, err: {:#?}", err);
                Error::DbQueryError("查询技能安装包信息失败".to_string())
            })?
            .ok_or_else(|| {
                error!("技能安装包不存在, id: {}", package_id);
                Error::DbQueryEmptyError("技能安装包不存在或无权访问".to_string())
            })?;

        if record.user_id != user_id {
            error!(
                "无权操作该技能安装包, id: {}, owner: {}, current: {}",
                package_id, record.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("技能安装包不存在或无权访问".to_string()).into());
        }

        Ok(())
    }
}
