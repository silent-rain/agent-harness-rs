//! 规则模块
//!
//! 提供规则加载器与规则相关工具。规则（Rule）是注入到 System Prompt 中的纯文本约束或指令。
//!
//! 提供的主要内容：
//! - [`LocalRuleLoader`] - 本地文件系统规则加载器
//! - [`RuleListTool`] - 内置工具，列出当前已注入的规则
//!
//! ## 设计说明
//!
//! 各类 Loader 负责从特定来源加载 [`RuleConfig`](crate::types::rule::RuleConfig)，
//! 由调用方将结果通过
//! [`AgentBuilder::add_rules`](crate::runtime::AgentBuilder::add_rules) 直接注入
//! 到 [`AgentBundle`](crate::types::runtime::AgentBundle) 中，
//! 在 build 阶段由 builder 统一按 `name` 去重（先注入者位置优先 + 后写覆盖）。
//!
//! ```text
//!   ┌─────────────────────┐
//!   │ LocalRuleLoader     │──load──┐
//!   ├─────────────────────┤        │  add_rules
//!   │ DbRuleLoader (业务) │──load──┼──────────────▶ AgentBuilder ──build──▶ Vec<RuleConfig>
//!   ├─────────────────────┤        │
//!   │ HttpRuleLoader ...  │──load──┘
//!   └─────────────────────┘
//! ```
//!
//! 这种分离使得：
//! - 新增来源（数据库、HTTP、Redis 等）只需新增一个 Loader
//! - Builder 在 build 阶段统一去重，避免规则在 System Prompt 中互相打架
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use agent_base::rule::LocalRuleLoader;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // 本地加载
//! let local_rules = LocalRuleLoader::new(".codebuddy/rules").load().await?;
//!
//! // 动态注入（数据库或 HTTP 加载后注入）
//! use agent_base::types::rule::{RuleConfig, RuleSource};
//! let injected_rule = RuleConfig {
//!     name: "db-rule-001".to_string(),
//!     description: "Always use transactions.".to_string(),
//!     content: "Always use transactions.".to_string(),
//!     source: RuleSource::Injected,
//!     source_hint: Some("db:42".to_string()),
//! };
//!
//! // 通过 AgentBuilder::add_rules 注入到 bundle
//! // builder.add_rules(local_rules).add_rules(vec![injected_rule]);
//! # Ok(())
//! # }
//! ```
mod loader;
mod tools;

pub use loader::LocalRuleLoader;
pub use tools::RuleListTool;
