//! rig-core 工具桥接与调用上下文
//!
//! rig-core 0.45 的工具契约由三部分组成：
//!
//! - [`Tool`]：类型化工具（`Args` / `Output` / `Error`），`call` 接收
//!   `&mut ToolContext`；
//! - [`DynamicTool`]：对象安全形态（运行期名称 / 闭包回调），运行期统一持有的类型；
//! - [`ToolContext`]：每次调用的上下文容器，由两类槽位组成 ——
//!   - **inbound**：`Serialize + DeserializeOwned` 的纯数据（[`AgentRun`] / [`StepIndex`]）；
//!   - **scopes**：不可序列化的活体句柄（[`AgentRunScope`] 持有父运行的事件发送端）。
//!
//! 本模块负责本仓库与上述契约之间的全部胶水，不引入第二套工具抽象：
//!
//! - [`tool_name_of`]：运行时字符串名称 → [`ToolName`]（空名兜底）；
//! - [`dynamic_tool`] / [`dynamic_tool_named`]：把类型化 [`Tool`] 适配为
//!   [`DynamicTool`]，并把调用方传入的 [`ToolContext`] 原样交给工具；
//! - [`call_context`]：装配一次工具调用所需的上下文（inbound + scope）；
//! - [`ToolContextExt`]：工具侧读取上下文的便捷访问器；
//! - [`tool_output_to_string`]：把 [`ToolOutput`] 折叠为本仓库工具结果的字符串形态。

use std::sync::Arc;

use rig_core::{
    message::ToolName,
    tool::{
        ContextValue, DynamicTool, IntoToolOutput, Tool, ToolContext, ToolContextError,
        ToolExecutionError, ToolOutput,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{error::Error, session::AgentStreamEvent};

/// 把运行时名称转为 [`ToolName`]。
///
/// 空名称在 rig 中非法，此处兜底为 `unnamed_tool`，避免动态工具
/// （子 Agent、MCP 远端工具）因配置缺失直接 panic。
///
/// # Parameters
///
/// * `name` - 运行时工具名称
pub fn tool_name_of(name: &str) -> ToolName {
    ToolName::new(name)
        .unwrap_or_else(|_| ToolName::new("unnamed_tool").expect("兜底名称非空，构造必然成功"))
}

/// 单次运行的标识信息（`ToolContext` inbound 槽位）。
///
/// 全部字段可序列化，因此走 inbound；事件通道等活体句柄见 [`AgentRunScope`]。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    /// 本次运行的运行 ID（每次 `run_stream` 生成一个）。
    pub run_id: String,
    /// 所属会话 ID。
    pub session_id: String,
    /// 当前智能体 ID。
    pub agent_id: i64,
    /// 当前用户 ID（支持多种身份系统）。
    pub user_id: String,
}

impl AgentRun {
    /// 构造一次运行的标识信息。
    pub fn new(
        run_id: impl Into<String>,
        session_id: impl Into<String>,
        agent_id: i64,
        user_id: impl Into<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            session_id: session_id.into(),
            agent_id,
            user_id: user_id.into(),
        }
    }
}

impl ContextValue for AgentRun {
    const KEY: &'static str = "agent.run";
}

/// 当前 ReAct 步序（`ToolContext` inbound 槽位）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StepIndex(pub u32);

impl ContextValue for StepIndex {
    const KEY: &'static str = "agent.step";
}

/// 运行期活体作用域（`ToolContext` scope 槽位）。
///
/// `ToolContext` 的 inbound 只能是可序列化数据，事件发送端这类句柄经
/// `with_scope` 挂入，工具侧用 [`ToolContextExt::run_scope`] 取回 `Arc`。
#[derive(Clone)]
pub struct AgentRunScope {
    /// 父运行的事件通道：需要透传子事件的工具（子 Agent / A2A 委派）向它发送。
    events: mpsc::Sender<AgentStreamEvent>,
}

impl AgentRunScope {
    /// 用父运行的事件发送端构造作用域。
    pub fn new(events: mpsc::Sender<AgentStreamEvent>) -> Self {
        Self { events }
    }

    /// 父运行的事件发送端。
    pub fn events(&self) -> &mpsc::Sender<AgentStreamEvent> {
        &self.events
    }

    /// 向父运行的事件通道发送一个事件；通道关闭视为流式失败。
    pub async fn emit(&self, event: AgentStreamEvent) -> Result<(), Error> {
        self.events
            .send(event)
            .await
            .map_err(|e| Error::Streaming(e.to_string()))
    }
}

/// [`ToolContext`] 的便捷读取扩展。
///
/// 工具实现里用 `ctx.agent_run()?` / `ctx.step_index()?` / `ctx.run_scope()?`
/// 取代散落的 `require::<T>()` 调用。
pub trait ToolContextExt {
    /// 读取本次运行的身份信息。
    fn agent_run(&self) -> Result<AgentRun, Error>;
    /// 读取当前 ReAct 步序；未注入时视为第 0 步。
    fn step_index(&self) -> Result<u32, Error>;
    /// 读取运行期作用域（父运行事件通道）。
    fn run_scope(&self) -> Result<Arc<AgentRunScope>, Error>;
}

impl ToolContextExt for ToolContext {
    fn agent_run(&self) -> Result<AgentRun, Error> {
        Ok(self.require::<AgentRun>()?)
    }

    fn step_index(&self) -> Result<u32, Error> {
        Ok(self.get::<StepIndex>()?.map(|step| step.0).unwrap_or(0))
    }

    fn run_scope(&self) -> Result<Arc<AgentRunScope>, Error> {
        self.scope::<AgentRunScope>()
            .ok_or_else(|| Error::ToolExecution {
                tool_name: "<context>".to_string(),
                message: "运行期作用域缺失（AgentRunScope）".to_string(),
            })
    }
}

/// 装配一次工具调用的 [`ToolContext`]：inbound 放标识信息，scope 放事件通道。
///
/// # Parameters
///
/// * `run` - 本次运行的身份信息
/// * `step` - 当前 ReAct 步序
/// * `events` - 父运行的事件发送端
pub fn call_context(
    run: AgentRun,
    step: u32,
    events: mpsc::Sender<AgentStreamEvent>,
) -> Result<ToolContext, Error> {
    let mut ctx = ToolContext::new();
    ctx.insert(run)?;
    ctx.insert(StepIndex(step))?;
    Ok(ctx.with_scope(Arc::new(AgentRunScope::new(events))))
}

impl From<ToolContextError> for Error {
    fn from(err: ToolContextError) -> Self {
        Error::ToolExecution {
            tool_name: "<context>".to_string(),
            message: err.to_string(),
        }
    }
}

/// 把类型化 [`Tool`] 适配为可运行时调用的 [`DynamicTool`]。
///
/// 工具名取自 [`Tool::NAME`]；调用方传入的 [`ToolContext`] 原样透传，
/// 因此 inbound 值与 scope 对工具可见。
///
/// # Parameters
///
/// * `tool` - 任意实现了 rig [`Tool`] 的工具实例
pub fn dynamic_tool<T>(tool: T) -> DynamicTool
where
    T: Tool + 'static,
{
    dynamic_tool_named(T::NAME, tool)
}

/// 同 [`dynamic_tool`]，但用运行期计算出的名称覆盖 [`Tool::NAME`]。
///
/// 用于“一个工具实例一个名字”的场景（如 `sub_agent_{name}` /
/// `a2a_{service}`）：rig 的 [`Tool::NAME`] 是常量，运行期名称只能在这一层给定。
///
/// # Parameters
///
/// * `name` - 覆盖后的运行时工具名称
/// * `tool` - 任意实现了 rig [`Tool`] 的工具实例
pub fn dynamic_tool_named<T>(name: &str, tool: T) -> DynamicTool
where
    T: Tool + 'static,
{
    let name = tool_name_of(name);
    let description = tool.description();
    let parameters = tool.parameters();
    let shared = Arc::new(tool);

    DynamicTool::new_with_context(
        name,
        description,
        parameters,
        move |context: &mut ToolContext, args: serde_json::Value| {
            let tool = Arc::clone(&shared);
            Box::pin(async move {
                let args: T::Args = serde_json::from_value(args)
                    .map_err(|e| ToolExecutionError::invalid_args(e.to_string()))?;
                match Tool::call(&*tool, context, args).await {
                    Ok(output) => output.into_tool_output(),
                    Err(err) => Err(Tool::map_error(&*tool, err)),
                }
            })
        },
    )
}

/// 把 [`ToolOutput`] 折叠为工具结果字符串。
///
/// 单一文本块直接取其文本；其余情况（JSON、多模态）序列化为 JSON 文本，
/// 与 LLM 侧工具结果的消费方式保持一致。
///
/// # Parameters
///
/// * `output` - rig 规范化后的工具输出
pub fn tool_output_to_string(output: &ToolOutput) -> String {
    match output.as_text() {
        Some(text) => text.to_string(),
        None => serde_json::to_string(output).unwrap_or_else(|_| String::new()),
    }
}
