//! 规则列表扫描工具
//!
//! 提供对当前已注册规则列表的只读查询能力。
//! LLM 可通过此工具获取所有已注入 System Prompt 的规则摘要信息（名称与来源）。
//!
//! ## 设计说明
//!
//! 规则内容在启动时已全量注入 System Prompt，无需按需加载。
//! 此工具仅用于让 LLM 了解当前生效的规则集合。

use std::sync::Arc;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::{
    error::Error,
    types::rule::{RuleConfig, RuleSource},
};

/// 规则列表扫描工具
///
/// 返回所有已注册规则的摘要信息（名称与来源），无需任何参数。
/// 适用于 LLM 需要了解当前生效规则全集的场景。
pub struct RuleListTool {
    /// 已注册的规则摘要列表（按名称排序）
    entries: Arc<Vec<RuleEntry>>,
}

/// 规则摘要条目
#[derive(Debug, Clone, Serialize)]
pub struct RuleEntry {
    /// 规则名称
    pub name: String,
    /// 规则描述（本地文件加载时可为空）
    pub description: String,
    /// 规则来源
    pub source: RuleSource,
}

impl RuleListTool {
    /// 创建规则列表工具
    ///
    /// # Parameters
    ///
    /// * `rule_configs` - 已加载的规则配置列表
    pub fn new(rule_configs: &[RuleConfig]) -> Self {
        let mut entries: Vec<RuleEntry> = rule_configs
            .iter()
            .map(|r| RuleEntry {
                name: r.name.clone(),
                description: r.description.clone(),
                source: r.source.clone(),
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Self {
            entries: Arc::new(entries),
        }
    }
}

/// 规则列表工具参数（无参数）
#[derive(Debug, Deserialize)]
pub struct RuleListArgs {}

/// 规则列表工具输出
#[derive(Debug, Serialize)]
pub struct RuleListOutput {
    /// 可用规则总数
    pub total: usize,
    /// 规则摘要列表
    pub rules: Vec<RuleEntry>,
}

impl Tool for RuleListTool {
    const NAME: &'static str = "rule_list";

    type Error = Error;
    type Args = RuleListArgs;
    type Output = RuleListOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "rule_list".to_string(),
            description: "列出所有已注入 System Prompt 的规则摘要（名称与来源）。\
                当需要了解当前生效的规则全集时调用此工具。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(RuleListOutput {
            total: self.entries.len(),
            rules: (*self.entries).clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::rule::RuleSource;

    fn make_rules() -> Vec<RuleConfig> {
        vec![
            RuleConfig {
                name: "security".to_string(),
                description: String::new(),
                content: "No hardcoded secrets.".to_string(),
                source: RuleSource::Local,
                source_hint: Some(".codebuddy/rules/security.md".to_string()),
            },
            RuleConfig {
                name: "code-style".to_string(),
                description: String::new(),
                content: "Use 4 spaces for indentation.".to_string(),
                source: RuleSource::Local,
                source_hint: Some(".codebuddy/rules/code-style.md".to_string()),
            },
            RuleConfig {
                name: "db-rule-001".to_string(),
                description: "Always use transactions.".to_string(),
                content: "Always use transactions.".to_string(),
                source: RuleSource::Injected,
                source_hint: Some("db:42".to_string()),
            },
        ]
    }

    #[tokio::test]
    async fn test_rule_list_returns_all_rules() {
        let rules = make_rules();
        let tool = RuleListTool::new(&rules);

        let result = tool.call(RuleListArgs {}).await.unwrap();

        assert_eq!(result.total, 3);
        assert_eq!(result.rules.len(), 3);
    }

    #[tokio::test]
    async fn test_rule_list_sorted_by_name() {
        let rules = make_rules();
        let tool = RuleListTool::new(&rules);

        let result = tool.call(RuleListArgs {}).await.unwrap();

        let names: Vec<&str> = result.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["code-style", "db-rule-001", "security"]);
    }

    #[tokio::test]
    async fn test_rule_list_source_field() {
        let rules = make_rules();
        let tool = RuleListTool::new(&rules);

        let result = tool.call(RuleListArgs {}).await.unwrap();

        let db_rule = result
            .rules
            .iter()
            .find(|r| r.name == "db-rule-001")
            .unwrap();
        assert_eq!(db_rule.source, RuleSource::Injected);

        let security = result.rules.iter().find(|r| r.name == "security").unwrap();
        assert_eq!(security.source, RuleSource::Local);
    }

    #[tokio::test]
    async fn test_rule_list_empty_registry() {
        let tool = RuleListTool::new(&[]);

        let result = tool.call(RuleListArgs {}).await.unwrap();

        assert_eq!(result.total, 0);
        assert!(result.rules.is_empty());
    }
}
