//! 装配层加载器
//!
//! 按"资源域 → 数据来源"两级组织各类 Loader，与 `agent-runtime` 内部的
//! `rule/loader/local.rs` 结构形成镜像：
//!
//! - `agent-runtime` 在每个资源域内放置零依赖的本地加载器
//! - `agent-assembly` 在 `loader/<resource>/<source>.rs` 提供基于外部存储
//!   （DB / HTTP / Cache 等）的加载器
//!
//! 提供的主要内容：
//! - [`a2a`]: A2A 服务加载器（含 [`a2a::DbA2aLoader`]）
//! - [`rule`]: 规则加载器（含 [`rule::DbRuleLoader`]）
//! - [`command`]: 命令加载器（含 [`command::DbCommandLoader`]）
//! - [`mcp`]: MCP 服务加载器（含 [`mcp::DbMcpLoader`]）
//! - [`skill`]: 技能加载器（含 [`skill::DbSkillLoader`]）
//! - [`tool`]: 工具加载器（含 [`tool::DbToolLoader`]）
//! - [`sub_agent`]: 子智能体加载器（含 [`sub_agent::DbSubAgentLoader`]）

pub mod a2a;
pub mod command;
pub mod mcp;
pub mod rule;
pub mod skill;
pub mod sub_agent;
pub mod tool;
