//! Agent types

/// Sub-agent delegation mode
///
/// Controls how sub-agents are exposed to the parent agent's LLM.
///
/// ## Modes
///
/// - **Flat** (default): Each sub-agent is registered as an independent tool
///   (`sub_agent_{name}`). The LLM uses native function-calling to select
///   which sub-agent to invoke. Supports parallel invocation.
/// - **SingleTool**: All sub-agents are behind a single `delegate_sub_agent`
///   tool. The LLM selects via a `sub_agent_name` text parameter. Lower tool
///   count but less precise selection.
///
/// ## Choosing a Mode
///
/// | Scenario | Recommended Mode |
/// |----------|------------------|
/// | ≤ 15 sub-agents | `Flat` (better accuracy) |
/// | > 15 sub-agents | `SingleTool` (lower token overhead) |
/// | Need parallel sub-agent calls | `Flat` |
/// | Legacy compatibility | `SingleTool` |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubAgentDelegationMode {
    /// Each sub-agent is an independent tool (`sub_agent_{name}`)
    #[default]
    Flat,
    /// All sub-agents behind a single `delegate_sub_agent` tool
    SingleTool,
}
