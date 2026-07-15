//! Rule 内省工具
//!
//! 提供 LLM 可调用的规则内省能力：
//! - [`RuleListTool`] - 列出当前所有已注入 System Prompt 的规则摘要

mod rule_list;

pub use rule_list::RuleListTool;
