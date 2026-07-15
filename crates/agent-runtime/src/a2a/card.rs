//! AgentBundle → A2A AgentCard 自动生成
//!
//! 提供从 [`AgentBundle`] 自动派生 A2A v1.0 [`AgentCard`] 的纯函数，
//! 无需开发者手动填写协议字段。
//!
//! 提供的主要内容：
//! - [`agent_card_from_bundle`] - 从 bundle 自动生成 AgentCard（推荐）
//! - [`AgentCardBuilder`] - 链式构造器，用于自定义 AgentCard 字段

use a2a_rs_core::{
    AgentCapabilities, AgentCard, AgentInterface, AgentProvider, AgentSkill, PROTOCOL_VERSION,
};

use crate::types::runtime::AgentBundle;

/// 从 [`AgentBundle`] 自动生成 A2A v1.0 [`AgentCard`]
///
/// 自动提取 bundle 中的元数据（名称、描述、工具、技能、子 Agent）
/// 生成符合 A2A v1.0 规范的 AgentCard。
///
/// # Parameters
///
/// * `bundle` - 智能体装配包（只读引用）
/// * `base_url` - 服务的公开基础 URL（例如 `"http://0.0.0.0:3000"`）
///
/// # Returns
///
/// 符合 A2A v1.0 规范的 [`AgentCard`]
///
/// # Examples
///
/// ```rust,no_run
/// use agent_runtime::a2a::card::agent_card_from_bundle;
/// use agent_runtime::types::runtime::AgentBundle;
///
/// let bundle = AgentBundle::default();
/// let card = agent_card_from_bundle(&bundle, "http://localhost:3000");
/// assert_eq!(card.version, "1.0");
/// ```
pub fn agent_card_from_bundle(bundle: &AgentBundle, base_url: &str) -> AgentCard {
    AgentCard {
        name: bundle.agent_definition.name.clone(),
        description: bundle.agent_definition.description.clone(),
        version: PROTOCOL_VERSION.to_string(),
        supported_interfaces: vec![AgentInterface {
            // 服务端启动后会用实际绑定地址填充；此处留空由 a2a-rs-server 自动填充
            url: base_url.to_string(),
            protocol_binding: "JSONRPC".to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            tenant: None,
        }],
        provider: None,
        documentation_url: None,
        capabilities: AgentCapabilities {
            streaming: Some(true),
            push_notifications: Some(false),
            extended_agent_card: Some(false),
            ..Default::default()
        },
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
        skills: build_skills_from_bundle(bundle),
        ..Default::default()
    }
}

/// 从 bundle 中提取技能列表
///
/// 合并以下来源：
/// - `tool_configs` → 每个工具映射为一个 Skill
/// - `skill_configs` → 每个技能映射为一个 Skill
/// - `sub_agent_bundles` → 每个子 Agent 映射为一个 Skill（带 `sub-agent` tag）
fn build_skills_from_bundle(bundle: &AgentBundle) -> Vec<AgentSkill> {
    let mut skills = Vec::new();

    // 工具 → Skill
    for tool in &bundle.tool_configs {
        skills.push(AgentSkill {
            id: tool.name.clone(),
            name: tool.name.clone(),
            description: tool.description.clone(),
            tags: vec!["tool".to_string()],
            examples: vec![],
            ..Default::default()
        });
    }

    // 技能 → Skill
    for skill in &bundle.skill_configs {
        skills.push(AgentSkill {
            id: skill.skill_code.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
            tags: vec!["skill".to_string()],
            examples: vec![],
            ..Default::default()
        });
    }

    // 子 Agent → Skill
    for sub in &bundle.sub_agent_bundles {
        skills.push(AgentSkill {
            id: sub.agent_definition.name.clone(),
            name: sub.agent_definition.name.clone(),
            description: sub.agent_definition.description.clone(),
            tags: vec!["sub-agent".to_string()],
            examples: vec![],
            ..Default::default()
        });
    }

    skills
}

/// AgentCard 链式构造器
///
/// 在 [`agent_card_from_bundle`] 的基础上提供自定义能力，
/// 适用于需要覆盖默认字段（如 provider、icon、额外 skill）的场景。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_runtime::a2a::card::AgentCardBuilder;
/// use agent_runtime::types::runtime::AgentBundle;
///
/// let bundle = AgentBundle::default();
/// let card = AgentCardBuilder::from_bundle(&bundle, "http://localhost:3000")
///     .with_provider("MyTeam", "https://myteam.example.com")
///     .with_extra_skill("code_review", "Code Review", "Reviews Rust code", &["rust"])
///     .build();
/// ```
pub struct AgentCardBuilder {
    card: AgentCard,
}

impl AgentCardBuilder {
    /// 从 bundle 初始化构造器（等同于 [`agent_card_from_bundle`]）
    ///
    /// # Parameters
    ///
    /// * `bundle` - 智能体装配包
    /// * `base_url` - 服务的公开基础 URL
    pub fn from_bundle(bundle: &AgentBundle, base_url: &str) -> Self {
        Self {
            card: agent_card_from_bundle(bundle, base_url),
        }
    }

    /// 设置 provider 信息
    ///
    /// # Parameters
    ///
    /// * `org` - 组织名称
    /// * `url` - 组织 URL
    pub fn with_provider(mut self, org: impl Into<String>, url: impl Into<String>) -> Self {
        self.card.provider = Some(AgentProvider {
            organization: org.into(),
            url: url.into(),
        });
        self
    }

    /// 设置文档 URL
    ///
    /// # Parameters
    ///
    /// * `url` - 文档 URL
    pub fn with_documentation_url(mut self, url: impl Into<String>) -> Self {
        self.card.documentation_url = Some(url.into());
        self
    }

    /// 追加一个额外的 Skill
    ///
    /// # Parameters
    ///
    /// * `id` - Skill 唯一 ID
    /// * `name` - Skill 显示名称
    /// * `description` - Skill 描述
    /// * `tags` - 标签列表
    pub fn with_extra_skill(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        tags: &[&str],
    ) -> Self {
        self.card.skills.push(AgentSkill {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            examples: vec![],
            ..Default::default()
        });
        self
    }

    /// 构建最终的 [`AgentCard`]
    pub fn build(self) -> AgentCard {
        self.card
    }
}

// ─── 单元测试 ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        runtime::{AgentBundle, AgentDefinition},
        skill::SkillConfig,
        tool::ToolConfig,
    };

    fn make_bundle() -> AgentBundle {
        AgentBundle {
            agent_definition: AgentDefinition {
                name: "test-agent".to_string(),
                description: "A test agent".to_string(),
                ..Default::default()
            },
            tool_configs: vec![ToolConfig {
                name: "search".to_string(),
                description: "Search the web".to_string(),
                ..Default::default()
            }],
            skill_configs: vec![SkillConfig {
                skill_code: "code_review".to_string(),
                name: "Code Review".to_string(),
                description: "Reviews code".to_string(),
                source_hint: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn agent_card_from_bundle_fills_name_and_description() {
        let bundle = make_bundle();
        let card = agent_card_from_bundle(&bundle, "http://localhost:3000");

        assert_eq!(card.name, "test-agent");
        assert_eq!(card.description, "A test agent");
        assert_eq!(card.version, PROTOCOL_VERSION);
    }

    #[test]
    fn agent_card_from_bundle_generates_skills_from_tools_and_skills() {
        let bundle = make_bundle();
        let card = agent_card_from_bundle(&bundle, "http://localhost:3000");

        // 1 tool + 1 skill = 2 skills
        assert_eq!(card.skills.len(), 2);

        let tool_skill = card.skills.iter().find(|s| s.id == "search").unwrap();
        assert!(tool_skill.tags.contains(&"tool".to_string()));

        let skill_skill = card.skills.iter().find(|s| s.id == "code_review").unwrap();
        assert!(skill_skill.tags.contains(&"skill".to_string()));
    }

    #[test]
    fn agent_card_from_bundle_streaming_capability_is_true() {
        let bundle = make_bundle();
        let card = agent_card_from_bundle(&bundle, "http://localhost:3000");
        assert_eq!(card.capabilities.streaming, Some(true));
    }

    #[test]
    fn agent_card_builder_with_provider() {
        let bundle = make_bundle();
        let card = AgentCardBuilder::from_bundle(&bundle, "http://localhost:3000")
            .with_provider("MyTeam", "https://myteam.example.com")
            .build();

        let provider = card.provider.unwrap();
        assert_eq!(provider.organization, "MyTeam");
        assert_eq!(provider.url, "https://myteam.example.com");
    }

    #[test]
    fn agent_card_builder_with_extra_skill() {
        let bundle = make_bundle();
        let card = AgentCardBuilder::from_bundle(&bundle, "http://localhost:3000")
            .with_extra_skill("extra", "Extra Skill", "An extra skill", &["custom"])
            .build();

        // 原有 2 个 + 新增 1 个 = 3 个
        assert_eq!(card.skills.len(), 3);
        let extra = card.skills.iter().find(|s| s.id == "extra").unwrap();
        assert_eq!(extra.name, "Extra Skill");
        assert!(extra.tags.contains(&"custom".to_string()));
    }
}
