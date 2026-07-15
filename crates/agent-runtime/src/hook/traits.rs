//! 智能体生命周期钩子 trait
//!
//! 提供的主要内容：
//! - [`AgentHook`] - 统一钩子 trait，覆盖 Agent / Model / Tool 三层共 6 个钩子

use crate::{
    error::AgentResult,
    types::hook::{
        AfterAgentArgs, AfterAgentResult, AfterModelArgs, AfterModelResult, AfterToolArgs,
        AfterToolResult, BeforeAgentArgs, BeforeAgentResult, BeforeModelArgs, BeforeModelResult,
        BeforeToolArgs, BeforeToolResult,
    },
};

/// 智能体生命周期统一钩子 trait
///
/// 单一 trait 覆盖 Agent / Model / Tool 三层共 6 个生命周期钩子。
/// 所有方法均提供默认实现（返回对应 `Result::default()`），
/// 实现者仅需覆写自己关心的钩子。
///
/// 与"每个钩子一个 trait"的设计相比，本 trait 的优势：
/// - 单一注册入口：一个实现即可关注多个钩子，天然共享内部状态（计数器、metrics、span 等）
/// - API 面小：Builder 只需 `on_hook`，Registry 只需 `register_hook`
/// - 执行顺序直观：同一实现的所有钩子总是按其在列表中的位置被调用
///
/// # Examples
///
/// ```rust,no_run
/// use agent_runtime::error::AgentResult;
/// use agent_runtime::hook::AgentHook;
/// use agent_runtime::types::hook::{
///     BeforeAgentArgs, BeforeAgentResult, AfterToolArgs, AfterToolResult,
/// };
///
/// struct Auditor;
///
/// #[async_trait::async_trait]
/// impl AgentHook for Auditor {
///     async fn on_before_agent(&self, args: BeforeAgentArgs) -> AgentResult<BeforeAgentResult> {
///         tracing::info!(user_id = args.user_id, "agent run started");
///         Ok(BeforeAgentResult::default())
///     }
///
///     async fn on_after_tool(&self, args: AfterToolArgs) -> AgentResult<AfterToolResult> {
///         tracing::info!(tool = %args.toolname, success = args.success, "tool invoked");
///         Ok(AfterToolResult::default())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait AgentHook: Send + Sync + 'static {
    /// Agent 运行前钩子（纯观察）。
    ///
    /// 默认实现：直接返回空结果。
    ///
    /// 适用场景：运行起点的 metrics / span / 审计记录。
    /// **不** 用于拦截 Agent 执行。如需输入校验 / 鉴权 / 限流，请在
    /// 调用 [`crate::Agent::run`] 之前由外层服务自行短路。
    async fn on_before_agent(&self, _args: BeforeAgentArgs) -> AgentResult<BeforeAgentResult> {
        Ok(BeforeAgentResult::default())
    }

    /// Agent 运行后钩子（纯观察）。
    ///
    /// 默认实现：直接返回空结果。
    ///
    /// 适用场景：记录运行总耗时 / 总步数 / 成功状态等 metrics。
    /// **不** 用于覆盖最终回答。如需内容过滤 / 敏感词替换 / 格式标准化，
    /// 请在调用方拿到 [`crate::runtime::RunOutcome`] 之后自行后处理。
    async fn on_after_agent(&self, _args: AfterAgentArgs) -> AgentResult<AfterAgentResult> {
        Ok(AfterAgentResult::default())
    }

    /// Model 调用前钩子（纯观察）。
    ///
    /// 默认实现：直接返回空结果。
    ///
    /// 适用场景：LLM 调用前的 metrics / span / 计数。
    /// **不** 用于拦截调用。如需响应缓存或 Mock 回放，请以
    /// `ModelAdapter` 装饰器形式实现；如需拦截整次运行，请使用 `on_before_agent`。
    async fn on_before_model(&self, _args: BeforeModelArgs) -> AgentResult<BeforeModelResult> {
        Ok(BeforeModelResult::default())
    }

    /// Model 调用后钩子（纯观察）。
    ///
    /// 默认实现：直接返回空结果。
    ///
    /// 适用场景：记录 token 数、tool_call 数、推理时延等 metrics。
    /// **不** 用于改写输出。如需内容护栏或工具调用过滤，请在
    /// `ToolExecutor` 的并发调度层或专用的 `OutputGuard` 中间件中实现。
    async fn on_after_model(&self, _args: AfterModelArgs) -> AgentResult<AfterModelResult> {
        Ok(AfterModelResult::default())
    }

    /// Tool 调用前钩子（纯观察）。
    ///
    /// 默认实现：直接返回空结果。
    ///
    /// 适用场景：工具调用起点的 metrics / span / 审计记录。
    /// **不** 用于跳过工具执行。如需权限校验拒绝，请在工具实现内部进行。
    async fn on_before_tool(&self, _args: BeforeToolArgs) -> AgentResult<BeforeToolResult> {
        Ok(BeforeToolResult::default())
    }

    /// Tool 调用后钩子。
    ///
    /// 默认实现：直接返回空结果。
    ///
    /// 当前版本保留扩展性，未提供结果替换能力。
    async fn on_after_tool(&self, _args: AfterToolArgs) -> AgentResult<AfterToolResult> {
        Ok(AfterToolResult::default())
    }
}
