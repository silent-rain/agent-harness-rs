//! 本地子智能体加载器
//!
//! 从本地文件系统扫描 agents 目录，将每个 `.md` 文件解析并转换为
//! [`AgentBundle`](crate::types::runtime::AgentBundle)。
//!
//! ## 设计
//!
//! 加载器仅做"被动扫描"：每次 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
//! 加载结果由调用方通过
//! [`AgentBuilder::add_sub_agents`](crate::runtime::AgentBuilder::add_sub_agents)
//! 注入到 bundle 中统一合并。
//!
//! ## 名称引用解析
//!
//! 子智能体 frontmatter 中声明的 `tools`、`rules`、`skills`、`mcpTools`、`commands`、`a2aAgents`
//! 均为**名称引用**。`to_bundle` 方法会根据名称去对应的工作空间目录读取实际文件内容，
//! 将其解析为完整的配置对象：
//!
//! - `rules` → `.codebuddy/rules/<name>.md` 或 `.txt`
//! - `skills` → `.codebuddy/skills/<name>/SKILL.md`
//! - `commands` → `.codebuddy/commands/<name>.md`
//! - `mcpTools` → `.codebuddy/mcp/mcp.json` 中按 service_name 过滤
//! - `a2aAgents` → `.codebuddy/a2a/a2a.json` 中按 service_name 过滤
//! - `tools` → 直接转为 `ToolConfig`（仅名称占位符，由运行时工具池匹配）
//!
//! 提供的主要内容：
//! - [`LocalSubAgentLoader`] - 本地文件系统子智能体加载器

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tokio::fs;
use tracing::{debug, info, warn};

use crate::{
    Error,
    constant::{DEFAULT_SUB_AGENTS_DIR, DEFAULT_WORKSPACE_DIR, SUB_AGENT_ENTRY_EXTENSIONS},
    sub_agent::SubAgentManifest,
    types::{
        runtime::{AgentBundle, AgentDefinition, ModelConfig},
        sub_agent::LocalSubAgentConfig,
        tool::ToolConfig,
    },
};

#[cfg(feature = "a2a")]
use crate::{
    a2a::loader::LocalA2aLoader,
    constant::{DEFAULT_A2A_CONFIG_DIR, DEFAULT_A2A_CONFIG_FILE},
    types::a2a::A2aServiceConfig,
};
#[cfg(feature = "command")]
use crate::{
    command::LocalCommandLoader,
    constant::{COMMAND_ENTRY_EXTENSIONS, DEFAULT_COMMANDS_DIR},
    types::command::CommandConfig,
};
#[cfg(feature = "skill")]
use crate::{constant::DEFAULT_SKILLS_DIR, skill::LocalSkillLoader, types::skill::SkillConfig};
#[cfg(feature = "mcp")]
use crate::{
    constant::{DEFAULT_MCP_CONFIG_DIR, DEFAULT_MCP_CONFIG_FILE},
    mcp::LocalMcpLoader,
    types::mcp::McpServerConfig,
};
#[cfg(feature = "rule")]
use crate::{
    constant::{DEFAULT_RULES_DIR, RULE_ENTRY_EXTENSIONS},
    rule::LocalRuleLoader,
    types::rule::RuleConfig,
};

/// 本地子智能体加载器
///
/// 扫描指定目录下匹配扩展名的文件（非递归），将每个文件解析并转换为
/// [`AgentBundle`]。文件名（不含扩展名）作为子智能体的默认 `name`，
/// frontmatter 中显式声明的 `name` 优先。
///
/// 设计上保持"被动"：仅在 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
/// 加载结果由调用方通过
/// [`AgentBuilder::add_sub_agents`](crate::runtime::AgentBuilder::add_sub_agents)
/// 注入到 bundle 中统一合并。
///
/// ## 使用示例
///
/// ```rust,no_run
/// use agent_base::sub_agent::LocalSubAgentLoader;
///
/// # async fn example() -> anyhow::Result<()> {
/// let loader = LocalSubAgentLoader::new(".codebuddy/agents");
/// let bundles = loader.load().await?;
///
/// // 通过 AgentBuilder::add_sub_agents 注入到 bundle
/// // builder.add_sub_agents(bundles);
/// # Ok(())
/// # }
/// ```
pub struct LocalSubAgentLoader {
    /// Agents 根目录路径
    agents_root: PathBuf,
    /// 工作空间根目录（`agents_root` 的父目录，用于定位 rules/skills/commands/mcp）
    workspace_root: PathBuf,
    /// 支持的智能体文件扩展名集合（不含前缀点，如 `"md"`）
    agent_extensions: HashSet<String>,
    /// 规则文件扩展名列表（不含前缀点），用于名称引用解析时遍历查找
    #[cfg(feature = "rule")]
    rule_extensions: Vec<String>,
    /// 命令文件扩展名列表（不含前缀点），用于名称引用解析时遍历查找
    #[cfg(feature = "command")]
    command_extensions: Vec<String>,
    /// 模型绑定配置, 本地智能体的加载需要主动传递模型配置
    pub model_config: ModelConfig,
    /// 所属用户ID, 从主智能体传递
    pub user_id: String,
}

impl Default for LocalSubAgentLoader {
    fn default() -> Self {
        let workspace_root = PathBuf::from(DEFAULT_WORKSPACE_DIR);
        let extensions = SUB_AGENT_ENTRY_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect();
        #[cfg(feature = "rule")]
        let rule_extensions = RULE_ENTRY_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect();
        #[cfg(feature = "command")]
        let command_extensions = COMMAND_ENTRY_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            agents_root: workspace_root.join(DEFAULT_SUB_AGENTS_DIR),
            workspace_root,
            agent_extensions: extensions,
            #[cfg(feature = "rule")]
            rule_extensions,
            #[cfg(feature = "command")]
            command_extensions,
            model_config: ModelConfig::default(),
            user_id: String::new(),
        }
    }
}

impl LocalSubAgentLoader {
    /// 创建本地子智能体加载器
    ///
    /// # Parameters
    ///
    /// * `agents_root` - agents 根目录路径（如 `.codebuddy/agents`）
    pub fn new(agents_root: impl Into<PathBuf>) -> Self {
        let agents_root = agents_root.into();
        Self {
            agents_root,
            ..Default::default()
        }
    }

    /// 设置模型配置
    ///
    /// # Parameters
    /// * `model_config` - 模型配置
    pub fn with_model_config(mut self, model_config: ModelConfig) -> Self {
        self.model_config = model_config;
        self
    }

    /// 设置所属用户ID
    ///
    /// 从主智能体中获取并传递给子智能体。
    ///
    /// # Parameters
    /// * `user_id` - 用户ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = user_id.into();
        self
    }

    /// 设置智能体文件的扩展名列表
    ///
    /// 替换默认的扩展名集合（`.md`）。扩展名不含前缀点，例如 `"md"`、`"markdown"`。
    ///
    /// # Parameters
    ///
    /// * `extensions` - 文件扩展名列表
    ///
    /// # Returns
    ///
    /// 返回 `self` 以支持链式调用
    pub fn with_agent_extensions(mut self, extensions: Vec<String>) -> Self {
        self.agent_extensions = extensions.into_iter().collect();
        self
    }

    /// 设置规则文件的扩展名列表
    ///
    /// 替换默认的规则扩展名列表（`.md`、`.txt`）。扩展名不含前缀点。
    ///
    /// # Parameters
    ///
    /// * `extensions` - 规则文件扩展名列表
    ///
    /// # Returns
    ///
    /// 返回 `self` 以支持链式调用
    #[cfg(feature = "rule")]
    pub fn with_rule_extensions(mut self, extensions: Vec<String>) -> Self {
        self.rule_extensions = extensions;
        self
    }

    /// 设置命令文件的扩展名列表
    ///
    /// 替换默认的命令扩展名列表（`.md`）。扩展名不含前缀点。
    ///
    /// # Parameters
    ///
    /// * `extensions` - 命令文件扩展名列表
    ///
    /// # Returns
    ///
    /// 返回 `self` 以支持链式调用
    #[cfg(feature = "command")]
    pub fn with_command_extensions(mut self, extensions: Vec<String>) -> Self {
        self.command_extensions = extensions;
        self
    }

    /// 切换工作空间根目录
    ///
    /// 将 `agents_root` 重新设置为 `workspace_dir/agents`，
    /// 同时更新 `workspace_root` 以便正确解析名称引用。
    ///
    /// # Parameters
    ///
    /// * `workspace_dir` - 工作空间根目录路径（如 `.codebuddy`）
    pub fn with_workspace_dir(mut self, workspace_dir: impl Into<PathBuf>) -> Self {
        let workspace_dir = workspace_dir.into();
        self.agents_root = workspace_dir.join(DEFAULT_SUB_AGENTS_DIR);
        self.workspace_root = workspace_dir;
        self
    }

    /// 获取 agents 根目录
    pub fn agents_root(&self) -> &Path {
        &self.agents_root
    }

    /// 获取工作空间根目录
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// 扫描本地 agents 根目录并加载所有子智能体
    ///
    /// 加载流程：
    /// 1. 扫描 `agents_root` 下所有匹配扩展名的文件（非递归）
    /// 2. 解析 frontmatter 与 markdown 正文为中间结构
    /// 3. `enabled: false` 的子智能体会被跳过
    /// 4. 通过 [`Self::to_bundle`] 将中间结构转换为 [`AgentBundle`]，
    ///    解析名称引用为实际文件内容
    ///
    /// # Returns
    ///
    /// 加载成功的子智能体列表，按文件名字典序排列以保证确定性
    ///
    /// # Errors
    ///
    /// 当 `agents_root` 存在但读取目录失败时返回 IO 错误；单个文件解析失败时
    /// 跳过并记录 `warn` 级别日志
    pub async fn load(&self) -> Result<Vec<AgentBundle>, Error> {
        if !self.agents_root.exists() {
            return Ok(Vec::new());
        }

        // Warn if model_config or user_id is not configured
        if self.model_config.model.is_empty() || self.model_config.base_url.is_empty() {
            warn!(
                agents_root = %self.agents_root.display(),
                "LocalSubAgentLoader: model_config is not set, sub-agents may not function correctly"
            );
        }
        if self.user_id.is_empty() {
            warn!(
                agents_root = %self.agents_root.display(),
                "LocalSubAgentLoader: user_id is not set, sub-agents will have empty user_id"
            );
        }

        let mut entries = fs::read_dir(&self.agents_root).await?;
        let mut agent_files: Vec<PathBuf> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && self.is_agent_file(&path) {
                agent_files.push(path);
            }
        }

        // Sort for deterministic order
        agent_files.sort();

        let mut bundles: Vec<AgentBundle> = Vec::with_capacity(agent_files.len());
        for file in agent_files {
            match self.load_single_sub_agent(&file).await {
                Ok(Some(bundle)) => {
                    info!(
                        name = %bundle.agent_definition.name,
                        path = %file.display(),
                        "Loaded sub-agent from local file"
                    );
                    bundles.push(bundle);
                }
                Ok(None) => {
                    // Disabled or skipped, already logged
                }
                Err(e) => {
                    warn!(
                        path = %file.display(),
                        error = %e,
                        "Failed to load sub-agent file, skipped"
                    );
                }
            }
        }

        info!(
            agents_root = %self.agents_root.display(),
            total_loaded = bundles.len(),
            "Sub-agent directory scan completed"
        );

        Ok(bundles)
    }

    /// 加载单个子智能体文件并转换为 [`AgentBundle`]
    ///
    /// 当子智能体在 frontmatter 中显式声明 `enabled: false` 时返回 `Ok(None)`。
    async fn load_single_sub_agent(&self, path: &Path) -> Result<Option<AgentBundle>, Error> {
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let raw_content = fs::read_to_string(path).await?;
        let manifest = SubAgentManifest::parse(&raw_content);
        let body = SubAgentManifest::body(&raw_content);

        let name = manifest.name.unwrap_or_else(|| file_stem.clone());
        let description = manifest.description.unwrap_or_default();
        let enabled = manifest.enabled.unwrap_or(true);

        if !enabled {
            debug!(name = %name, path = %path.display(), "Sub-agent is disabled, skipped");
            return Ok(None);
        }

        let local_config = LocalSubAgentConfig {
            name,
            description,
            system_prompt: body.to_string(),
            model: manifest.model,
            tools: SubAgentManifest::parse_csv(&manifest.tools),
            rules: SubAgentManifest::parse_csv(&manifest.rules),
            skills: SubAgentManifest::parse_csv(&manifest.skills),
            mcp_tools: SubAgentManifest::parse_csv(&manifest.mcp_tools),
            commands: SubAgentManifest::parse_csv(&manifest.commands),
            a2a_agents: SubAgentManifest::parse_csv(&manifest.a2a_agents),
            enabled,
            source_hint: Some(path.to_string_lossy().into_owned()),
        };
        Ok(Some(self.to_bundle(&local_config).await))
    }

    /// 将 [`LocalSubAgentConfig`] 转换为 [`AgentBundle`]
    ///
    /// 通过名称引用解析实际文件内容：
    /// - `tools` → 直接转为 `ToolConfig`（仅名称占位符，由运行时工具池匹配）
    /// - `rules` → 从 `workspace_root/rules/` 目录读取对应文件内容
    /// - `skills` → 从 `workspace_root/skills/<name>/SKILL.md` 读取并解析
    /// - `commands` → 从 `workspace_root/commands/` 目录读取对应文件内容
    /// - `mcpTools` → 从 `workspace_root/mcp/mcp.json` 中按名称过滤
    ///
    /// 当某个名称引用无法解析时（文件不存在等），记录 `warn` 日志并跳过该项，
    /// 不会中断整体转换流程。
    /// 本地模型需要进行主动传递模型配置，暂不支持从 model 池中进行获取 model 配置。
    ///
    /// # Parameters
    ///
    /// * `local` - 本地子智能体中间配置
    ///
    /// # Returns
    ///
    /// 完整的 [`AgentBundle`] 装配包
    pub async fn to_bundle(&self, local: &LocalSubAgentConfig) -> AgentBundle {
        // Tools: name-only placeholders (resolved by runtime tool pool)
        let tool_configs: Vec<ToolConfig> = local
            .tools
            .iter()
            .map(|name| ToolConfig::from_name(name))
            .collect();

        // Rules: resolve by reading files from rules directory
        #[cfg(feature = "rule")]
        let rule_configs = self.resolve_rules(&local.rules).await;
        #[cfg(not(feature = "rule"))]
        let rule_configs = Vec::new();

        // Skills: resolve by reading SKILL.md from skill subdirectories
        #[cfg(feature = "skill")]
        let skill_configs = self.resolve_skills(&local.skills).await;
        #[cfg(not(feature = "skill"))]
        let skill_configs = Vec::new();

        // Commands: resolve by reading files from commands directory
        #[cfg(feature = "command")]
        let command_configs = self.resolve_commands(&local.commands).await;
        #[cfg(not(feature = "command"))]
        let command_configs = Vec::new();

        // MCP: resolve by filtering mcp.json entries by name
        #[cfg(feature = "mcp")]
        let mcp_configs = self.resolve_mcp(&local.mcp_tools).await;
        #[cfg(not(feature = "mcp"))]
        let mcp_configs = Vec::new();

        // A2A: resolve by filtering a2a.json entries by name
        #[cfg(feature = "a2a")]
        let a2a_configs = self.resolve_a2a(&local.a2a_agents).await;
        #[cfg(not(feature = "a2a"))]
        let a2a_configs = Vec::new();

        // Apply per-agent model override if declared in frontmatter
        let model_config = if let Some(ref model) = local.model {
            ModelConfig {
                model: model.clone(),
                ..self.model_config.clone()
            }
        } else {
            self.model_config.clone()
        };

        AgentBundle {
            agent_definition: AgentDefinition {
                id: 0,
                user_id: self.user_id.clone(),
                name: local.name.clone(),
                description: local.description.clone(),
                system_prompt: local.system_prompt.clone(),
                welcome_message: None,
                max_delegation_depth: 0, // Will be overridden in build_runtime_configs
            },
            model_config,
            tool_configs,
            rule_configs,
            skill_configs,
            command_configs,
            mcp_configs,
            sub_agent_bundles: Vec::new(),
            a2a_configs,
        }
    }

    // ─── Name Reference Resolution ─────────────────────────────────────────

    /// 解析规则名称引用列表为 [`RuleConfig`] 列表
    ///
    /// 复用 [`LocalRuleLoader::load_single_rule`] 从 `workspace_root/rules/` 目录
    /// 读取匹配的文件（支持 `.md` 和 `.txt` 扩展名）。
    #[cfg(feature = "rule")]
    async fn resolve_rules(&self, names: &[String]) -> Vec<RuleConfig> {
        if names.is_empty() {
            return Vec::new();
        }

        let rules_dir = self.workspace_root.join(DEFAULT_RULES_DIR);
        let mut configs = Vec::with_capacity(names.len());

        for name in names {
            for ext in &self.rule_extensions {
                let file_path = rules_dir.join(format!("{name}.{ext}"));
                if !file_path.exists() {
                    warn!(
                        name = %name,
                        rules_dir = %rules_dir.display(),
                        "Rule name reference could not be resolved, skipped"
                    );
                    continue;
                }

                match LocalRuleLoader::load_single_rule(&file_path).await {
                    Ok(config) => {
                        configs.push(config);
                        break;
                    }
                    Err(e) => {
                        warn!(
                            name = %name,
                            path = %file_path.display(),
                            error = %e,
                            "Failed to load rule file"
                        );
                        break;
                    }
                }
            }
        }

        configs
    }

    /// 解析技能名称引用列表为 [`SkillConfig`] 列表
    ///
    /// 复用 [`LocalSkillLoader::scan_skill_at`] 从 `workspace_root/skills/<name>/`
    /// 目录读取并解析 `SKILL.md`。
    #[cfg(feature = "skill")]
    async fn resolve_skills(&self, names: &[String]) -> Vec<SkillConfig> {
        if names.is_empty() {
            return Vec::new();
        }

        let skills_dir = self.workspace_root.join(DEFAULT_SKILLS_DIR);
        let skill_loader = LocalSkillLoader::new(&skills_dir);
        let mut configs = Vec::with_capacity(names.len());

        for name in names {
            let skill_dir = skills_dir.join(name);
            match skill_loader.scan_skill_at(&skill_dir).await {
                Ok(Some(config)) => configs.push(config),
                Ok(None) => {
                    warn!(
                        name = %name,
                        skills_dir = %skills_dir.display(),
                        "Skill name reference could not be resolved, skipped"
                    );
                }
                Err(e) => {
                    warn!(
                        name = %name,
                        skills_dir = %skills_dir.display(),
                        error = %e,
                        "Failed to load skill"
                    );
                }
            }
        }

        configs
    }

    /// 解析命令名称引用列表为 [`CommandConfig`] 列表
    ///
    /// 复用 [`LocalCommandLoader::load_single_command`] 从 `workspace_root/commands/`
    /// 目录读取匹配的文件。
    #[cfg(feature = "command")]
    async fn resolve_commands(&self, names: &[String]) -> Vec<CommandConfig> {
        if names.is_empty() {
            return Vec::new();
        }

        let commands_dir = self.workspace_root.join(DEFAULT_COMMANDS_DIR);
        let mut configs = Vec::with_capacity(names.len());

        for name in names {
            for ext in &self.command_extensions {
                let file_path = commands_dir.join(format!("{name}.{ext}"));
                if !file_path.exists() {
                    warn!(
                        name = %name,
                        commands_dir = %commands_dir.display(),
                        "Command name reference could not be resolved, skipped"
                    );
                    continue;
                }

                match LocalCommandLoader::load_single_command(&file_path).await {
                    Ok(config) => {
                        configs.push(config);
                        break;
                    }
                    Err(e) => {
                        warn!(
                            name = %name,
                            path = %file_path.display(),
                            error = %e,
                            "Failed to load command file"
                        );
                        break;
                    }
                }
            }
        }

        configs
    }

    /// 解析 MCP 服务名称引用列表为 `McpServerConfig` 列表
    ///
    /// 从 `workspace_root/mcp/mcp.json` 中加载所有 MCP 配置，
    /// 然后按名称过滤出匹配的服务。
    #[cfg(feature = "mcp")]
    async fn resolve_mcp(&self, names: &[String]) -> Vec<McpServerConfig> {
        if names.is_empty() {
            return Vec::new();
        }

        let mcp_config_path = self
            .workspace_root
            .join(DEFAULT_MCP_CONFIG_DIR)
            .join(DEFAULT_MCP_CONFIG_FILE);

        let loader = LocalMcpLoader::new(&mcp_config_path);
        let all_configs = match loader.load().await {
            Ok(configs) => configs,
            Err(e) => {
                warn!(
                    path = %mcp_config_path.display(),
                    error = %e,
                    "Failed to load MCP config for sub-agent name resolution"
                );
                return Vec::new();
            }
        };

        let name_set: HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
        all_configs
            .into_iter()
            .filter(|config| name_set.contains(config.service_name.as_str()))
            .collect()
    }

    /// 解析 A2A 远程 Agent 服务名称引用列表为 [`A2aServiceConfig`] 列表
    ///
    /// 从 `workspace_root/a2a/a2a.json` 中加载所有 A2A 配置，
    /// 然后按名称过滤出匹配的服务。
    #[cfg(feature = "a2a")]
    async fn resolve_a2a(&self, names: &[String]) -> Vec<A2aServiceConfig> {
        if names.is_empty() {
            return Vec::new();
        }

        let a2a_config_path = self
            .workspace_root
            .join(DEFAULT_A2A_CONFIG_DIR)
            .join(DEFAULT_A2A_CONFIG_FILE);

        let loader = LocalA2aLoader::new(&a2a_config_path);
        let all_configs = match loader.load().await {
            Ok(configs) => configs,
            Err(e) => {
                warn!(
                    path = %a2a_config_path.display(),
                    error = %e,
                    "Failed to load A2A config for sub-agent name resolution"
                );
                return Vec::new();
            }
        };

        let name_set: HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
        all_configs
            .into_iter()
            .filter(|cfg| name_set.contains(cfg.service_name.as_str()))
            .collect()
    }

    // ─── Helpers ────────────────────────────────────────────────────────────

    /// 判断文件是否为支持的子智能体文件格式
    fn is_agent_file(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| self.agent_extensions.contains(ext))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::TempDir;

    /// Create a workspace structure with agents, rules, skills, commands, mcp, a2a dirs
    fn create_workspace(tmp: &Path) -> PathBuf {
        let workspace = tmp.join(".codebuddy");
        std_fs::create_dir_all(workspace.join("agents")).unwrap();
        std_fs::create_dir_all(workspace.join("rules")).unwrap();
        std_fs::create_dir_all(workspace.join("skills")).unwrap();
        std_fs::create_dir_all(workspace.join("commands")).unwrap();
        std_fs::create_dir_all(workspace.join("mcp")).unwrap();
        std_fs::create_dir_all(workspace.join("a2a")).unwrap();
        workspace
    }

    fn create_agent_file(root: &Path, name: &str, content: &str) {
        std_fs::write(root.join(name), content).unwrap();
    }

    #[tokio::test]
    async fn test_load_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let loader = LocalSubAgentLoader::new(tmp.path());
        let configs = loader.load().await.unwrap();
        assert!(configs.is_empty());
    }

    #[tokio::test]
    async fn test_load_nonexistent_directory_returns_empty() {
        let loader = LocalSubAgentLoader::new("/nonexistent/path/for/sub-agent/loader/test");
        let configs = loader.load().await.unwrap();
        assert!(configs.is_empty());
    }

    #[tokio::test]
    async fn test_load_full_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let workspace = create_workspace(tmp.path());
        let agents_dir = workspace.join("agents");

        let content = r#"---
name: 测试3
description: 测试3
tools: list_files, search_file, search_content, read_file
enabled: true
---
测试3
"#;
        create_agent_file(&agents_dir, "测试3.md", content);

        let loader = LocalSubAgentLoader::new(&agents_dir);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let cfg = &configs[0];
        assert_eq!(cfg.agent_definition.name, "测试3");
        assert_eq!(cfg.agent_definition.description, "测试3");
        let tool_names: Vec<&str> = cfg.tool_configs.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            tool_names,
            vec!["list_files", "search_file", "search_content", "read_file"]
        );
        assert_eq!(cfg.agent_definition.system_prompt, "测试3");
    }

    #[tokio::test]
    async fn test_load_with_model_and_mcp() {
        let tmp = TempDir::new().unwrap();
        let workspace = create_workspace(tmp.path());
        let agents_dir = workspace.join("agents");

        // Create MCP config
        let mcp_json = r#"{"mcpServers": {"MCP server w/ Browser Use": {"url": "http://localhost:8080", "transportType": "sse"}}}"#;
        std_fs::write(workspace.join("mcp/mcp.json"), mcp_json).unwrap();

        let content = r#"---
name: 测试12
description: 测试12
model: glm-4.7
tools: list_files, search_file
mcpTools: MCP server w/ Browser Use
enabled: true
---
测试12
"#;
        create_agent_file(&agents_dir, "测试12.md", content);

        let loader = LocalSubAgentLoader::new(&agents_dir).with_workspace_dir(&workspace);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let cfg = &configs[0];
        assert_eq!(cfg.agent_definition.name, "测试12");
        assert_eq!(cfg.model_config.model, "glm-4.7");
        let mcp_names: Vec<&str> = cfg
            .mcp_configs
            .iter()
            .map(|m| m.service_name.as_str())
            .collect();
        assert_eq!(mcp_names, vec!["MCP server w/ Browser Use"]);
    }

    #[cfg(feature = "a2a")]
    #[tokio::test]
    async fn test_resolve_a2a_from_workspace() {
        let tmp = TempDir::new().unwrap();
        let workspace = create_workspace(tmp.path());
        let agents_dir = workspace.join("agents");

        // Create A2A config
        let a2a_json = r#"{
            "a2aServers": {
                "weather_agent": {
                    "base_url": "http://weather-agent:3000",
                    "description": "Query weather information",
                    "timeout_secs": 30
                },
                "math_agent": {
                    "base_url": "http://math-agent:3000"
                }
            }
        }"#;
        std_fs::write(workspace.join("a2a/a2a.json"), a2a_json).unwrap();

        let content = r#"---
name: test-agent
a2aAgents: weather_agent
enabled: true
---
Agent prompt
"#;
        create_agent_file(&agents_dir, "test-agent.md", content);

        let loader = LocalSubAgentLoader::new(&agents_dir).with_workspace_dir(&workspace);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let cfg = &configs[0];
        assert_eq!(cfg.a2a_configs.len(), 1);
        assert_eq!(cfg.a2a_configs[0].service_name, "weather_agent");
        assert_eq!(cfg.a2a_configs[0].base_url, "http://weather-agent:3000");
        assert_eq!(cfg.a2a_configs[0].description, "Query weather information");
        assert_eq!(cfg.a2a_configs[0].timeout_secs, Some(30));
    }

    #[tokio::test]
    async fn test_load_no_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let agents_dir = tmp.path().to_path_buf();
        create_agent_file(&agents_dir, "plain-agent.md", "Just a plain agent prompt");

        let loader = LocalSubAgentLoader::new(&agents_dir);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let cfg = &configs[0];
        assert_eq!(cfg.agent_definition.name, "plain-agent");
        assert!(cfg.tool_configs.is_empty());
        assert_eq!(
            cfg.agent_definition.system_prompt,
            "Just a plain agent prompt"
        );
    }

    #[tokio::test]
    async fn test_load_disabled_agent_skipped() {
        let tmp = TempDir::new().unwrap();
        create_agent_file(
            tmp.path(),
            "disabled.md",
            "---\nname: disabled-agent\nenabled: false\n---\nPrompt",
        );

        let loader = LocalSubAgentLoader::new(tmp.path());
        let configs = loader.load().await.unwrap();
        assert!(configs.is_empty());
    }

    #[tokio::test]
    async fn test_load_skips_non_md_files() {
        let tmp = TempDir::new().unwrap();
        create_agent_file(tmp.path(), "agent.md", "---\nname: valid\n---\nPrompt");
        create_agent_file(tmp.path(), "ignore.json", "{}");
        create_agent_file(tmp.path(), "ignore.rs", "fn main() {}");

        let loader = LocalSubAgentLoader::new(tmp.path());
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].agent_definition.name, "valid");
    }

    #[tokio::test]
    async fn test_deterministic_load_order() {
        let tmp = TempDir::new().unwrap();
        create_agent_file(tmp.path(), "c-agent.md", "---\nname: C\n---\nC");
        create_agent_file(tmp.path(), "a-agent.md", "---\nname: A\n---\nA");
        create_agent_file(tmp.path(), "b-agent.md", "---\nname: B\n---\nB");

        let loader = LocalSubAgentLoader::new(tmp.path());
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 3);
        assert_eq!(configs[0].agent_definition.name, "A");
        assert_eq!(configs[1].agent_definition.name, "B");
        assert_eq!(configs[2].agent_definition.name, "C");
    }

    #[test]
    fn test_with_workspace_dir() {
        let loader = LocalSubAgentLoader::default().with_workspace_dir("/tmp/my-workspace");
        assert_eq!(loader.agents_root(), Path::new("/tmp/my-workspace/agents"));
        assert_eq!(loader.workspace_root(), Path::new("/tmp/my-workspace"));
    }

    #[cfg(feature = "rule")]
    #[tokio::test]
    async fn test_resolve_rules_from_workspace() {
        let tmp = TempDir::new().unwrap();
        let workspace = create_workspace(tmp.path());
        let agents_dir = workspace.join("agents");

        // Create rule files
        std_fs::write(workspace.join("rules/security.md"), "Security rule content").unwrap();
        std_fs::write(workspace.join("rules/naming.txt"), "Naming rule content").unwrap();

        let content = r#"---
name: test-agent
rules: security, naming
enabled: true
---
Agent prompt
"#;
        create_agent_file(&agents_dir, "test-agent.md", content);

        let loader = LocalSubAgentLoader::new(&agents_dir).with_workspace_dir(&workspace);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let cfg = &configs[0];
        assert_eq!(cfg.rule_configs.len(), 2);
        assert_eq!(cfg.rule_configs[0].name, "security");
        assert_eq!(cfg.rule_configs[0].content, "Security rule content");
        assert_eq!(cfg.rule_configs[1].name, "naming");
        assert_eq!(cfg.rule_configs[1].content, "Naming rule content");
    }

    #[cfg(feature = "skill")]
    #[tokio::test]
    async fn test_resolve_skills_from_workspace() {
        let tmp = TempDir::new().unwrap();
        let workspace = create_workspace(tmp.path());
        let agents_dir = workspace.join("agents");

        // Create skill directory with SKILL.md
        let skill_dir = workspace.join("skills/code-review");
        std_fs::create_dir_all(&skill_dir).unwrap();
        std_fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Code Review\ndescription: Reviews code quality\n---\nSkill body",
        )
        .unwrap();

        let content = r#"---
name: test-agent
skills: code-review
enabled: true
---
Agent prompt
"#;
        create_agent_file(&agents_dir, "test-agent.md", content);

        let loader = LocalSubAgentLoader::new(&agents_dir).with_workspace_dir(&workspace);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let cfg = &configs[0];
        assert_eq!(cfg.skill_configs.len(), 1);
        assert_eq!(cfg.skill_configs[0].skill_code, "code-review");
        assert_eq!(cfg.skill_configs[0].name, "Code Review");
        assert_eq!(cfg.skill_configs[0].description, "Reviews code quality");
    }

    #[cfg(feature = "command")]
    #[tokio::test]
    async fn test_resolve_commands_from_workspace() {
        let tmp = TempDir::new().unwrap();
        let workspace = create_workspace(tmp.path());
        let agents_dir = workspace.join("agents");

        // Create command file
        std_fs::write(workspace.join("commands/build.md"), "Run cargo build").unwrap();

        let content = r#"---
name: test-agent
commands: build
enabled: true
---
Agent prompt
"#;
        create_agent_file(&agents_dir, "test-agent.md", content);

        let loader = LocalSubAgentLoader::new(&agents_dir).with_workspace_dir(&workspace);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let cfg = &configs[0];
        assert_eq!(cfg.command_configs.len(), 1);
        assert_eq!(cfg.command_configs[0].name, "build");
        assert_eq!(cfg.command_configs[0].content, "Run cargo build");
    }

    #[cfg(all(feature = "rule", feature = "skill", feature = "command"))]
    #[tokio::test]
    async fn test_unresolvable_references_skipped() {
        let tmp = TempDir::new().unwrap();
        let workspace = create_workspace(tmp.path());
        let agents_dir = workspace.join("agents");

        // Only create one rule, leave others unresolvable
        std_fs::write(workspace.join("rules/existing.md"), "Existing rule").unwrap();

        let content = r#"---
name: test-agent
rules: existing, nonexistent
skills: missing-skill
commands: missing-cmd
enabled: true
---
Agent prompt
"#;
        create_agent_file(&agents_dir, "test-agent.md", content);

        let loader = LocalSubAgentLoader::new(&agents_dir).with_workspace_dir(&workspace);
        let configs = loader.load().await.unwrap();

        assert_eq!(configs.len(), 1);
        let cfg = &configs[0];
        // Only the existing rule should be resolved
        assert_eq!(cfg.rule_configs.len(), 1);
        assert_eq!(cfg.rule_configs[0].name, "existing");
        // Missing references are skipped
        assert!(cfg.skill_configs.is_empty());
        assert!(cfg.command_configs.is_empty());
    }
}
