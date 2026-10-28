//! 规则按需加载工具
//!
//! 根据规则名称返回该规则的完整正文内容，用于规则未全量注入 System Prompt
//! （例如规则数量较多、按需取用）或 LLM 需要复核某条规则原文时。
//!
//! ## 设计说明
//!
//! 与 `command_load` 对称：
//! 1. 构建时：规则已由各 Loader 加载进内存（`RuleConfig.content`）
//! 2. **调用时**：LLM 传入规则名称，本工具直接命中内存映射表，不再发起 I/O
//!
//! 与 `rule_list` 的分工：
//! - [`RuleListTool`](crate::rule::RuleListTool)：列出名称 / 描述 / 来源摘要
//! - `RuleLoadTool`：按名称取回单条规则的正文

use std::collections::HashMap;
use std::sync::Arc;

use rig_core::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};

use crate::{error::Error, types::rule::RuleConfig};

/// 规则按需加载工具
///
/// 根据 `rule_name` 从已注册的规则列表中查找并返回规则的完整正文。
/// 规则内容在构建时已加载到内存中（来自本地文件或动态注入），无需再次读取文件。
pub struct RuleLoadTool {
    /// 已注册的规则映射表（name → content）
    rules: Arc<HashMap<String, String>>,
}

impl RuleLoadTool {
    /// 创建规则加载工具
    ///
    /// # Parameters
    ///
    /// * `rule_configs` - 已加载的规则配置列表
    pub fn new(rule_configs: &[RuleConfig]) -> Self {
        let rules: HashMap<String, String> = rule_configs
            .iter()
            .map(|r| (r.name.clone(), r.content.clone()))
            .collect();
        Self {
            rules: Arc::new(rules),
        }
    }

    /// 获取可用的规则名称列表（已排序）
    pub fn available_rules(&self) -> Vec<String> {
        let mut names: Vec<String> = self.rules.keys().cloned().collect();
        names.sort();
        names
    }
}

/// 规则加载工具参数
#[derive(Debug, Deserialize)]
pub struct RuleLoadArgs {
    /// 规则名称（即 Rules 列表中的规则标识）
    rule_name: String,
}

/// 规则加载工具输出
#[derive(Debug, Serialize)]
pub struct RuleLoadOutput {
    /// 规则名称
    rule_name: String,
    /// 规则完整正文内容
    content: String,
}

impl Tool for RuleLoadTool {
    const NAME: &'static str = "rule_load";

    type Error = Error;
    type Args = RuleLoadArgs;
    type Output = RuleLoadOutput;

    fn description(&self) -> String {
        "加载指定规则的完整正文内容。当需要查看某条规则的原文时调用此工具，\
                规则名称见 Rules 列表。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "rule_name": {
                    "type": "string",
                    "description": "要加载的规则名称（即 Rules 列表中的规则标识）"
                }
            },
            "required": ["rule_name"]
        })
    }

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let rule_name = &args.rule_name;

        // Look up rule content from the in-memory registry
        let content = self
            .rules
            .get(rule_name)
            .ok_or_else(|| Error::ToolExecution {
                tool_name: "rule_load".to_string(),
                message: format!(
                    "规则 '{}' 不在可用列表中。可用规则: {}",
                    rule_name,
                    self.available_rules().join(", ")
                ),
            })?;

        Ok(RuleLoadOutput {
            rule_name: rule_name.clone(),
            content: content.clone(),
        })
    }
}
