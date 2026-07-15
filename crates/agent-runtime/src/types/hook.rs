//! 生命周期钩子数据类型
//!
//! 智能体运行时三层生命周期钩子（Agent / Model / Tool）所需的输入参数与返回结果。
//! 这些类型仅承载数据，与具体的 trait 行为解耦，因此放置在 `types` 层供
//! `hook` 模块及运行器消费。
//!
//! 提供的主要内容：
//! - [`BeforeAgentArgs`] / [`BeforeAgentResult`] - Agent 运行前
//! - [`AfterAgentArgs`]  / [`AfterAgentResult`]  - Agent 运行后
//! - [`BeforeModelArgs`] / [`BeforeModelResult`] - Model 调用前
//! - [`AfterModelArgs`]  / [`AfterModelResult`]  - Model 调用后
//! - [`BeforeToolArgs`]  / [`BeforeToolResult`]  - Tool 调用前
//! - [`AfterToolArgs`]   / [`AfterToolResult`]   - Tool 调用后

// ─── Agent 生命周期参数 / 结果 ─────────────────────────────────

/// Agent 运行前钩子参数
#[derive(Debug, Clone)]
pub struct BeforeAgentArgs {
    /// 会话 ID
    pub session_id: String,
    /// 用户输入
    pub input: String,
    /// 用户 ID
    pub user_id: String,
}

/// Agent 运行前钩子结果（纯观察）
///
/// 钩子仅用于审计 / 指标 / 链路追踪等观察性诉求，**不允许**修改主流程。
/// 如需输入校验拒绝，请在调用 [`crate::Agent::run`] 之前自行短路；
/// 如需鉴权 / 限流，请在更外层的网关或 Service 中实现。
#[derive(Debug, Clone, Default)]
pub struct BeforeAgentResult {}

/// Agent 运行后钩子参数
#[derive(Debug, Clone)]
pub struct AfterAgentArgs {
    /// 会话 ID
    pub session_id: String,
    /// 最终回答（可能为空，如运行出错时）
    pub final_answer: Option<String>,
    /// 运行错误（可能为空，如运行成功时）
    pub error: Option<String>,
    /// 总步数
    pub total_steps: u32,
}

/// Agent 运行后钩子结果（纯观察）
///
/// 钩子仅用于审计 / 指标 / 链路追踪等观察性诉求，**不允许**修改最终回答。
/// 如需内容过滤 / 敏感词替换 / 格式标准化，请在 [`crate::Agent::run`] 的
/// 调用方对 [`crate::runtime::RunOutcome::final_answer`] 做后处理。
#[derive(Debug, Clone, Default)]
pub struct AfterAgentResult {}
// ─── Model 生命周期参数 / 结果 ─────────────────────────────────

/// Model 调用前钩子参数
#[derive(Debug, Clone)]
pub struct BeforeModelArgs {
    /// 运行 ID
    pub run_id: String,
    /// 当前步数
    pub step: u32,
    /// 消息列表长度
    pub message_count: usize,
    /// 可用工具数量
    pub tool_count: usize,
}

/// Model 调用前钩子结果
///
/// 纯观察钩子，无返回字段。如需实现响应缓存或 Mock 回放，请改用
/// `ModelAdapter` 的装饰器实现，而非通过此钩子拦截 LLM 调用。
#[derive(Debug, Clone, Default)]
pub struct BeforeModelResult {}

/// Model 调用后钩子参数
#[derive(Debug, Clone)]
pub struct AfterModelArgs {
    /// 运行 ID
    pub run_id: String,
    /// 当前步数
    pub step: u32,
    /// 本轮聚合的文本输出（如有）
    pub text: Option<String>,
    /// 本轮聚合的工具调用数量
    pub tool_call_count: usize,
}

/// Model 调用后钩子结果
///
/// 纯观察钩子，无返回字段。如需做内容护栏或工具调用过滤，请在
/// `ToolExecutor` 的并发调度层或专用的 `OutputGuard` 中间件中实现。
#[derive(Debug, Clone, Default)]
pub struct AfterModelResult {}

// ─── Tool 生命周期参数 / 结果 ──────────────────────────────────

/// Tool 调用前钩子参数
#[derive(Debug, Clone)]
pub struct BeforeToolArgs {
    /// 运行 ID
    pub run_id: String,
    /// 当前步数
    pub step: u32,
    /// 工具名称
    pub toolname: String,
    /// 调用 ID
    pub call_id: String,
    /// 工具参数（JSON 字符串）
    pub arguments_json: String,
}

/// Tool 调用前钩子结果（纯观察）
///
/// 钩子仅用于审计 / 指标 / 链路追踪等观察性诉求，**不允许**跳过工具执行。
/// 如需根据条件屏蔽某个工具，请在 `ToolSet` 装配阶段直接不挂载该工具，
/// 或在工具实现内部做权限校验后返回错误。
#[derive(Debug, Clone, Default)]
pub struct BeforeToolResult {}

/// Tool 调用后钩子参数
#[derive(Debug, Clone)]
pub struct AfterToolArgs {
    /// 运行 ID
    pub run_id: String,
    /// 当前步数
    pub step: u32,
    /// 工具名称
    pub toolname: String,
    /// 调用 ID
    pub call_id: String,
    /// 是否成功
    pub success: bool,
    /// 工具结果摘要
    pub result_summary: Option<String>,
    /// 工具执行耗时（毫秒）
    pub duration_ms: i64,
}

/// Tool 调用后钩子结果
#[derive(Debug, Clone, Default)]
pub struct AfterToolResult {}
