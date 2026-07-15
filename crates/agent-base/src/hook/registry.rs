//! 钩子注册表
//!
//! 管理所有生命周期钩子的注册、存储和链式执行。
//!
//! 提供的主要内容：
//! - [`HookRegistry`] - 钩子注册表

use std::sync::Arc;

use tracing::warn;

use super::AgentHook;
use crate::{
    error::AgentResult,
    types::hook::{
        AfterAgentArgs, AfterAgentResult, AfterModelArgs, AfterModelResult, AfterToolArgs,
        AfterToolResult, BeforeAgentArgs, BeforeAgentResult, BeforeModelArgs, BeforeModelResult,
        BeforeToolArgs, BeforeToolResult,
    },
};

/// 钩子注册表
///
/// 管理智能体运行时的所有生命周期钩子，支持：
/// - 统一注册：每个 [`AgentHook`] 实现作为一个整体被注册
/// - 链式执行：按注册顺序依次调用每个钩子的对应方法
/// - 错误恢复：单个钩子失败不影响后续钩子执行（可配置）
///
/// 所有生命周期钩子均为**纯观察**语义：它们仅用于审计 / 指标 / 链路追踪，
/// 不能拦截主流程、不能覆盖输出、不能跳过工具执行。
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use agent_base::error::AgentResult;
/// use agent_base::hook::{AgentHook, HookRegistry};
/// use agent_base::types::hook::{BeforeAgentArgs, BeforeAgentResult};
///
/// struct MyHook;
///
/// #[async_trait::async_trait]
/// impl AgentHook for MyHook {
///     async fn on_before_agent(&self, _args: BeforeAgentArgs) -> AgentResult<BeforeAgentResult> {
///         Ok(BeforeAgentResult::default())
///     }
/// }
///
/// let mut registry = HookRegistry::new();
/// registry.register_hook(Arc::new(MyHook));
/// ```
#[derive(Clone, Default)]
pub struct HookRegistry {
    /// 已注册的钩子列表（每个实现按注册顺序参与所有方法）
    hooks: Vec<Arc<dyn AgentHook>>,
    /// 钩子执行出错时是否继续执行后续钩子
    continue_on_error: bool,
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookRegistry")
            .field("hook_count", &self.hooks.len())
            .field("continue_on_error", &self.continue_on_error)
            .finish()
    }
}

impl HookRegistry {
    /// 创建空的钩子注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个生命周期钩子实现
    ///
    /// 同一个钩子实例会参与其覆写过的所有方法，未覆写的方法走默认实现（无副作用）。
    pub fn register_hook(&mut self, hook: Arc<dyn AgentHook>) {
        self.hooks.push(hook);
    }

    /// 设置错误时是否继续执行后续钩子
    pub fn with_continue_on_error(mut self, value: bool) -> Self {
        self.continue_on_error = value;
        self
    }

    /// 是否为空（没有任何钩子注册）
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    /// 已注册的钩子数量
    pub fn len(&self) -> usize {
        self.hooks.len()
    }

    // ─── 链式执行方法 ───────────────────────────────────

    /// 执行 Agent 运行前钩子链（纯观察）
    ///
    /// 按注册顺序依次调用每个钩子的 [`AgentHook::on_before_agent`]，不会影响后续流程。
    ///
    /// 单个钩子出错时：
    /// - `continue_on_error = false`（默认）：立即返回错误
    /// - `continue_on_error = true`：记录警告，继续执行后续钩子
    pub async fn fire_before_agent(&self, args: BeforeAgentArgs) -> AgentResult<BeforeAgentResult> {
        if self.hooks.is_empty() {
            return Ok(BeforeAgentResult::default());
        }

        for hook in &self.hooks {
            if let Err(e) = hook.on_before_agent(args.clone()).await {
                warn!(error = %e, "BeforeAgent hook failed");
                if !self.continue_on_error {
                    return Err(e);
                }
            }
        }

        Ok(BeforeAgentResult::default())
    }

    /// 执行 Agent 运行后钩子链（纯观察）
    pub async fn fire_after_agent(&self, args: AfterAgentArgs) -> AgentResult<AfterAgentResult> {
        if self.hooks.is_empty() {
            return Ok(AfterAgentResult::default());
        }

        for hook in &self.hooks {
            if let Err(e) = hook.on_after_agent(args.clone()).await {
                warn!(error = %e, "AfterAgent hook failed");
                if !self.continue_on_error {
                    return Err(e);
                }
            }
        }

        Ok(AfterAgentResult::default())
    }

    /// 执行 Model 调用前钩子链（纯观察）
    ///
    /// 按注册顺序依次调用每个钩子的 [`AgentHook::on_before_model`]，不会影响后续流程。
    ///
    /// 单个钩子出错时：
    /// - `continue_on_error = false`（默认）：立即返回错误
    /// - `continue_on_error = true`：记录警告，继续执行后续钩子
    pub async fn fire_before_model(&self, args: BeforeModelArgs) -> AgentResult<BeforeModelResult> {
        if self.hooks.is_empty() {
            return Ok(BeforeModelResult::default());
        }

        for hook in &self.hooks {
            if let Err(e) = hook.on_before_model(args.clone()).await {
                warn!(error = %e, "BeforeModel hook failed");
                if !self.continue_on_error {
                    return Err(e);
                }
            }
        }

        Ok(BeforeModelResult::default())
    }

    /// 执行 Model 调用后钩子链（纯观察）
    pub async fn fire_after_model(&self, args: AfterModelArgs) -> AgentResult<AfterModelResult> {
        if self.hooks.is_empty() {
            return Ok(AfterModelResult::default());
        }

        for hook in &self.hooks {
            if let Err(e) = hook.on_after_model(args.clone()).await {
                warn!(error = %e, "AfterModel hook failed");
                if !self.continue_on_error {
                    return Err(e);
                }
            }
        }

        Ok(AfterModelResult::default())
    }

    /// 执行 Tool 调用前钩子链（纯观察）
    pub async fn fire_before_tool(&self, args: BeforeToolArgs) -> AgentResult<BeforeToolResult> {
        if self.hooks.is_empty() {
            return Ok(BeforeToolResult::default());
        }

        for hook in &self.hooks {
            if let Err(e) = hook.on_before_tool(args.clone()).await {
                warn!(error = %e, "BeforeTool hook failed");
                if !self.continue_on_error {
                    return Err(e);
                }
            }
        }

        Ok(BeforeToolResult::default())
    }

    /// 执行 Tool 调用后钩子链
    pub async fn fire_after_tool(&self, args: AfterToolArgs) -> AgentResult<AfterToolResult> {
        if self.hooks.is_empty() {
            return Ok(AfterToolResult::default());
        }
        for hook in &self.hooks {
            match hook.on_after_tool(args.clone()).await {
                Ok(_) => {}
                Err(e) => {
                    warn!(error = %e, "AfterTool hook failed");
                    if !self.continue_on_error {
                        return Err(e);
                    }
                }
            }
        }

        Ok(AfterToolResult::default())
    }
}
