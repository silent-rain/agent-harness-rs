//! Sub-agent manifest (frontmatter) parser
//!
//! Parses YAML frontmatter from sub-agent `.md` files.
//! The frontmatter declares the sub-agent's capabilities (tools, rules, skills, etc.)
//! and the body after frontmatter serves as the system prompt.
//!
//! Provided types:
//! - [`SubAgentManifest`] - Parsed frontmatter structure
//!
//! ## File format
//!
//! ```markdown
//! ---
//! name: Code Reviewer
//! description: Reviews code for quality and best practices
//! model: gpt-4o
//! tools: read_file, search_content, list_files
//! rules: security-rule, code-style
//! skills: code-review
//! mcpTools: MCP server w/ Browser Use
//! a2aAgents: weather_agent, math_agent
//! commands: build, deploy
//! agentMode: agentic
//! enabled: true
//! enabledAutoRun: true
//! ---
//! You are a code review expert...
//! ```

use serde::Deserialize;

/// Sub-agent manifest parsed from YAML frontmatter
///
/// All fields are optional; missing fields use sensible defaults.
/// The `tools`, `rules`, `skills`, `mcpTools`, and `commands` fields accept
/// comma-separated strings that are parsed into `Vec<String>`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentManifest {
    /// Sub-agent display name
    pub name: Option<String>,
    /// Sub-agent description
    pub description: Option<String>,
    /// Model override (None = inherit parent model)
    pub model: Option<String>,
    /// Tool whitelist (comma-separated string)
    pub tools: Option<String>,
    /// Rule whitelist (comma-separated string)
    pub rules: Option<String>,
    /// Skill whitelist (comma-separated string)
    pub skills: Option<String>,
    /// MCP service whitelist (comma-separated string)
    pub mcp_tools: Option<String>,
    /// Command whitelist (comma-separated string)
    pub commands: Option<String>,
    /// A2A remote agent service whitelist (comma-separated string)
    pub a2a_agents: Option<String>,
    /// Execution mode (raw string; reserved for future typed parsing)
    pub agent_mode: Option<String>,
    /// Whether this sub-agent is enabled
    pub enabled: Option<bool>,
    /// Whether auto-run is enabled
    pub enabled_auto_run: Option<bool>,
}

impl SubAgentManifest {
    /// Parse frontmatter from raw markdown content
    ///
    /// Extracts the YAML block between `---` delimiters at the start of the file.
    /// Returns `Default` if no valid frontmatter is found.
    ///
    /// # Parameters
    ///
    /// * `raw` - Raw markdown content
    ///
    /// # Returns
    ///
    /// Parsed manifest (with defaults for missing fields)
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim_start();
        if !trimmed.starts_with("---") {
            return Self::default();
        }

        // Find the closing `---`
        let after_open = &trimmed[3..];
        let close_pos = after_open.find("\n---");
        match close_pos {
            Some(pos) => {
                let yaml_block = &after_open[..pos];
                parse_yaml_frontmatter(yaml_block).unwrap_or_default()
            }
            None => Self::default(),
        }
    }

    /// Extract the body content after frontmatter
    ///
    /// Returns the content after the closing `---` delimiter.
    /// If no frontmatter is found, returns the entire content.
    ///
    /// # Parameters
    ///
    /// * `raw` - Raw markdown content
    ///
    /// # Returns
    ///
    /// Body content (trimmed)
    pub fn body(raw: &str) -> &str {
        let trimmed = raw.trim_start();
        if !trimmed.starts_with("---") {
            return raw.trim();
        }

        let after_open = &trimmed[3..];
        match after_open.find("\n---") {
            Some(pos) => {
                let after_close = &after_open[pos + 4..]; // skip "\n---"
                after_close.trim()
            }
            None => raw.trim(),
        }
    }

    /// Parse a comma-separated string into a trimmed `Vec<String>`
    ///
    /// Filters out empty entries after trimming.
    ///
    /// # Parameters
    ///
    /// * `input` - Optional comma-separated string
    ///
    /// # Returns
    ///
    /// Parsed list of trimmed, non-empty strings
    pub fn parse_csv(input: &Option<String>) -> Vec<String> {
        input
            .as_deref()
            .map(|s| {
                s.split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Try to parse YAML frontmatter using the `config` crate, returning `None` on failure
fn parse_yaml_frontmatter(yaml_str: &str) -> Option<SubAgentManifest> {
    let cfg = config::Config::builder()
        .add_source(config::File::from_str(yaml_str, config::FileFormat::Yaml))
        .build()
        .ok()?;

    match cfg.try_deserialize::<SubAgentManifest>() {
        Ok(manifest) => Some(manifest),
        Err(e) => {
            tracing::debug!(error = %e, "Failed to parse sub-agent frontmatter as YAML");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_frontmatter() {
        let raw = r#"---
name: 测试3
description: 测试3
tools: list_files, search_file, search_content, read_file
rules: security-rule, code-style
skills: code-review
mcpTools: MCP server w/ Browser Use
commands: build, deploy
agentMode: agentic
enabled: true
enabledAutoRun: true
---
测试3
"#;
        let manifest = SubAgentManifest::parse(raw);
        assert_eq!(manifest.name.as_deref(), Some("测试3"));
        assert_eq!(manifest.description.as_deref(), Some("测试3"));
        assert_eq!(
            manifest.tools.as_deref(),
            Some("list_files, search_file, search_content, read_file")
        );
        assert_eq!(manifest.rules.as_deref(), Some("security-rule, code-style"));
        assert_eq!(manifest.skills.as_deref(), Some("code-review"));
        assert_eq!(
            manifest.mcp_tools.as_deref(),
            Some("MCP server w/ Browser Use")
        );
        assert_eq!(manifest.commands.as_deref(), Some("build, deploy"));
        assert_eq!(manifest.agent_mode.as_deref(), Some("agentic"));
        assert_eq!(manifest.enabled, Some(true));
        assert_eq!(manifest.enabled_auto_run, Some(true));
    }

    #[test]
    fn test_parse_with_model() {
        let raw = r#"---
name: 测试12
description: 测试12
model: glm-4.7
tools: list_files, search_file
agentMode: agentic
enabled: true
---
测试12
"#;
        let manifest = SubAgentManifest::parse(raw);
        assert_eq!(manifest.name.as_deref(), Some("测试12"));
        assert_eq!(manifest.model.as_deref(), Some("glm-4.7"));
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let raw = "Just plain text content";
        let manifest = SubAgentManifest::parse(raw);
        assert!(manifest.name.is_none());
        assert!(manifest.tools.is_none());
    }

    #[test]
    fn test_body_with_frontmatter() {
        let raw = "---\nname: test\n---\nThis is the body";
        let body = SubAgentManifest::body(raw);
        assert_eq!(body, "This is the body");
    }

    #[test]
    fn test_body_without_frontmatter() {
        let raw = "Just plain text content";
        let body = SubAgentManifest::body(raw);
        assert_eq!(body, "Just plain text content");
    }

    #[test]
    fn test_parse_csv() {
        let input = Some("list_files, search_file, read_file".to_string());
        let result = SubAgentManifest::parse_csv(&input);
        assert_eq!(result, vec!["list_files", "search_file", "read_file"]);
    }

    #[test]
    fn test_parse_csv_empty() {
        let result = SubAgentManifest::parse_csv(&None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_csv_with_spaces() {
        let input = Some(" a , b , c ".to_string());
        let result = SubAgentManifest::parse_csv(&input);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_csv_single_item() {
        let input = Some("MCP server w/ Browser Use".to_string());
        let result = SubAgentManifest::parse_csv(&input);
        assert_eq!(result, vec!["MCP server w/ Browser Use"]);
    }
}
