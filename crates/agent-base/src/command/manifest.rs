//! 命令清单解析
//!
//! 从命令 `.md` 文件中提取命令元数据（`name` 和 `description`）及正文内容。
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
//! ```yaml
//! ---
//! name: 生成文档
//! description: 为指定模块生成 API 文档
//! ---
//!
//! 请为以下模块生成完整的 API 文档...
//! ```

use std::collections::BTreeMap;

use serde::Deserialize;

/// Description 推断时的最大截断长度（字符数）
const DESCRIPTION_AUTO_MAX_CHARS: usize = 256;

/// 命令清单
///
/// 从命令 `.md` 文件中解析出的命令元数据。字段均为可选——
/// 解析器采用**渐进式容错**策略，即使 frontmatter 缺失或格式不规范，
/// 也会尝试从正文中启发式推断 `name` 和 `description`。
///
/// 调用 [`CommandManifest::parse`] 永远不会失败，返回的 manifest 可能字段全为 `None`。
/// 最终是否使用文件名作为 fallback 由调用方决定。
#[derive(Debug, Clone, Default)]
pub struct CommandManifest {
    /// 命令名称（来自 frontmatter.name 或正文首个 H1 标题）
    pub name: Option<String>,
    /// 命令描述（来自 frontmatter.description/summary/desc/about 或正文首段落）
    pub description: Option<String>,
    /// Frontmatter 中的其他扩展字段
    pub extra: BTreeMap<String, config::Value>,
}

/// 内部使用：反序列化 YAML frontmatter 的临时结构
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawFrontmatter {
    name: Option<String>,
    #[serde(alias = "summary", alias = "desc", alias = "about")]
    description: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, config::Value>,
}

impl CommandManifest {
    /// 从命令 `.md` 文件内容中解析 manifest
    ///
    /// 本方法**永不失败**：
    /// - 有效 frontmatter → 直接使用其字段
    /// - frontmatter 缺失或无效 → 从正文 H1 标题和首段推断
    /// - 两者皆无 → 返回字段全为 `None` 的空 manifest
    ///
    /// # Parameters
    ///
    /// * `content` - 命令 `.md` 文件的完整文本内容
    ///
    /// # Returns
    ///
    /// 总是返回一个 `CommandManifest`，字段为 `None` 时由调用方决定 fallback 策略
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
    /// * `content` - 命令 `.md` 文件的完整文本内容
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
                let rest = &after_open[pos + 4..];
                rest.trim_start_matches('-').trim()
            }
            None => trimmed,
        }
    }
}

/// 提取 `---` 之间的 YAML frontmatter 字符串
fn extract_frontmatter(content: &str) -> Option<&str> {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_open = &trimmed[3..];
    let close_pos = after_open.find("\n---")?;
    Some(&after_open[..close_pos])
}

/// 尝试解析 YAML frontmatter，失败时返回 `None`
fn parse_yaml_frontmatter(yaml_str: &str) -> Option<RawFrontmatter> {
    let cfg = config::Config::builder()
        .add_source(config::File::from_str(yaml_str, config::FileFormat::Yaml))
        .build()
        .ok()?;

    match cfg.try_deserialize::<RawFrontmatter>() {
        Ok(fm) => Some(fm),
        Err(e) => {
            tracing::debug!(error = %e, "Failed to parse command frontmatter as YAML");
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
/// 跳过标题行和代码块，首个连续非空非标题行构成段落。
fn extract_first_paragraph(body: &str) -> Option<String> {
    let mut paragraph = String::new();
    let mut in_code_block = false;

    for line in body.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

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

/// 按字符数截断字符串，避免在多字节字符中截断
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
name: gen-doc
description: |
  Generate API documentation.
---

# Generate Documentation

Please generate complete API docs for the following module.
"#;
        let manifest = CommandManifest::parse(content);
        assert_eq!(manifest.name.as_deref(), Some("gen-doc"));
        assert!(
            manifest
                .description
                .unwrap()
                .contains("Generate API documentation")
        );
    }

    #[test]
    fn test_parse_no_frontmatter_uses_h1_and_paragraph() {
        let content = "# Refactor Code\n\nRefactor the selected code for better readability.";
        let manifest = CommandManifest::parse(content);
        assert_eq!(manifest.name.as_deref(), Some("Refactor Code"));
        assert_eq!(
            manifest.description.as_deref(),
            Some("Refactor the selected code for better readability.")
        );
    }

    #[test]
    fn test_parse_only_name_in_frontmatter() {
        let content = r#"---
name: gen-test
---

# Generate Tests

Generate unit tests for the current module.
"#;
        let manifest = CommandManifest::parse(content);
        assert_eq!(manifest.name.as_deref(), Some("gen-test"));
        assert_eq!(
            manifest.description.as_deref(),
            Some("Generate unit tests for the current module.")
        );
    }

    #[test]
    fn test_parse_summary_alias() {
        let content = r#"---
name: alias-cmd
summary: Described via summary alias
---

body"#;
        let manifest = CommandManifest::parse(content);
        assert_eq!(
            manifest.description.as_deref(),
            Some("Described via summary alias")
        );
    }

    #[test]
    fn test_parse_preserves_extra_fields() {
        let content = r#"---
name: ext-cmd
description: Has extras
version: "1.0.0"
author: alice
---

body"#;
        let manifest = CommandManifest::parse(content);
        assert!(manifest.extra.contains_key("version"));
        assert!(manifest.extra.contains_key("author"));
    }

    #[test]
    fn test_parse_empty_content() {
        let manifest = CommandManifest::parse("");
        assert!(manifest.name.is_none());
        assert!(manifest.description.is_none());
    }

    #[test]
    fn test_body_extraction() {
        let content = r#"---
name: test
description: desc
---

# Body Content

Hello world."#;
        let body = CommandManifest::body(content);
        assert!(body.starts_with("# Body Content"));
        assert!(body.contains("Hello world."));
    }

    #[test]
    fn test_body_no_frontmatter() {
        let content = "# No frontmatter\n\nJust content.";
        let body = CommandManifest::body(content);
        assert_eq!(body, content.trim());
    }
}
