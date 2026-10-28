//! Sub-agent delegation tool (single-tool mode)
//!
//! Wraps **all** registered sub-agents behind a single contextual tool the
//! parent agent's LLM can invoke via function calling. The LLM picks the
//! target sub-agent through the `sub_agent_name` argument.
//!
//! ## 运行期上下文
//!
//! 子 Agent 委派需要运行时上下文：父运行事件通道（把子流事件以
//! [`AgentStreamEvent::SubAgentEvent`] 透传）与父 `run_id`（关联用）。
//!
//! 二者都来自 rig 的 [`ToolContext`]：
//! - `run_id` / `session_id` / `agent_id` / `user_id` 走 inbound（[`AgentRun`]）；
//! - 事件通道走 scope（[`AgentRunScope`](crate::tool::AgentRunScope)）。
//!
//! 工具实例本身无运行期可变状态，同一份实例可被多个运行并发复用。

use std::{collections::HashMap, sync::Arc};

use futures::StreamExt;
use rig_core::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{
    Agent, AgentBuilder, LlmAgent,
    error::Error,
    model::ModelAdapter,
    runtime::RuntimeServices,
    session::{AgentStreamEvent, FinishReason, event::DelegateType},
    tool::{AgentRun, ToolContextExt, ToolPool},
    types::agent_meta::AgentMeta,
    types::runtime::{AgentBundle, ModelConfig},
};

/// Sub-agent delegation tool (single-tool mode)
///
/// One instance is registered into the parent agent's [`ToolPool`] as a
/// [`DynamicTool`](rig_core::tool::DynamicTool). At call time it builds a
/// fresh child [`LlmAgent`] from the matched [`AgentBundle`], runs the
/// delegated task, forwards every child stream event through the parent
/// event channel taken from the [`ToolContext`] scope, and returns the
/// aggregated final answer as the tool result string.
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
        run: &AgentRun,
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
        let meta = AgentMeta::sub_agent(run.run_id.clone(), run.agent_id, run.session_id.clone());

        AgentBuilder::from_bundle(child_bundle, model)
            .with_global_tool_pool(Arc::clone(&self.tool_pool))
            .with_services(self.services.clone())
            .build()
            .await
            .map(|agent| agent.with_meta(meta))
    }

    /// Build an OpenAI-compatible [`ModelAdapter`] from [`ModelConfig`].
    fn build_model(&self, binding: &ModelConfig) -> Result<Arc<dyn ModelAdapter>, Error> {
        crate::model::create_model(binding)
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

impl Tool for DelegateSubAgentTool {
    const NAME: &'static str = "delegate_sub_agent";
    type Args = DelegateSubAgentArgs;
    type Output = DelegateSubAgentOutput;
    type Error = Error;

    fn description(&self) -> String {
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
        description
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
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
        })
    }

    async fn call(
        &self,
        ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let run = ctx.agent_run()?;
        let scope = ctx.run_scope()?;
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
            parent_run_id = %run.run_id,
            run_id = run_id,
            "Delegating task to sub-agent"
        );

        let child = match self.build_child_agent(bundle, &run).await {
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

        let mut stream = match child.run_stream(&run.session_id, task) {
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
                parent_run_id: run.run_id.clone(),
                parent_agent_id: run.agent_id,
                parent_session_id: run.session_id.clone(),
                delegate_target: sub_agent_name.clone(),
                delegate_type: DelegateType::SubAgent,
                event: Box::new(event),
            };
            if scope.emit(wrapped).await.is_err() {
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

    use crate::tool::call_context;
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

        assert_eq!(DelegateSubAgentTool::NAME, "delegate_sub_agent");
        assert!(tool.description().contains("code-reviewer"));
        assert!(tool.description().contains("doc-writer"));
        assert!(tool.description().contains("Reviews code for quality"));
    }

    #[tokio::test]
    async fn call_unknown_sub_agent_returns_error_payload() {
        let configs = make_bundles();
        let tool_pool = Arc::new(ToolPool::new());
        let tool = DelegateSubAgentTool::new(&configs, tool_pool, RuntimeServices::noop());

        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        let mut ctx = call_context(AgentRun::new("run-1", "sess-1", 0, "0"), 0, tx.clone())
            .expect("测试上下文构造失败");
        let payload = Tool::call(
            &tool,
            &mut ctx,
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
