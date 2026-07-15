//! 技能清单解析
//!
//! 从 `SKILL.md` 文件中提取技能元数据（`name` 和 `description`）及正文内容。
//!
//! ## 解析策略
//!
//! 采用**渐进式容错**策略，按以下优先级提取字段：
//!
//! 1. **YAML frontmatter**（以 `---` 分隔）中的显式字段
//! 2. **字段别名**：`description` 支持 `summary` / `desc` / `about` 别名
//! 3. **启发式推断**：
//!    - `name` 从正文第一个 H1 标题（`# Xxx`）推断
//!    - `description` 从正文第一个非空段落推断（截断到 256 字符）
//! 4. 未知 frontmatter 字段通过 `#[serde(flatten)]` 保留在 `extra` 中，避免解析失败
//!
//! ## 支持的 Frontmatter 示例
//!
//! 标准格式：
//!
//! ```yaml
//! ---
//! name: service-hub-generator
//! description: |
//!   Service Hub 服务开发规范技能。
//! ---
//! ```
//!
//! 仅有 name 也可（description 从正文推断）：
//!
//! ```yaml
//! ---
//! name: rust-expert
//! ---
//!
//! # Rust Expert
//!
//! 处理 Rust 代码时的专家指令...
//! ```
//!
//! 无 frontmatter 的纯 Markdown 也支持（name/description 均从正文推断）：
//!
//! ```markdown
//! # Rust Expert
//!
//! 处理 Rust 代码时的专家指令...
//! ```

use std::collections::BTreeMap;

use serde::Deserialize;

/// Description 推断时的最大截断长度（字符数）
const DESCRIPTION_AUTO_MAX_CHARS: usize = 256;

/// 技能清单
///
/// 从 `SKILL.md` 中解析出的技能元数据。字段均为可选——
/// 解析器采用**渐进式容错**策略，即使 frontmatter 缺失或格式不规范，
/// 也会尝试从正文中启发式推断 `name` 和 `description`。
///
/// 调用 [`SkillManifest::parse`] 永远不会失败，返回的 manifest 可能字段全为 `None`。
/// 最终是否使用目录名作为 fallback 由调用方决定。
#[derive(Debug, Clone, Default)]
pub struct SkillManifest {
    /// 技能名称（来自 frontmatter.name 或正文首个 H1 标题）
    pub name: Option<String>,
    /// 技能描述（来自 frontmatter.description/summary/desc/about 或正文首段落）
    pub description: Option<String>,
    /// Frontmatter 中的其他扩展字段（version / author / tags 等）
    pub extra: BTreeMap<String, config::Value>,
}

/// 内部使用：反序列化 YAML frontmatter 的临时结构
///
/// 所有字段可选 + `#[serde(flatten)]` 收集未知字段，
/// 确保 frontmatter 格式多样化时仍可解析成功。
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawFrontmatter {
    name: Option<String>,
    /// 主字段，同时支持 `summary` / `desc` / `about` 作为别名
    #[serde(alias = "summary", alias = "desc", alias = "about")]
    description: Option<String>,
    /// 未知字段（version / author / tags 等）全部收入 `extra`
    #[serde(flatten)]
    extra: BTreeMap<String, config::Value>,
}

impl SkillManifest {
    /// 从 SKILL.md 文件内容中解析 manifest
    ///
    /// 本方法**永不失败**：
    /// - 有效 frontmatter → 直接使用其字段
    /// - frontmatter 缺失或无效 → 从正文 H1 标题和首段推断
    /// - 两者皆无 → 返回字段全为 `None` 的空 manifest
    ///
    /// # Parameters
    ///
    /// * `content` - SKILL.md 文件的完整文本内容
    ///
    /// # Returns
    ///
    /// 总是返回一个 `SkillManifest`，字段为 `None` 时由调用方决定 fallback 策略
    pub fn parse(content: &str) -> Self {
        let raw = extract_frontmatter(content).and_then(parse_yaml_frontmatter);
        let body = Self::body(content);

        let (mut name, mut description, extra) = match raw {
            Some(fm) => (
                fm.name.filter(|s| !s.trim().is_empty()),
                fm.description.filter(|s| !s.trim().is_empty()),
                fm.extra,
            ),
            None => (None, None, BTreeMap::new()),
        };

        // Heuristic fallback: H1 title → name
        if name.is_none() {
            name = extract_first_h1(body);
        }

        // Heuristic fallback: first paragraph → description
        if description.is_none() {
            description = extract_first_paragraph(body)
                .map(|p| truncate_chars(&p, DESCRIPTION_AUTO_MAX_CHARS));
        }

        Self {
            name,
            description,
            extra,
        }
    }

    /// 获取 frontmatter 之后的正文内容
    ///
    /// # Parameters
    ///
    /// * `content` - SKILL.md 文件的完整文本内容
    ///
    /// # Returns
    ///
    /// frontmatter 之后的正文部分，若无 frontmatter 则返回完整内容（trim 后）
    pub fn body(content: &str) -> &str {
        let trimmed = content.trim();
        if !trimmed.starts_with("---") {
            return trimmed;
        }

        let after_open = &trimmed[3..];
        match after_open.find("\n---") {
            Some(pos) => {
                // Skip the closing `---` and any trailing newline
                let rest = &after_open[pos + 4..];
                rest.trim_start_matches('-').trim()
            }
            None => trimmed,
        }
    }
}

/// 提取 `---` 之间的 YAML frontmatter 字符串
///
/// 仅当 content 以 `---` 起始且存在闭合 `---` 时返回 `Some`。
fn extract_frontmatter(content: &str) -> Option<&str> {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_open = &trimmed[3..];
    // Require closing delimiter on its own line to avoid false positives
    let close_pos = after_open.find("\n---")?;
    Some(&after_open[..close_pos])
}

/// 尝试解析 YAML frontmatter，失败时返回 `None` 而非 panic
fn parse_yaml_frontmatter(yaml_str: &str) -> Option<RawFrontmatter> {
    let cfg = config::Config::builder()
        .add_source(config::File::from_str(yaml_str, config::FileFormat::Yaml))
        .build()
        .ok()?;

    match cfg.try_deserialize::<RawFrontmatter>() {
        Ok(fm) => Some(fm),
        Err(e) => {
            tracing::debug!(error = %e, "Failed to parse SKILL.md frontmatter as YAML");
            None
        }
    }
}

/// 从正文中提取第一个 H1 标题（`# Xxx`）
fn extract_first_h1(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let title = rest.trim().trim_end_matches('#').trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

/// 从正文中提取第一个非空段落
///
/// 规则：
/// - 跳过 H1 / H2 等标题行
/// - 跳过代码块（``` 包围的内容）
/// - 首个连续非空非标题行构成段落
fn extract_first_paragraph(body: &str) -> Option<String> {
    let mut paragraph = String::new();
    let mut in_code_block = false;

    for line in body.lines() {
        let trimmed = line.trim();

        // Toggle code block fence
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        // Skip headings
        if trimmed.starts_with('#') {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }

        if trimmed.is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }

        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(trimmed);
    }

    if paragraph.is_empty() {
        None
    } else {
        Some(paragraph)
    }
}

/// 按字符数（而非字节数）截断字符串，避免在多字节字符中截断
fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_frontmatter() {
        let content = r#"---
name: test-skill
description: |
  A test skill.
  Use when testing.
---

# Test Skill

Some body content.
"#;
        let manifest = SkillManifest::parse(content);
        assert_eq!(manifest.name.as_deref(), Some("test-skill"));
        assert!(manifest.description.unwrap().contains("A test skill"));
    }

    #[test]
    fn test_parse_no_frontmatter_uses_h1_and_paragraph() {
        let content = "# My Skill\n\nThis is the intro paragraph.\n\nAnother paragraph.";
        let manifest = SkillManifest::parse(content);
        assert_eq!(manifest.name.as_deref(), Some("My Skill"));
        assert_eq!(
            manifest.description.as_deref(),
            Some("This is the intro paragraph.")
        );
    }

    #[test]
    fn test_parse_only_name_in_frontmatter() {
        let content = r#"---
name: rust-expert
---

# Rust Expert

处理 Rust 代码时的专家指令。
"#;
        let manifest = SkillManifest::parse(content);
        assert_eq!(manifest.name.as_deref(), Some("rust-expert"));
        assert_eq!(
            manifest.description.as_deref(),
            Some("处理 Rust 代码时的专家指令。")
        );
    }

    #[test]
    fn test_parse_summary_alias() {
        let content = r#"---
name: alias-skill
summary: Described via summary alias
---

body"#;
        let manifest = SkillManifest::parse(content);
        assert_eq!(
            manifest.description.as_deref(),
            Some("Described via summary alias")
        );
    }

    #[test]
    fn test_parse_desc_alias() {
        let content = r#"---
name: alias-skill
desc: Described via desc alias
---

body"#;
        let manifest = SkillManifest::parse(content);
        assert_eq!(
            manifest.description.as_deref(),
            Some("Described via desc alias")
        );
    }

    #[test]
    fn test_parse_preserves_extra_fields() {
        let content = r#"---
name: ext-skill
description: Has extras
version: "1.2.3"
author: alice
tags:
  - rust
  - async
---

body"#;
        let manifest = SkillManifest::parse(content);
        assert!(manifest.extra.contains_key("version"));
        assert!(manifest.extra.contains_key("author"));
        assert!(manifest.extra.contains_key("tags"));
    }

    #[test]
    fn test_parse_pure_markdown_no_h1() {
        let content = "Just some text without any heading.\n\nSecond paragraph.";
        let manifest = SkillManifest::parse(content);
        assert!(manifest.name.is_none());
        assert_eq!(
            manifest.description.as_deref(),
            Some("Just some text without any heading.")
        );
    }

    #[test]
    fn test_parse_empty_content() {
        let manifest = SkillManifest::parse("");
        assert!(manifest.name.is_none());
        assert!(manifest.description.is_none());
    }

    #[test]
    fn test_parse_invalid_yaml_falls_back_to_heuristics() {
        // Malformed YAML (unclosed bracket) — should not fail, just fall back
        let content = "---\nname: [bad\n---\n\n# Fallback Title\n\nFallback description.";
        let manifest = SkillManifest::parse(content);
        assert_eq!(manifest.name.as_deref(), Some("Fallback Title"));
        assert_eq!(
            manifest.description.as_deref(),
            Some("Fallback description.")
        );
    }

    #[test]
    fn test_parse_skips_code_blocks_in_paragraph_detection() {
        let content = r#"# Title

```rust
fn foo() {}
```

The real description paragraph.
"#;
        let manifest = SkillManifest::parse(content);
        assert_eq!(
            manifest.description.as_deref(),
            Some("The real description paragraph.")
        );
    }

    #[test]
    fn test_parse_truncates_long_description() {
        let long = "a".repeat(500);
        let content = format!("# Title\n\n{long}");
        let manifest = SkillManifest::parse(&content);
        let desc = manifest.description.unwrap();
        // Truncated to max chars + 1 ellipsis
        assert_eq!(desc.chars().count(), DESCRIPTION_AUTO_MAX_CHARS + 1);
        assert!(desc.ends_with('…'));
    }

    #[test]
    fn test_body_extraction() {
        let content = r#"---
name: test
description: desc
---

# Body Content

Hello world."#;
        let body = SkillManifest::body(content);
        assert!(body.starts_with("# Body Content"));
        assert!(body.contains("Hello world."));
    }

    #[test]
    fn test_body_no_frontmatter() {
        let content = "# No frontmatter\n\nJust content.";
        let body = SkillManifest::body(content);
        assert_eq!(body, content.trim());
    }

    #[test]
    fn test_body_unclosed_frontmatter() {
        // Unclosed frontmatter — return full content as body
        let content = "---\nname: x\n# Actually content\n";
        let body = SkillManifest::body(content);
        assert!(body.contains("---"));
    }
}
