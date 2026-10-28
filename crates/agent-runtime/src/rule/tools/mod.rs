//! Rule 内省工具
//!
//! 提供 LLM 可调用的规则内省能力：
//! - [`RuleListTool`] - 列出当前所有规则的摘要（名称 / 描述 / 来源）
//! - [`RuleLoadTool`] - 按名称按需加载单条规则的完整正文
mod rule_list;
mod rule_load;

pub use rule_list::RuleListTool;
pub use rule_load::RuleLoadTool;
