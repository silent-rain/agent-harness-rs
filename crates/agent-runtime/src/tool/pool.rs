//! Shared tool pool backing every agent's tool wiring.
//!
//! The pool is the **single source of truth** for an agent's available tools:
//! every tool instance lives here as an `Arc<dyn ContextualTool>` together
//! with its [`ToolSource`] tag. The runtime [`ContextualToolSet`] consumed
//! by `ToolExecutor` is a flat clone of the pool produced via
//! [`ToolPool::into_contextual_toolset`].
//!
//! ## Why a single map?
//!
//! Earlier revisions stored two parallel maps — one for stateless
//! `rig::ToolDyn` instances and another for [`ContextualTool`]s — and merged
//! them at every dispatch point. Stateless tools are now auto-wrapped via
//! [`StatelessAsContextual`] at registration time, so the pool only needs a
//! single uniform map and the resulting [`ContextualToolSet`] requires no
//! merge / fall-through logic.
//!
//! ## Sharing model
//!
//! Tool instances are stored as `Arc<dyn ContextualTool>`, so a parent agent
//! and any sub-agents reusing the same `Arc<ToolPool>` share the underlying
//! resources (HTTP clients, caches, …). Deriving a [`ContextualToolSet`]
//! only clones the `Arc`s, never the underlying instance.

use std::{collections::HashMap, sync::Arc};

use rig_core::tool::ToolDyn;

use crate::{
    tool::contextual::{ContextualTool, ContextualToolDyn, ContextualToolSet, RigToolWrapper},
    types::tool::ToolSource,
};

/// Unified tool pool: instance + source live in one table.
///
/// Each registration writes both `tools` and `sources` atomically, so
/// `source_of(name)` always agrees with whether `name` is callable. Cloning
/// the pool (or wrapping it in `Arc`) shares the underlying tool instances
/// — no instance is duplicated.
#[derive(Default, Clone)]
pub struct ToolPool {
    /// `tool_name` → contextual tool instance (shared via `Arc`).
    ///
    /// Stateless `rig::ToolDyn` tools are wrapped through
    /// [`StatelessAsContextual`] at registration time, so the runtime sees a
    /// uniform `ContextualToolDyn` shape regardless of source.
    tools: HashMap<String, Arc<dyn ContextualToolDyn>>,
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
    /// This is the single low-level entry point for all stateless rig tools.
    /// The tool is wrapped through [`RigToolWrapper`] so it sits in the same
    /// uniform map as stateful [`ContextualTool`]s. Same-named tools
    /// overwrite both the instance and the source (last-wins).
    pub fn register_rig_tool<T>(&mut self, tool: T, source: ToolSource) -> &mut Self
    where
        T: ToolDyn + 'static,
    {
        let adapter = RigToolWrapper::new(Arc::new(tool));
        let name = adapter.name().to_string();
        self.sources.insert(name.clone(), source);
        self.tools.insert(name, Arc::new(adapter));
        self
    }

    /// Register an already-boxed stateless tool with an explicit source tag.
    ///
    /// Useful for trait-object factories such as MCP remote tools where the
    /// concrete type is opaque.
    pub fn register_rig_tool_boxed(
        &mut self,
        tool: Box<dyn ToolDyn>,
        source: ToolSource,
    ) -> &mut Self {
        let adapter = RigToolWrapper::new(Arc::from(tool));
        let name = adapter.name().to_string();
        self.sources.insert(name.clone(), source);
        self.tools.insert(name, Arc::new(adapter));
        self
    }

    /// Register a stateful [`ContextualTool`] by value with an explicit
    /// source tag.
    ///
    /// Used for tools that need [`ToolCallCtx`](super::contextual::ToolCallCtx)
    /// at call time (e.g. sub-agent delegation). Same-named tools overwrite
    /// both the instance and the source (last-wins).
    pub fn register_contextual<T>(&mut self, tool: T, source: ToolSource) -> &mut Self
    where
        T: ContextualTool,
    {
        let name = ContextualTool::name(&tool).to_string();
        self.sources.insert(name.clone(), source);
        self.tools.insert(name, Arc::new(tool));
        self
    }

    // ─── Source-tagged convenience APIs ───────────────────────────────

    /// Register a built-in tool (`ToolSource::Builtin`).
    ///
    /// Accepts any `rig::Tool` implementation (auto-derives `ToolDyn`).
    pub fn register_builtin<T: ToolDyn + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::Builtin)
    }

    /// Register a skill-derived tool (`ToolSource::Skill`).
    pub fn register_skill<T: ToolDyn + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::Skill)
    }

    /// Register a command-derived tool. Commands surface through built-in
    /// introspection tools (`command_load` / `command_list`), so they tag as
    /// `ToolSource::Builtin`.
    pub fn register_command<T: ToolDyn + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::Builtin)
    }

    /// Register an MCP-derived tool by value (`ToolSource::Mcp`).
    pub fn register_mcp<T: ToolDyn + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::Mcp)
    }

    /// Register a sub-agent delegation tool (`ToolSource::SubAgent`).
    pub fn register_sub_agent<T: ToolDyn + 'static>(&mut self, tool: T) -> &mut Self {
        self.register_rig_tool(tool, ToolSource::SubAgent)
    }
    // ─── Bulk helpers ───────────────────────────────────────────────────

    /// Bulk-register a list of already-boxed stateless tools under the same
    /// source tag.
    ///
    /// Convenient with helpers such as `all_builtin_tools()` that yield
    /// `Vec<Box<dyn ToolDyn>>`. Same-named tools overwrite previous entries.
    pub fn register_many(&mut self, tools: Vec<Box<dyn ToolDyn>>, source: ToolSource) -> &mut Self {
        for tool in tools {
            self.register_rig_tool_boxed(tool, source);
        }
        self
    }

    /// Merge another pool into `self`, sharing `Arc`s and source tags.
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
    /// Replaces the prior pattern of "drain into a Vec, then re-register"
    /// which silently flipped source tags as a side effect. The tool
    /// instances themselves are left untouched.
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

    // ─── ContextualToolSet derivation ───────────────────────────────────

    /// Derive a [`ContextualToolSet`] containing every registered tool.
    ///
    /// Boxed adapters in the resulting set hold `Arc::clone`s of the pool's
    /// instances, so multiple derived sets share the same backing objects.
    /// This is the canonical entry point used by `LlmAgent` to assemble the
    /// per-run toolset; the resulting wrapper is what `ToolExecutor`
    /// dispatches against.
    pub fn into_contextual_toolset(&self) -> ContextualToolSet {
        let mut set = ContextualToolSet::new();
        for arc in self.tools.values() {
            set.register(Arc::clone(arc));
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{ListDirTool, ReadFileTool};

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

    #[tokio::test]
    async fn into_contextual_toolset_includes_all_tools() {
        let mut pool = ToolPool::new();
        pool.register_builtin(ReadFileTool)
            .register_builtin(ListDirTool);

        let set = pool.into_contextual_toolset();
        assert!(set.contains("read_file"));
        assert!(set.contains("list_dir"));
        let defs = set.tool_definitions();
        assert_eq!(defs.len(), 2);
    }

    #[test]
    fn register_many_applies_same_source_to_all() {
        let mut pool = ToolPool::new();
        pool.register_many(
            vec![Box::new(ReadFileTool), Box::new(ListDirTool)],
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
