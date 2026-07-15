//! 规则加载器
//!
//! 提供从不同来源加载规则的加载器。每个加载器返回 [`crate::types::rule::RuleConfig`] 列表，
//! 由调用方通过
//! [`AgentBuilder::add_rules`](crate::runtime::AgentBuilder::add_rules) 注入到 bundle 中统一合并。
//!
//! 提供的主要内容：
//! - [`LocalRuleLoader`] - 从本地文件系统扫描规则文件

mod local;

pub use local::LocalRuleLoader;
