//! Sub-agent types
//!
//! Defines the configuration types for sub-agents, supporting both local
//! filesystem loading (from `.codebuddy/agents/`) and runtime injection
//! (from database or API).
//!
//! ## Capability declaration model
//!
//! Sub-agent capabilities are declared via [`SubAgentCapabilities`], an enum
//! with two variants:
//!
//! | Variant | Source | Content | `build_child_runtime` behavior |
//! |---------|--------|---------|-------------------------------|
//! | `Local` | `.codebuddy/agents/*.md` | Name whitelists | Filter parent's capability pool |
//! | `Full`  | Database / API | Complete config objects | Pass directly to Builder |
//!
//! Provided types:
//! - [`SubAgentConfig`] - Sub-agent configuration (common fields + capabilities)
//! - [`SubAgentCapabilities`] - Capability declaration (Local whitelists or Full configs)
//! - [`LocalCapabilities`] - Name-based whitelists for local sub-agents
//! - [`FullCapabilities`] - Complete config objects for online sub-agents
//! - [`SubAgentSource`] - Sub-agent loading source
//! - [`AgentMode`] - Sub-agent execution mode

use serde::{Deserialize, Serialize};

/// Sub-agent configuration
///
/// Describes a sub-agent that can be delegated to by the parent agent.
/// Each sub-agent will be built into a complete `Agent` with its own
/// tools, rules, skills, MCP services, and commands.
///
/// ## Local file format
///
/// Local sub-agents are loaded from `.codebuddy/agents/<name>.md` files.
/// The file uses YAML frontmatter to declare capabilities:
///
/// ```markdown
/// ---
/// name: Code Reviewer
/// description: Reviews code for quality and best practices
/// model: gpt-4o
/// tools: read_file, search_content, list_files
/// rules: security-rule, code-style
/// skills: code-review
/// mcpTools: MCP server w/ Browser Use
/// commands: build, deploy
/// agentMode: agentic
/// enabled: true
/// enabledAutoRun: true
/// ---
/// You are a code review expert...
/// ```
///
/// ## Online format
///
/// Online sub-agents are loaded from the database (`t_agent_sub_agent_rel`)
/// and share the same field structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSubAgentConfig {
    /// Sub-agent name (from frontmatter `name` or file stem)
    pub name: String,
    /// Sub-agent description (from frontmatter `description`)
    pub description: String,
    /// System prompt (from the markdown body after frontmatter)
    pub system_prompt: String,
    /// Model override (from frontmatter `model`; None = inherit parent model)
    pub model: Option<String>,
    /// Tool whitelist (from frontmatter `tools`, comma-separated)
    pub tools: Vec<String>,
    /// Rule whitelist (from frontmatter `rules`, comma-separated)
    pub rules: Vec<String>,
    /// Skill whitelist (from frontmatter `skills`, comma-separated)
    pub skills: Vec<String>,
    /// MCP service whitelist (from frontmatter `mcpTools`, comma-separated)
    pub mcp_tools: Vec<String>,
    /// Command whitelist (from frontmatter `commands`, comma-separated)
    pub commands: Vec<String>,
    /// A2A remote agent service whitelist (from frontmatter `a2aAgents`, comma-separated)
    pub a2a_agents: Vec<String>,
    /// Whether this sub-agent is enabled (from frontmatter `enabled`)
    pub enabled: bool,
    /// Source hint (file path or database identifier)
    pub source_hint: Option<String>,
}

impl Default for LocalSubAgentConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            model: None,
            tools: Vec::new(),
            rules: Vec::new(),
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            commands: Vec::new(),
            a2a_agents: Vec::new(),
            enabled: true,
            system_prompt: String::new(),
            source_hint: None,
        }
    }
}
