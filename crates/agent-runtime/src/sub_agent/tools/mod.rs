//! Sub-agent tools
//!
//! Provides tool implementations for sub-agent delegation.
//!
//! Two delegation modes are available:
//!
//! - **Single-tool mode** ([`DelegateSubAgentTool`]): One tool serves all sub-agents;
//!   the LLM selects via a `sub_agent_name` parameter.
//! - **Flat mode** ([`FlatSubAgentTool`]): Each sub-agent is exposed as an independent
//!   tool (`sub_agent_{name}`), giving the LLM native function-calling selection.
//!
//! Provided types:
//! - [`DelegateSubAgentTool`] - Single-tool mode (all sub-agents behind one tool)
//! - [`FlatSubAgentTool`] - Flat mode (one tool per sub-agent)

mod delegate_sub_agent;
mod flat_sub_agent;

pub use delegate_sub_agent::DelegateSubAgentTool;
pub use flat_sub_agent::FlatSubAgentTool;
