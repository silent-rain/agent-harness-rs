//! Shared tool pool backing every agent's tool wiring.
//!
//! The pool is the **single source of truth** for an agent's available tools:
//! every tool lives here as a rig [`DynamicTool`] together with its
//! [`ToolSource`] tag. `ToolExecutor` dispatches directly against the pool —
//! there is no second registry type, and stateless / contextual tools share
//! one code path (both are `DynamicTool` receiving the same
//! [`ToolContext`](rig_core::tool::ToolContext)).
//!
//! ## Sharing model
//!
//! `DynamicTool` is `Clone` over an `Arc`-held callback, so a parent agent and
//! any sub-agents reusing the same `Arc<ToolPool>` share the underlying
//! instances (HTTP clients, caches, …) — deriving a dispatch list never
//! duplicates them.

use std::{collections::HashMap, sync::Arc};

use rig_core::{
    completion::ToolDefinition,
    tool::{DynamicTool, ToolContext},
};

use crate::{
    error::Error,
    tool::{
        rig_bridge::{dynamic_tool, tool_output_to_string},
        // `tool_name_of` 由 rig_bridge 统一提供，避免名称规则分叉
    },
    types::tool::ToolSource,
};

/// 工具派发错误。
///
/// `ToolExecutor` 会把它折叠成字符串写入 `ToolCallFinished` 审计事件，
/// 因此变体划分保持最小：工具自身报错 / 名称未注册。
#[derive(Debug, thiserror::Error)]
pub enum ToolCallError {
    /// 工具执行失败（含参数解析失败）。
    #[error("tool error: {0}")]
    Tool(#[from] Error),
    /// 没有注册该名称的工具。
    #[error("tool not found: {0}")]
    NotFound(String),
}

/// Unified tool pool: instance + source live in one table.
///
/// Each registration writes both `tools` and `sources` atomically, so
/// `source_of(name)` always agrees with whether `name` is callable. Cloning
/// the pool (or wrapping it in `Arc`) shares the underlying tool instances
/// — no instance is duplicated.
#[derive(Default, Clone)]
pub struct ToolPool {
    /// `tool_name` → 运行期工具实例（`DynamicTool` 内部以 `Arc` 共享回调）。
    tools: HashMap<String, Arc<DynamicTool>>,
    /// `tool_name` → source tag (mirrors `tools` keys).
    sources: HashMap<String, ToolSource>,
}

impl ToolPool {
    /// Create an empty pool.
    pub fn new() -> Self {
        Self::default()
    }

    // ─── Generic entry points ────────────────────────────────────────

    /// Register any `rig::Tool` implementation with an explicit source tag.
    ///
    /// This is the single low-level entry point for typed tools: the tool is
    /// adapted into a [`DynamicTool`] and shares the same dispatch path as
    /// runtime-defined tools. Same-named tools overwrite both the instance and
    /// the source (last-wins).
    pub fn register_rig_tool<T>(&mut self, tool: T, source: ToolSource) -> &mut Self
    where
        T: rig_core::tool::Tool + 'static,
    {
        self.register_dynamic(dynamic_tool(tool), source)
    }

    /// Register an already-erased [`DynamicTool`] with an explicit source tag.
    ///
    /// Used by every dynamic-name tool and by trait-object factories such as
    /// MCP remote tools where the concrete type is opaque. Same-named tools
    /// overwrite both the instance and the source (last-wins).
    pub fn register_dynamic(&mut self, tool: DynamicTool, source: ToolSource) -> &mut Self {
        let name = tool.name().to_string();
        self.sources.insert(name.clone(), source);
        self.tools.insert(name, Arc::new(tool));
        self
    }

    // ─── Source-tagged convenience APIs ───────────────────────────────

    /// Register a built-in tool (`ToolSource::Builtin`).
    ///
    /// Accepts any `rig::Tool` implementation.
    pub fn register_builtin<T: rig_core::tool::Tool + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::Builtin)
    }

    /// Register a skill-derived tool (`ToolSource::Skill`).
    pub fn register_skill<T: rig_core::tool::Tool + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::Skill)
    }

    /// Register a command-derived tool. Commands surface through built-in
    /// introspection tools (`command_load` / `command_list`), so they tag as
    /// `ToolSource::Builtin`.
    pub fn register_command<T: rig_core::tool::Tool + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::Builtin)
    }

    /// Register an MCP-derived tool by value (`ToolSource::Mcp`).
    pub fn register_mcp<T: rig_core::tool::Tool + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::Mcp)
    }

    /// Register a sub-agent delegation tool (`ToolSource::SubAgent`).
    pub fn register_sub_agent<T: rig_core::tool::Tool + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::SubAgent)
    }

    // ─── Bulk helpers ───────────────────────────────────────────────────

    /// Bulk-register a list of already-erased tools under the same source tag.
    ///
    /// Convenient with helpers such as `all_builtin_tools()` that yield
    /// `Vec<DynamicTool>`. Same-named tools overwrite previous entries.
    pub fn register_many(&mut self, tools: Vec<DynamicTool>, source: ToolSource) -> &mut Self {
        for tool in tools {
            self.register_dynamic(tool, source);
        }
        self
    }

    /// Merge another pool into `self`, sharing instances and source tags.
    ///
    /// Same-named tools overwrite previous entries.
    pub fn merge(&mut self, other: &ToolPool) -> &mut Self {
        for (name, arc) in &other.tools {
            self.tools.insert(name.clone(), Arc::clone(arc));
        }
        for (name, source) in &other.sources {
            self.sources.insert(name.clone(), *source);
        }
        self
    }

    /// Re-tag every registered tool with the given source.
    ///
    /// The tool instances themselves are left untouched.
    pub fn retag_all(&mut self, source: ToolSource) -> &mut Self {
        for slot in self.sources.values_mut() {
            *slot = source;
        }
        self
    }

    /// Re-tag the named subset with the given source. Missing names are
    /// silently skipped to keep config drift non-fatal.
    pub fn retag(&mut self, names: &[String], source: ToolSource) -> &mut Self {
        for name in names {
            if self.tools.contains_key(name) {
                self.sources.insert(name.clone(), source);
            }
        }
        self
    }

    /// Look up the source tag for a tool name.
    ///
    /// Returns [`ToolSource::Unknown`] for unregistered names so the runtime
    /// event stream stays robust against stale references.
    pub fn source_of(&self, name: &str) -> ToolSource {
        self.sources
            .get(name)
            .copied()
            .unwrap_or(ToolSource::Unknown)
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the pool contains no tools.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Whether `tool_name` is registered.
    pub fn contains(&self, tool_name: &str) -> bool {
        self.tools.contains_key(tool_name)
    }

    /// Iterate over registered tool codes (unspecified order).
    pub fn tool_names(&self) -> impl Iterator<Item = &str> {
        self.tools.keys().map(String::as_str)
    }

    // ─── Dispatch ───────────────────────────────────────────────────────

    /// 返回每个已注册工具的 [`ToolDefinition`]，供 LLM function-calling 使用。
    ///
    /// 顺序不保证稳定。
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }

    /// 派发一次工具调用。
    ///
    /// 参数以 LLM 给出的 JSON 文本传入，结果折叠为字符串回填给模型；
    /// 上下文（运行身份 + 事件通道作用域）由调用方装配后传入。
    ///
    /// # Parameters
    ///
    /// * `ctx` - 本次调用的 [`ToolContext`]
    /// * `name` - 工具名称（须已注册）
    /// * `args_json` - LLM 给出的参数 JSON 文本
    pub async fn call(
        &self,
        ctx: &mut ToolContext,
        name: &str,
        args_json: &str,
    ) -> Result<String, ToolCallError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolCallError::NotFound(name.to_string()))?;

        let args: serde_json::Value =
            serde_json::from_str(args_json).map_err(|e| Error::ToolExecution {
                tool_name: name.to_string(),
                message: format!("invalid arguments JSON: {e}"),
            })?;
        // 无参工具的模型输出常见为 `null`，统一归一为空对象
        let args = if args.is_null() {
            serde_json::json!({})
        } else {
            args
        };

        let output = tool
            .run_with(ctx, args)
            .await
            .map_err(|e| Error::ToolExecution {
                tool_name: name.to_string(),
                message: e.message().to_string(),
            })?;

        Ok(tool_output_to_string(&output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{ListDirTool, ReadFileTool, rig_bridge::dynamic_tool};

    #[test]
    fn register_writes_instance_and_source_atomically() {
        let mut pool = ToolPool::new();
        pool.register_builtin(ReadFileTool);

        assert!(pool.contains("read_file"));
        assert_eq!(pool.source_of("read_file"), ToolSource::Builtin);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn register_skill_tags_source_skill() {
        let mut pool = ToolPool::new();
        pool.register_skill(ReadFileTool);
        assert_eq!(pool.source_of("read_file"), ToolSource::Skill);
    }

    #[test]
    fn register_overwrites_same_name() {
        let mut pool = ToolPool::new();
        pool.register_builtin(ReadFileTool)
            .register_skill(ReadFileTool); // same name, new source
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.source_of("read_file"), ToolSource::Skill);
    }

    #[test]
    fn tool_definitions_list_every_registered_tool() {
        let mut pool = ToolPool::new();
        pool.register_builtin(ReadFileTool)
            .register_builtin(ListDirTool);

        let mut names: Vec<String> = pool
            .tool_definitions()
            .iter()
            .map(|def| def.name.to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["list_dir".to_string(), "read_file".to_string()]);
    }

    #[test]
    fn register_many_applies_same_source_to_all() {
        let mut pool = ToolPool::new();
        pool.register_many(
            vec![dynamic_tool(ReadFileTool), dynamic_tool(ListDirTool)],
            ToolSource::Skill,
        );
        assert_eq!(pool.source_of("read_file"), ToolSource::Skill);
        assert_eq!(pool.source_of("list_dir"), ToolSource::Skill);
    }

    #[test]
    fn source_of_unknown_returns_unknown() {
        let pool = ToolPool::new();
        assert_eq!(pool.source_of("missing"), ToolSource::Unknown);
    }
}
