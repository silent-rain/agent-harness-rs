//! Context-aware tool abstraction unifying stateless and stateful tools.
//!
//! ## Why a single abstraction?
//!
//! `rig_core::tool::Tool` follows a pure request/response contract: it does
//! not allow runtime context (parent event sender, parent `run_id`, ...) to
//! flow into a tool invocation. That's a deliberate, sound design for
//! stateless tools (file IO, web search, MCP RPC, ...).
//!
//! Sub-agent delegation, however, is *not* stateless: it needs to forward
//! child events back into the parent stream. Earlier revisions kept two
//! parallel registries (one `rig::ToolSet` for stateless tools and a
//! separate map for contextual ones) and merged their definitions / call
//! routes at every dispatch point. That worked but introduced redundant
//! storage, last-wins merge logic, and double bookkeeping between
//! [`crate::tool::ToolPool`] and [`ContextualToolSet`].
//!
//! This module unifies both worlds behind a typed trait pair, mirroring
//! `rig_core::tool::{Tool, ToolDyn}`:
//!
//! - [`ContextualTool`] — the **typed** trait implementers write against.
//!   Carries `Args` / `Output` / `Error` associated types so each tool
//!   speaks structured Rust values, never raw JSON strings.
//! - [`ContextualToolDyn`] — the **object-safe** wrapper the runtime
//!   dispatches against. Blanket-implemented for every `ContextualTool`,
//!   so implementers never touch JSON (de)serialisation directly.
//! - [`StatelessAsContextual`] — adapts a stateless `rig::ToolDyn` into
//!   `ContextualToolDyn`, passing JSON straight through (no double
//!   serde round-trip). Stateless tools therefore plug into the same
//!   registry without rewriting them.
//! - [`ToolCallCtx`] — borrowed runtime context (parent sender, run_id, ...).
//! - [`ContextualToolSet`] — flat registry keyed by tool name.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use rig_core::{completion::ToolDefinition, tool::ToolDyn};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::mpsc;

use crate::{error::Error, session::AgentStreamEvent};

/// Borrowed runtime context handed to a [`ContextualTool`] at call time.
///
/// All fields are borrowed for the duration of the call — no shared mutable
/// state, no locks, no `Option` fallbacks. The lifetime is tied to the
/// `ToolExecutor` that owns the originating sender and `run_id`.
pub struct ToolCallCtx<'a> {
    /// Parent run identifier produced once per `run_stream` call.
    pub run_id: &'a str,
    /// Current ReAct step index (0-based).
    pub step: u32,
    /// Parent event sender. Contextual tools that produce sub-events
    /// (e.g. sub-agent delegation) push wrapped events into this channel.
    pub events: &'a mpsc::Sender<AgentStreamEvent>,
    /// Session id for the in-flight run.
    pub session_id: &'a str,
    /// Current agent ID. Passed through so contextual tools can scope their
    /// operations to the owning agent.
    pub agent_id: i64,
    /// Current user ID. Passed through so contextual tools (e.g. skill
    /// management) can scope their operations to the calling user without
    /// carrying a separate constructor argument.
    pub user_id: &'a str,
}

/// Project-local **typed** tool trait dispatched by the agent runtime.
///
/// This is the trait implementers actually write against. It mirrors
/// `rig_core::tool::Tool` (associated `Args` / `Output` / `Error`) so tool
/// authors work with structured Rust values exclusively — JSON
/// (de)serialisation is handled by the [`ContextualToolDyn`] blanket impl
/// further down.
///
/// Distinct from `rig_core::tool::Tool`, which is intentionally
/// context-free: `call` here receives a borrowed [`ToolCallCtx`] so
/// stateful tools (e.g. sub-agent delegation) can forward events back
/// into the parent run's stream. Stateless rig tools are wrapped by
/// [`StatelessAsContextual`] so the runtime sees a uniform shape.
#[async_trait]
pub trait ContextualTool: Send + Sync + 'static {
    /// The tool name surfaced to the LLM via `ToolDefinition::name`.
    ///
    /// Must be a stable identifier — the runtime keys its dispatch table
    /// on this string.
    const NAME: &'static str;

    /// Structured tool arguments. Decoded from the model-provided JSON by
    /// the [`ContextualToolDyn`] adapter.
    type Args: DeserializeOwned + Send + Sync;

    /// Structured tool output. Re-encoded to JSON for the LLM-visible
    /// tool-result message.
    type Output: Serialize + Send + Sync;

    /// Domain error type. Converted into [`crate::error::Error`] at the
    /// dyn boundary so the runtime sees a single error shape.
    type Error: Into<Error> + Send + Sync;

    /// A method returning the tool name. Defaults to [`Self::NAME`] but
    /// implementers can override for tools whose name is computed at
    /// runtime (e.g. `sub_agent_{name}`).
    fn name(&self) -> &str {
        Self::NAME
    }

    /// Human-readable tool description surfaced to the LLM.
    ///
    /// Mirrors `rig_core::tool::Tool::description`.
    fn description(&self) -> String;

    /// OpenAI-compatible JSON schema for the tool's arguments.
    ///
    /// Mirrors `rig_core::tool::Tool::parameters`.
    fn parameters(&self) -> serde_json::Value;

    /// Execute the tool with the given runtime context and structured arguments.
    ///
    /// Returns the structured tool output that will be JSON-encoded and
    /// fed back to the LLM as the `tool` role message. Errors abort this
    /// single call only; the surrounding `ToolExecutor` translates them
    /// into `ToolCallFinished { success: false, ... }` events.
    async fn call(
        &self,
        ctx: ToolCallCtx<'_>,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error>;
}

/// Object-safe wrapper around [`ContextualTool`] used by the runtime.
///
/// Mirrors `rig_core::tool::ToolDyn`: it owns no associated types so it
/// can live behind `dyn`. A blanket impl below adapts every
/// [`ContextualTool`] into this trait, transparently doing the JSON
/// (de)serialisation. Implementers should almost never touch
/// `ContextualToolDyn` directly — write [`ContextualTool`] instead.
#[async_trait]
pub trait ContextualToolDyn: Send + Sync {
    /// Returns the tool name used for runtime dispatch.
    fn name(&self) -> &str;

    /// Human-readable tool description surfaced to the LLM.
    fn description(&self) -> String;

    /// OpenAI-compatible JSON schema for the tool's arguments.
    fn parameters(&self) -> serde_json::Value;

    /// Calls the tool with the JSON-encoded arguments produced by the LLM
    /// and returns the JSON-encoded tool result fed back into the next
    /// turn.
    async fn call(&self, ctx: ToolCallCtx<'_>, args_json: &str) -> Result<String, Error>;
}

#[async_trait]
impl<T: ContextualTool> ContextualToolDyn for T {
    fn name(&self) -> &str {
        self.name()
    }

    fn description(&self) -> String {
        self.description()
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters()
    }

    async fn call(&self, ctx: ToolCallCtx<'_>, args_json: &str) -> Result<String, Error> {
        let args: T::Args = serde_json::from_str(args_json).map_err(|e| Error::ToolExecution {
            tool_name: self.name().to_string(),
            message: format!("invalid arguments JSON: {e}"),
        })?;

        let output = self.call(ctx, args).await.map_err(Into::into)?;

        let value = serde_json::to_value(output).map_err(|e| Error::ToolExecution {
            tool_name: self.name().to_string(),
            message: format!("serialize output failed: {e}"),
        })?;
        match value {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }
}

/// Adapter exposing a stateless `rig::ToolDyn` as a [`ContextualToolDyn`].
///
/// The runtime context is intentionally ignored: stateless tools have no
/// stream to forward events to. We implement [`ContextualToolDyn`]
/// *directly* — not [`ContextualTool`] — because the inner `ToolDyn` is
/// already JSON-shaped, and going through the typed trait would force a
/// pointless `String → Args → String` round-trip. The adapter caches the
/// tool name once at construction time so `name(&self) -> &str` can borrow
/// from it without re-querying the inner trait object on every dispatch.
pub struct RigToolWrapper {
    name: String,
    inner: Arc<dyn ToolDyn>,
}

impl RigToolWrapper {
    /// Wrap an `Arc<dyn ToolDyn>` so it can be registered alongside
    /// stateful tools in [`ContextualToolSet`].
    pub fn new(inner: Arc<dyn ToolDyn>) -> Self {
        let name = inner.name();
        Self { name, inner }
    }
}

#[async_trait]
impl ContextualToolDyn for RigToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> String {
        self.inner.description()
    }

    fn parameters(&self) -> serde_json::Value {
        self.inner.parameters()
    }

    async fn call(&self, _ctx: ToolCallCtx<'_>, args_json: &str) -> Result<String, Error> {
        self.inner
            .call(args_json.to_string())
            .await
            .map_err(|e| Error::ToolExecution {
                tool_name: self.name().to_string(),
                message: e.to_string(),
            })
    }
}

/// Flat registry of [`ContextualToolDyn`]s keyed by tool name.
///
/// This is the single dispatch point used by `ToolExecutor`. Both stateless
/// and stateful tools live in the same map: stateless tools are wrapped
/// upstream (typically by [`crate::tool::ToolPool`]) via
/// [`StatelessAsContextual`] before being registered.
#[derive(Clone)]
pub struct ContextualToolSet {
    tools: HashMap<String, Arc<dyn ContextualToolDyn>>,
}

impl ContextualToolSet {
    /// Build an empty contextual tool set.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a contextual tool. Same-named entries overwrite (last-wins).
    pub fn register(&mut self, tool: Arc<dyn ContextualToolDyn>) -> &mut Self {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
        self
    }

    /// Bulk-register a list of contextual tools.
    pub fn register_many(&mut self, tools: Vec<Arc<dyn ContextualToolDyn>>) -> &mut Self {
        for tool in tools {
            self.register(tool);
        }
        self
    }

    /// Whether the given `name` resolves to a registered tool.
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Return every registered tool's [`ToolDefinition`], suitable for LLM
    /// function-calling. Order is unspecified.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = Vec::with_capacity(self.tools.len());
        for tool in self.tools.values() {
            defs.push(ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description(),
                parameters: tool.parameters(),
            });
        }
        defs
    }

    /// Dispatch a single tool call against the registered tools.
    pub async fn call(
        &self,
        ctx: ToolCallCtx<'_>,
        name: &str,
        args_json: String,
    ) -> Result<String, ContextualCallError> {
        match self.tools.get(name) {
            Some(tool) => tool
                .call(ctx, &args_json)
                .await
                .map_err(ContextualCallError::Tool),
            None => Err(ContextualCallError::NotFound(name.to_string())),
        }
    }
}

impl Default for ContextualToolSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type returned by [`ContextualToolSet::call`].
///
/// `ToolExecutor` flattens this back into a string for the
/// `ToolCallFinished` audit event, so the variant taxonomy is intentionally
/// minimal.
#[derive(Debug, thiserror::Error)]
pub enum ContextualCallError {
    /// The dispatched tool returned an [`Error`].
    #[error("tool error: {0}")]
    Tool(#[from] Error),
    /// No tool with the given name is registered.
    #[error("tool not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session::AgentStreamEvent, tool::ReadFileTool};
    use serde::Deserialize;
    use serde_json::json;

    /// Echoed-back arguments returned by the typed test tool.
    #[derive(Debug, Deserialize, Serialize)]
    struct EchoArgs {
        k: i64,
    }

    /// Minimal contextual tool used in unit tests: echoes the args back as
    /// the structured output and emits a single text-delta event so we can
    /// assert the borrowed context wiring.
    struct EchoTool;

    #[async_trait]
    impl ContextualTool for EchoTool {
        const NAME: &'static str = "echo_ctx";
        type Args = EchoArgs;
        type Output = EchoArgs;
        type Error = Error;

        fn description(&self) -> String {
            "Echo back the arguments JSON".to_string()
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        async fn call(
            &self,
            ctx: ToolCallCtx<'_>,
            args: Self::Args,
        ) -> Result<Self::Output, Self::Error> {
            let _ = ctx
                .events
                .send(AgentStreamEvent::TextDelta {
                    run_id: ctx.run_id.to_string(),
                    delta: "x".to_string(),
                })
                .await;
            Ok(args)
        }
    }

    #[tokio::test]
    async fn contextual_tool_dispatches_with_borrowed_context() {
        let mut set = ContextualToolSet::new();
        set.register(Arc::new(EchoTool));

        let (tx, mut rx) = mpsc::channel(8);
        let ctx = ToolCallCtx {
            run_id: "r1",
            step: 0,
            session_id: "s1",
            events: &tx,
            user_id: "0",
            agent_id: 0,
        };
        let out = set
            .call(ctx, "echo_ctx", "{\"k\":1}".to_string())
            .await
            .expect("call should succeed");
        // Output is JSON-encoded structured payload.
        assert_eq!(out, "{\"k\":1}");
        let ev = rx.recv().await.expect("event should be forwarded");
        assert!(matches!(ev, AgentStreamEvent::TextDelta { .. }));
    }

    #[tokio::test]
    async fn unknown_tool_name_returns_not_found() {
        let set = ContextualToolSet::new();

        let (tx, _rx) = mpsc::channel(1);
        let ctx = ToolCallCtx {
            run_id: "r1",
            step: 0,
            events: &tx,
            session_id: "s1",
            user_id: "",
            agent_id: 0,
        };
        let err = set
            .call(ctx, "missing", "{}".to_string())
            .await
            .expect_err("missing tool should error");
        assert!(matches!(err, ContextualCallError::NotFound(_)));
    }

    #[tokio::test]
    async fn invalid_args_json_surfaces_as_tool_error() {
        let mut set = ContextualToolSet::new();
        set.register(Arc::new(EchoTool));

        let (tx, _rx) = mpsc::channel(1);
        let ctx = ToolCallCtx {
            run_id: "r1",
            step: 0,
            events: &tx,
            session_id: "s1",
            user_id: "",
            agent_id: 0,
        };
        let err = set
            .call(ctx, "echo_ctx", "not json".to_string())
            .await
            .expect_err("invalid JSON should error");
        match err {
            ContextualCallError::Tool(Error::ToolExecution { tool_name, message }) => {
                assert_eq!(tool_name, "echo_ctx");
                assert!(message.contains("invalid arguments JSON"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn stateless_tool_dispatches_through_adapter() {
        let mut set = ContextualToolSet::new();
        let stateless: Arc<dyn ToolDyn> = Arc::new(ReadFileTool);
        set.register(Arc::new(RigToolWrapper::new(stateless)));

        let defs = set.tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
    }

    #[tokio::test]
    async fn tool_definitions_lists_every_registered_tool() {
        let mut set = ContextualToolSet::new();
        set.register(Arc::new(EchoTool));
        let stateless: Arc<dyn ToolDyn> = Arc::new(ReadFileTool);
        set.register(Arc::new(RigToolWrapper::new(stateless)));

        let defs = set.tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"echo_ctx"));
    }
}
