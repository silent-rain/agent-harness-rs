//! A2A Agent 工具
//!
//! 将远程 A2A Agent 服务包装为 [`ContextualTool`]，使其可以被 LLM 通过
//! function calling 直接调用，与本地子智能体工具（`FlatSubAgentTool`）完全对称。
//!
//! 提供的主要内容：
//! - [`A2aAgentTool`] - 将 `A2AAgent` 包装为 `ContextualTool`

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{
    Agent,
    a2a::client::A2AAgent,
    error::Error,
    session::{AgentStreamEvent, FinishReason, event::DelegateType},
    tool::{ContextualTool, ToolCallCtx},
    types::a2a::A2aServiceConfig,
};

/// 工具名称前缀（`a2a_{service_name}`）
const A2A_TOOL_PREFIX: &str = "a2a_";

/// A2A Agent 工具
///
/// 将远程 A2A Agent 服务包装为 [`ContextualTool`]，工具名为 `a2a_{service_name}`。
/// LLM 可以像调用本地工具一样调用远程 A2A Agent，底层协议细节完全透明。
///
/// ## 工作原理
///
/// ```text
/// LLM 调用 a2a_weather_agent(task="...")
///   ↓
/// A2aAgentTool::call()
///   ↓
/// A2AAgent::run_stream(session_id, task)  ← 通过 A2A 协议调用远程 Agent
///   ↓
/// 收集流式事件，转发 SubAgentEvent 给父 Agent
///   ↓
/// 返回 { success, answer } 给 LLM
/// ```
pub struct A2aAgentTool {
    /// 工具名称（`a2a_{service_name}`）
    tool_name: String,
    /// 工具描述（LLM 可见）
    description: String,
    /// 已连接的远程 A2A Agent
    agent: Arc<A2AAgent>,
}

impl A2aAgentTool {
    /// 从已连接的 A2AAgent 和配置创建工具
    ///
    /// # Parameters
    ///
    /// * `agent` - 已完成 AgentCard 发现的 A2AAgent 实例
    /// * `config` - A2A 服务配置（用于工具名称和描述）
    pub fn new(agent: A2AAgent, config: &A2aServiceConfig) -> Self {
        let tool_name = format!("{}{}", A2A_TOOL_PREFIX, config.service_name);
        // 优先使用配置中的描述，其次从 AgentCard 获取
        let description = if !config.description.is_empty() {
            config.description.clone()
        } else {
            agent.agent_card().description.clone()
        };

        Self {
            tool_name,
            description,
            agent: Arc::new(agent),
        }
    }

    /// 工具名称（`a2a_{service_name}`）
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }
}

/// 工具输入参数
#[derive(Debug, Deserialize)]
pub struct A2aAgentArgs {
    /// 委派给远程 Agent 的任务描述
    task: String,
}

/// 工具输出（LLM 可见的 JSON 载荷）
#[derive(Debug, Serialize)]
pub struct A2aAgentOutput {
    /// 远程 Agent 名称
    agent_name: String,
    /// 是否成功
    success: bool,
    /// 最终回答
    answer: Option<String>,
    /// 错误信息
    error: Option<String>,
}

#[async_trait]
impl ContextualTool for A2aAgentTool {
    const NAME: &'static str = A2A_TOOL_PREFIX;
    type Args = A2aAgentArgs;
    type Output = A2aAgentOutput;
    type Error = Error;

    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> String {
        if self.description.is_empty() {
            format!(
                "Delegate a task to the remote A2A agent '{}'.",
                self.agent.name()
            )
        } else {
            format!(
                "Delegate a task to the remote A2A agent '{}': {}",
                self.agent.name(),
                self.description
            )
        }
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Task description to delegate to this remote A2A agent"
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
        let agent_name = self.agent.name().to_string();
        info!(
            agent_name = %agent_name,
            tool_name = %self.tool_name,
            task = %task,
            parent_run_id = %ctx.run_id,
            "A2A: delegating task to remote agent"
        );

        let mut stream = match self.agent.run_stream(ctx.session_id, task) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    agent_name = %agent_name,
                    error = %e,
                    "A2A: stream initialisation failed"
                );
                return Ok(A2aAgentOutput {
                    agent_name,
                    success: false,
                    answer: None,
                    error: Some(format!("A2A agent execution failed: {e}")),
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

            // 将远程 Agent 的事件包装为 SubAgentEvent 转发给父 Agent
            let wrapped = AgentStreamEvent::SubAgentEvent {
                parent_run_id: ctx.run_id.to_string(),
                parent_agent_id: ctx.agent_id,
                parent_session_id: ctx.session_id.to_string(),
                delegate_target: agent_name.clone(),
                delegate_type: DelegateType::A2aAgent,
                event: Box::new(event),
            };
            if ctx.events.send(wrapped).await.is_err() {
                warn!(
                    agent_name = %agent_name,
                    "Parent event channel closed; stop forwarding A2A agent events"
                );
            }
        }

        info!(
            agent_name = %agent_name,
            tool_name = %self.tool_name,
            success = success,
            "A2A: remote agent delegation completed"
        );

        Ok(A2aAgentOutput {
            agent_name,
            success,
            answer: Some(final_answer),
            error,
        })
    }
}
