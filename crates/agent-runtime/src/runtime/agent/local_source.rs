//! 本地目录扫描源
//!
//! 通过本地文件系统的 `.codebuddy/*` 目录加载 rules / skills / mcp / commands /
//! sub_agents 配置，作为一个 [`AgentSource`] 注入 [`AgentBuilder`]。
//!
//! 取代旧的 `enable_local_scan` 布尔开关与 `with_local_scan()` 链式方法。

use async_trait::async_trait;

#[cfg(feature = "command")]
use crate::command::LocalCommandLoader;
#[cfg(feature = "mcp")]
use crate::mcp::LocalMcpLoader;
#[cfg(feature = "rule")]
use crate::rule::LocalRuleLoader;
#[cfg(feature = "skill")]
use crate::skill::LocalSkillLoader;
#[cfg(feature = "sub_agent")]
use crate::sub_agent::LocalSubAgentLoader;

use crate::{
    error::AgentResult,
    runtime::agent::AgentSource,
    types::runtime::{AgentBundle, ModelConfig},
};

/// 本地目录扫描源
///
/// 启用时会从默认路径扫描以下能力配置：
/// - `.codebuddy/rules` — 本地规则文件
/// - `.codebuddy/skills` — 本地技能文件
/// - `.codebuddy/mcp/mcp.json` — 本地 MCP 服务配置
/// - `.codebuddy/commands` — 本地命令文件
/// - `.codebuddy/agents` — 本地子智能体文件
///
/// 各能力可独立开关，未启用的类型不会扫描磁盘。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_base::runtime::LocalAgentSource;
///
/// // 全部启用（最常见场景）
/// let source = LocalAgentSource::default().with_all_enabled();
///
/// // 仅启用规则与技能
/// let source = LocalAgentSource::default().with_rules(true).with_skills(true);
/// ```
#[derive(Debug, Clone, Default)]
pub struct LocalAgentSource {
    /// 是否扫描本地规则
    pub include_rules: bool,
    /// 是否扫描本地技能
    pub include_skills: bool,
    /// 是否扫描本地 MCP 配置
    pub include_mcp: bool,
    /// 是否扫描本地命令
    pub include_commands: bool,
    /// 是否扫描本地子智能体
    pub include_sub_agents: bool,
    /// 子智能体使用的默认 [`ModelConfig`]（仅在 `include_sub_agents` 启用时生效）
    pub sub_agent_model_config: Option<ModelConfig>,
    /// 子智能所属用户ID, 仅在 `include_sub_agents` 启用时生效
    pub sub_agent_user_id: Option<String>,
}

impl LocalAgentSource {
    /// 设置子智能体所属用户ID
    ///
    /// 本地子智能体没有自带用户ID，需要从父 Agent 继承用户ID。
    pub fn with_sub_agent_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.sub_agent_user_id = Some(user_id.into());
        self
    }

    /// 设置子智能体扫描时使用的默认 [`ModelConfig`]
    ///
    /// 本地子智能体没有自带模型绑定，需要从父 Agent 继承模型配置。
    pub fn with_sub_agent_model_config(mut self, model_config: ModelConfig) -> Self {
        self.sub_agent_model_config = Some(model_config);
        self
    }

    /// 启用所有本地能力扫描
    pub fn with_all_enabled(mut self) -> Self {
        self.include_rules = true;
        self.include_skills = true;
        self.include_mcp = true;
        self.include_commands = true;
        self.include_sub_agents = true;
        self.sub_agent_model_config = None;
        self
    }

    /// 设置是否扫描规则
    pub fn with_rules(mut self, enable: bool) -> Self {
        self.include_rules = enable;
        self
    }

    /// 设置是否扫描技能
    pub fn with_skills(mut self, enable: bool) -> Self {
        self.include_skills = enable;
        self
    }

    /// 设置是否扫描 MCP 配置
    pub fn with_mcp(mut self, enable: bool) -> Self {
        self.include_mcp = enable;
        self
    }

    /// 设置是否扫描命令
    pub fn with_commands(mut self, enable: bool) -> Self {
        self.include_commands = enable;
        self
    }

    /// 设置是否扫描子智能体
    pub fn with_sub_agents(mut self, enable: bool) -> Self {
        self.include_sub_agents = enable;
        self
    }
}

#[async_trait]
impl AgentSource for LocalAgentSource {
    async fn load(&self) -> AgentResult<AgentBundle> {
        #[allow(unused_mut)]
        let mut bundle = AgentBundle::default();

        #[cfg(feature = "rule")]
        if self.include_rules {
            bundle.rule_configs = LocalRuleLoader::default().load().await?;
        }

        #[cfg(feature = "skill")]
        if self.include_skills {
            bundle.skill_configs = LocalSkillLoader::default().load().await?;
        }

        #[cfg(feature = "mcp")]
        if self.include_mcp {
            bundle.mcp_configs = LocalMcpLoader::default().load().await?;
        }

        #[cfg(feature = "command")]
        if self.include_commands {
            bundle.command_configs = LocalCommandLoader::default().load().await?;
        }

        #[cfg(feature = "sub_agent")]
        if self.include_sub_agents {
            let mut loader = LocalSubAgentLoader::default();
            if let Some(model_config) = self.sub_agent_model_config.clone() {
                loader = loader.with_model_config(model_config);
            }
            if let Some(sub_agent_user_id) = self.sub_agent_user_id.clone() {
                loader = loader.with_user_id(sub_agent_user_id);
            }
            bundle.sub_agent_bundles = loader.load().await?;
        }

        Ok(bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_enables_every_flag() {
        let s = LocalAgentSource::default().with_all_enabled();
        assert!(s.include_rules);
        assert!(s.include_skills);
        assert!(s.include_mcp);
        assert!(s.include_commands);
        assert!(s.include_sub_agents);
    }

    #[test]
    fn none_disables_every_flag() {
        let s = LocalAgentSource::default();
        assert!(!s.include_rules);
        assert!(!s.include_skills);
        assert!(!s.include_mcp);
        assert!(!s.include_commands);
        assert!(!s.include_sub_agents);
    }

    #[test]
    fn fluent_setters_work() {
        let s = LocalAgentSource::default()
            .with_rules(true)
            .with_skills(true);
        assert!(s.include_rules);
        assert!(s.include_skills);
        assert!(!s.include_mcp);
    }
}
