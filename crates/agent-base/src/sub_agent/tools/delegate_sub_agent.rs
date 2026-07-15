//! Sub-agent delegation tool (single-tool mode)
//!
//! Wraps **all** registered sub-agents behind a single contextual tool the
//! parent agent's LLM can invoke via function calling. The LLM picks the
//! target sub-agent through the `sub_agent_name` argument.
//!
//! ## Why a [`ContextualTool`] (not `rig::Tool`)?
//!
//! Sub-agent delegation needs runtime context that `rig::Tool` cannot
//! carry: the parent event sender (to forward child stream events as
//! [`AgentStreamEvent::SubAgentEvent`]) and the parent `run_id` (for
//! correlation). Earlier revisions stashed both in
//! `Arc<RwLock<Option<...>>>` slots; they are now passed as borrowed
//! [`ToolCallCtx`] fields at call time \u2014 no shared mutable state.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use futures::StreamExt;
use rig_core::{client::CompletionClient, completion::ToolDefinition, providers::openai};
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

/// Sub-agent delegation tool (single-tool mode)
///
/// One instance is registered into the parent agent's [`ToolPool`] as a
/// [`ContextualTool`]. At call time it builds a fresh child [`LlmAgent`]
/// from the matched [`AgentBundle`], runs the delegated task, forwards
/// every child stream event through the borrowed parent sender, and
/// returns the aggregated final answer as the tool result string.
pub struct DelegateSubAgentTool {
    /// Materialised sub-agent bundles indexed by name for O(1) lookup.
    sub_agents: Arc<HashMap<String, AgentBundle>>,
    /// Shared tool pool inherited from the parent agent.
    ///
    /// Child agents reuse the same underlying tool instances (`Arc`)
    /// instead of rebuilding the candidate pool on every delegation. The
    /// snapshot's `tool_configs` still acts as the per-child whitelist.
    tool_pool: Arc<ToolPool>,
    /// Runtime services inherited from the parent agent.
    ///
    /// Child agents reuse the same `session` / `knowledge` instances so
    /// their stream events and context writes land in the parent's audit
    /// trail (instead of a fresh `Noop` sink).
    services: RuntimeServices,
}

impl DelegateSubAgentTool {
    /// Create a new delegation tool from a list of pre-materialised bundles.
    pub fn new(
        bundles: &[AgentBundle],
        tool_pool: Arc<ToolPool>,
        services: RuntimeServices,
    ) -> Self {
        let sub_agents: HashMap<String, AgentBundle> = bundles
            .iter()
            .map(|b| (b.agent_definition.name.clone(), b.clone()))
            .collect();

        Self {
            sub_agents: Arc::new(sub_agents),
            tool_pool,
            services,
        }
    }

    /// Sorted list of registered sub-agent names (helpful for diagnostics).
    pub fn sub_agent_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.sub_agents.keys().cloned().collect();
        names.sort();
        names
    }

    /// Build a child [`LlmAgent`] from a pre-materialised [`AgentBundle`], injecting
    /// parent context via [`AgentMeta`] so the child skips session lifecycle management.
    async fn build_child_agent(
        &self,
        bundle: &AgentBundle,
        ctx: &ToolCallCtx<'_>,
    ) -> Result<LlmAgent, Error> {
        let model = self.build_model(&bundle.model_config)?;

        // Depth control: a child with no remaining delegation budget becomes
        // a leaf node and must not spawn further sub-agents.
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

    /// Build an OpenAI-compatible [`ModelAdapter`] from [`ModelConfig`].
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

/// Tool input parameters
#[derive(Debug, Deserialize)]
pub struct DelegateSubAgentArgs {
    /// Name of the sub-agent to delegate to.
    sub_agent_name: String,
    /// Task description to delegate.
    task: String,
}

/// Tool output (serialised as the LLM-visible tool-result JSON).
#[derive(Debug, Serialize, Deserialize)]
pub struct DelegateSubAgentOutput {
    sub_agent_name: String,
    success: bool,
    answer: Option<String>,
    error: Option<String>,
}

#[async_trait]
impl ContextualTool for DelegateSubAgentTool {
    const NAME: &'static str = "delegate_sub_agent";
    type Args = DelegateSubAgentArgs;
    type Output = DelegateSubAgentOutput;
    type Error = Error;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let sub_agent_list: Vec<String> = self
            .sub_agents
            .values()
            .map(|cfg| {
                let def = &cfg.agent_definition;
                if def.description.is_empty() {
                    format!("- {}", def.name)
                } else {
                    format!("- {}: {}", def.name, def.description)
                }
            })
            .collect();

        let description = format!(
            "Delegate a task to a sub-agent. Available sub-agents:\n{}",
            sub_agent_list.join("\n")
        );

        ToolDefinition {
            name: Self::NAME.to_string(),
            description,
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "sub_agent_name": {
                        "type": "string",
                        "description": "Name of the sub-agent to delegate to"
                    },
                    "task": {
                        "type": "string",
                        "description": "Task description to delegate to the sub-agent"
                    }
                },
                "required": ["sub_agent_name", "task"]
            }),
        }
    }

    async fn call(
        &self,
        ctx: ToolCallCtx<'_>,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let sub_agent_name = &args.sub_agent_name;
        let task = &args.task;
        let run_id = uuid::Uuid::new_v4().to_string();

        let bundle = match self.sub_agents.get(sub_agent_name.as_str()) {
            Some(b) => b,
            None => {
                return Ok(DelegateSubAgentOutput {
                    sub_agent_name: sub_agent_name.clone(),
                    success: false,
                    answer: None,
                    error: Some(format!(
                        "Sub-agent '{}' not found. Available: {}",
                        sub_agent_name,
                        self.sub_agent_names().join(", ")
                    )),
                });
            }
        };

        info!(
            sub_agent_name = %sub_agent_name,
            task = %task,
            parent_run_id = %ctx.run_id,
            run_id = run_id,
            "Delegating task to sub-agent"
        );

        let child = match self.build_child_agent(bundle, &ctx).await {
            Ok(rt) => rt,
            Err(e) => {
                warn!(
                    sub_agent_name = %sub_agent_name,
                    error = %e,
                    "Failed to build child agent for sub-agent"
                );
                return Ok(DelegateSubAgentOutput {
                    sub_agent_name: sub_agent_name.clone(),
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
                    "Sub-agent stream initialisation failed"
                );
                return Ok(DelegateSubAgentOutput {
                    sub_agent_name: sub_agent_name.clone(),
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
            success = success,
            "Sub-agent delegation completed"
        );

        Ok(DelegateSubAgentOutput {
            sub_agent_name: sub_agent_name.clone(),
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

    fn make_bundles() -> Vec<AgentBundle> {
        vec![
            make_bundle("code-reviewer", "Reviews code for quality"),
            make_bundle("doc-writer", "Generates documentation"),
        ]
    }

    #[test]
    fn available_sub_agents_sorted() {
        let configs = make_bundles();
        let tool_pool = Arc::new(ToolPool::new());
        let tool = DelegateSubAgentTool::new(&configs, tool_pool, RuntimeServices::noop());
        assert_eq!(tool.sub_agent_names(), vec!["code-reviewer", "doc-writer"]);
    }

    #[test]
    fn empty_sub_agents() {
        let tool_pool = Arc::new(ToolPool::new());
        let tool = DelegateSubAgentTool::new(&[], tool_pool, RuntimeServices::noop());
        assert!(tool.sub_agent_names().is_empty());
    }

    #[tokio::test]
    async fn definition_includes_sub_agent_list() {
        let configs = make_bundles();
        let tool_pool = Arc::new(ToolPool::new());
        let tool = DelegateSubAgentTool::new(&configs, tool_pool, RuntimeServices::noop());
        let def = tool.definition(String::new()).await;

        assert_eq!(def.name, "delegate_sub_agent");
        assert!(def.description.contains("code-reviewer"));
        assert!(def.description.contains("doc-writer"));
        assert!(def.description.contains("Reviews code for quality"));
    }

    #[tokio::test]
    async fn call_unknown_sub_agent_returns_error_payload() {
        let configs = make_bundles();
        let tool_pool = Arc::new(ToolPool::new());
        let tool = DelegateSubAgentTool::new(&configs, tool_pool, RuntimeServices::noop());

        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        let ctx = ToolCallCtx {
            run_id: "run-1",
            step: 0,
            events: &tx,
            session_id: "sess-1",
            agent_id: 0,
            user_id: "0",
        };
        let payload = ContextualTool::call(
            &tool,
            ctx,
            DelegateSubAgentArgs {
                sub_agent_name: "nonexistent".to_string(),
                task: "do something".to_string(),
            },
        )
        .await
        .expect("contextual call returns Ok with output payload, even on logical failure");

        assert!(!payload.success);
        let err = payload.error.unwrap_or_default();
        assert!(err.contains("nonexistent"));
        assert!(err.contains("not found"));
    }
}
