//! Flat-mode sub-agent delegation tool
//!
//! Each sub-agent is exposed as an **independent** [`ContextualTool`] named
//! `sub_agent_{name}`. This gives the LLM native function-calling selection
//! semantics (instead of the routing-via-argument used by
//! [`DelegateSubAgentTool`](super::DelegateSubAgentTool)) and supports
//! parallel invocation of multiple sub-agents.
//!
//! See the parent module docs for the trade-off matrix.

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use rig_core::{client::CompletionClient, providers::openai};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    Agent, AgentBuilder, AgentResult, LlmAgent,
    error::Error,
    model::ModelAdapter,
    runtime::RuntimeServices,
    session::{AgentStreamEvent, FinishReason, event::DelegateType},
    tool::{ContextualTool, ToolCallCtx, ToolPool},
    types::agent_meta::AgentMeta,
    types::runtime::{AgentBundle, ModelConfig},
};

/// Tool name prefix for flat sub-agent tools (`sub_agent_{name}`).
const FLAT_TOOL_PREFIX: &str = "sub_agent_";

/// Flat-mode sub-agent delegation tool
///
/// Wraps a single sub-agent as an independent [`ContextualTool`]. The tool
/// name is `sub_agent_{name}` where `{name}` is the sub-agent's configured
/// name (with hyphens preserved).
pub struct FlatSubAgentTool {
    /// The sub-agent bundle this tool instance wraps.
    bundle: AgentBundle,
    /// Generated tool name: `sub_agent_{name}`.
    tool_name: String,
    /// Shared tool pool inherited from the parent agent.
    tool_pool: Arc<ToolPool>,
    /// Runtime services inherited from the parent agent.
    ///
    /// Child agents reuse the same `session` / `knowledge` instances so
    /// their stream events and context writes land in the parent's audit
    /// trail (instead of a fresh `Noop` sink).
    services: RuntimeServices,
}

impl FlatSubAgentTool {
    /// Create a new flat sub-agent tool for a single sub-agent.
    pub fn new(bundle: AgentBundle, tool_pool: Arc<ToolPool>, services: RuntimeServices) -> Self {
        let tool_name = format!("{}{}", FLAT_TOOL_PREFIX, bundle.agent_definition.name);
        Self {
            bundle,
            tool_name,
            tool_pool,
            services,
        }
    }

    /// The generated tool name (`sub_agent_{name}`).
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    /// The wrapped sub-agent's configured name.
    pub fn sub_agent_name(&self) -> &str {
        &self.bundle.agent_definition.name
    }

    /// Build a child [`LlmAgent`] from the stored bundle, injecting parent context
    /// via [`AgentMeta`] so the child skips session lifecycle management.
    async fn build_child_agent(&self, ctx: &ToolCallCtx<'_>) -> Result<LlmAgent, Error> {
        let bundle = &self.bundle;
        let model = self.build_model(&bundle.model_config)?;

        // Apply depth control: a child with no remaining delegation budget
        // becomes a leaf node and must not spawn further sub-agents.
        let mut child_bundle = bundle.clone();
        if child_bundle.agent_definition.max_delegation_depth == 0 {
            child_bundle.sub_agent_bundles.clear();
        }

        // Inject parent context so the child skips session lifecycle management
        // and avoids overwriting the parent's Running status.
        let meta = AgentMeta::sub_agent(
            ctx.run_id.to_string(),
            ctx.agent_id,
            ctx.session_id.to_string(),
        );

        AgentBuilder::from_bundle(child_bundle, model)
            .with_global_tool_pool(Arc::clone(&self.tool_pool))
            .with_services(self.services.clone())
            .build()
            .await
            .map(|agent| agent.with_meta(meta))
    }

    fn build_model(&self, binding: &ModelConfig) -> Result<Arc<dyn ModelAdapter>, Error> {
        let client = openai::Client::builder()
            .api_key(binding.api_key.as_str())
            .base_url(binding.base_url.as_str())
            .build()
            .map_err(|e| Error::ModelCall(format!("build openai client failed: {e}")))?
            .completions_api();

        let model = client.completion_model(binding.model.as_str());
        Ok(Arc::new(model))
    }

    /// Convenience factory: produce one [`FlatSubAgentTool`] per bundle.
    pub fn from_bundles(
        bundles: &[AgentBundle],
        tool_pool: Arc<ToolPool>,
        services: RuntimeServices,
    ) -> Vec<Self> {
        bundles
            .iter()
            .map(|bundle| {
                FlatSubAgentTool::new(bundle.clone(), Arc::clone(&tool_pool), services.clone())
            })
            .collect()
    }

    /// 向 events 通道发送一个事件
    async fn emit(
        &self,
        events: &mpsc::Sender<AgentStreamEvent>,
        event: AgentStreamEvent,
    ) -> AgentResult<()> {
        events
            .send(event)
            .await
            .map_err(|e| Error::Streaming(e.to_string()))
    }
}

/// Tool input parameters for the flat sub-agent tool.
#[derive(Debug, Deserialize)]
pub struct FlatSubAgentArgs {
    /// Task description to delegate to the sub-agent.
    task: String,
}

/// Tool output for the flat sub-agent tool (LLM-visible JSON payload).
#[derive(Debug, Serialize)]
pub struct FlatSubAgentOutput {
    sub_agent_name: String,
    success: bool,
    answer: Option<String>,
    error: Option<String>,
}

#[async_trait]
impl ContextualTool for FlatSubAgentTool {
    /// Static prefix; the real per-instance tool name (`sub_agent_{code}`)
    /// is returned from [`Self::name`] override below — the runtime keys
    /// dispatch off the instance method, never the constant.
    const NAME: &'static str = FLAT_TOOL_PREFIX;
    type Args = FlatSubAgentArgs;
    type Output = FlatSubAgentOutput;
    type Error = Error;

    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> String {
        let def = &self.bundle.agent_definition;
        if def.description.is_empty() {
            format!("Delegate a task to the '{}' sub-agent.", def.name)
        } else {
            format!(
                "Delegate a task to the '{}' sub-agent: {}",
                def.name, def.description
            )
        }
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Task description to delegate to this sub-agent"
                }
            },
            "required": ["task"]
        })
    }

    async fn call(
        &self,
        ctx: ToolCallCtx<'_>,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let task = &args.task;
        let sub_agent_name = self.sub_agent_name().to_string();
        info!(
            sub_agent_name = %sub_agent_name,
            tool_name = %self.tool_name,
            task = %task,
            parent_run_id = %ctx.run_id,
            "Flat-mode: delegating task to sub-agent"
        );

        let child = match self.build_child_agent(&ctx).await {
            Ok(rt) => rt,
            Err(e) => {
                warn!(
                    sub_agent_name = %sub_agent_name,
                    error = %e,
                    "Failed to build child agent for flat sub-agent"
                );
                return Ok(FlatSubAgentOutput {
                    sub_agent_name,
                    success: false,
                    answer: None,
                    error: Some(format!("Failed to build sub-agent runtime: {e}")),
                });
            }
        };

        let mut stream = match child.run_stream(ctx.session_id, task) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    sub_agent_name = %sub_agent_name,
                    error = %e,
                    "Flat sub-agent stream initialisation failed"
                );
                return Ok(FlatSubAgentOutput {
                    sub_agent_name,
                    success: false,
                    answer: None,
                    error: Some(format!("Sub-agent execution failed: {e}")),
                });
            }
        };

        let mut final_answer = String::new();
        let mut success = false;
        let mut error: Option<String> = None;

        while let Some(event) = stream.next().await {
            if let AgentStreamEvent::Answer { content, .. } = &event {
                final_answer = content.clone();
            }
            if let AgentStreamEvent::RunFinished {
                finish_reason,
                error_message,
                ..
            } = &event
            {
                match finish_reason {
                    FinishReason::FinalizeAction => {
                        success = true;
                    }
                    _ => {
                        error = (*error_message).clone();
                    }
                }
            }

            let wrapped = AgentStreamEvent::SubAgentEvent {
                parent_run_id: ctx.run_id.to_string(),
                parent_agent_id: ctx.agent_id,
                parent_session_id: ctx.session_id.to_string(),
                delegate_target: sub_agent_name.clone(),
                delegate_type: DelegateType::SubAgent,
                event: Box::new(event),
            };
            if self.emit(ctx.events, wrapped).await.is_err() {
                warn!(
                    sub_agent_name = %sub_agent_name,
                    "Parent event channel closed; stop forwarding child events"
                );
            }
        }

        info!(
            sub_agent_name = %sub_agent_name,
            tool_name = %self.tool_name,
            success = success,
            "Flat sub-agent delegation completed"
        );

        Ok(FlatSubAgentOutput {
            sub_agent_name,
            success,
            answer: Some(final_answer),
            error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::runtime::{AgentDefinition, ModelConfig};

    fn make_bundle(name: &str, description: &str) -> AgentBundle {
        AgentBundle {
            agent_definition: AgentDefinition {
                id: 0,
                user_id: "0".to_string(),
                name: name.to_string(),
                description: description.to_string(),
                system_prompt: format!("You are {name}."),
                welcome_message: None,
                max_delegation_depth: 0,
            },
            model_config: ModelConfig::default(),
            ..Default::default()
        }
    }

    #[test]
    fn tool_name_generation() {
        let tool_pool = Arc::new(ToolPool::new());
        let bundle = make_bundle("code-reviewer", "Reviews code for quality");
        let tool = FlatSubAgentTool::new(bundle, tool_pool, RuntimeServices::noop());

        assert_eq!(tool.tool_name(), "sub_agent_code-reviewer");
        assert_eq!(tool.sub_agent_name(), "code-reviewer");
    }

    #[test]
    fn from_bundles_creates_multiple_tools() {
        let tool_pool = Arc::new(ToolPool::new());
        let bundles = vec![
            make_bundle("code-reviewer", "Reviews code"),
            make_bundle("doc-writer", "Writes docs"),
            make_bundle("test-gen", "Generates tests"),
        ];

        let tools = FlatSubAgentTool::from_bundles(&bundles, tool_pool, RuntimeServices::noop());

        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].tool_name(), "sub_agent_code-reviewer");
        assert_eq!(tools[1].tool_name(), "sub_agent_doc-writer");
        assert_eq!(tools[2].tool_name(), "sub_agent_test-gen");
    }

    #[tokio::test]
    async fn definition_contains_description() {
        let tool_pool = Arc::new(ToolPool::new());
        let bundle = make_bundle("code-reviewer", "Reviews code for quality");
        let tool = FlatSubAgentTool::new(bundle, tool_pool, RuntimeServices::noop());

        assert_eq!(tool.name(), "sub_agent_code-reviewer");
        assert!(tool.description().contains("code-reviewer"));
        assert!(tool.description().contains("Reviews code for quality"));
    }

    #[tokio::test]
    async fn definition_empty_description() {
        let tool_pool = Arc::new(ToolPool::new());
        let bundle = make_bundle("helper", "");
        let tool = FlatSubAgentTool::new(bundle, tool_pool, RuntimeServices::noop());

        assert_eq!(tool.name(), "sub_agent_helper");
        assert!(tool.description().contains("helper"));
        assert!(tool.description().contains("Delegate a task"));
    }
}
