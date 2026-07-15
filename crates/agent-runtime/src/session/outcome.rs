//! Agent 运行结果与流类型
//!
//! 提供的主要内容：
//! - [`RunOutcome`] - Agent 运行结果（最终回答 + 步数 + 成功标志）
//! - [`BoxStream`] - 装箱事件流类型别名

use futures::StreamExt;

use crate::session::AgentEventStream;

use super::{AgentStreamEvent, FinishReason};

/// Agent 运行结果
///
/// 任意 [`Agent`](crate::runtime::Agent) 实现执行结束后返回该结果，
/// 承载最终回答及执行统计。
///
/// ## 与事件流的关系
///
/// `RunOutcome` 是**事件流的下采样视图**，并非独立计算的结果。
/// 真正的事实源是 [`AgentEventStream`]：
///
/// - `final_answer` 由 [`AgentStreamEvent::Answer`] 提取而来
/// - `total_steps` / `success` 由 [`AgentStreamEvent::RunFinished`] 提取
///
/// 因此调用方既可以选择走流式接口拿到完整事件流，也可以走 `run` 接口
/// 拿到聚合后的简化结果，二者天然一致，不存在对账问题。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_runtime::runtime::RunOutcome;
///
/// let outcome = RunOutcome::success("最终回答", 3);
/// assert!(outcome.success);
/// assert_eq!(outcome.total_steps, 3);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunOutcome {
    /// 最终回答
    pub final_answer: String,
    /// 执行步骤总数（具体含义由实现者解释：LLM 轮次 / 子 Agent 编号 等）
    pub total_steps: u32,
    /// 执行是否成功
    pub success: bool,
}

impl RunOutcome {
    /// 创建成功结果
    ///
    /// # Parameters
    ///
    /// * `final_answer` - 最终回答文本
    /// * `total_steps` - 总步数
    pub fn success(final_answer: impl Into<String>, total_steps: u32) -> Self {
        Self {
            final_answer: final_answer.into(),
            total_steps,
            success: true,
        }
    }

    /// 创建失败结果
    ///
    /// # Parameters
    ///
    /// * `final_answer` - 最终回答文本（可能为空或包含部分输出）
    /// * `total_steps` - 总步数
    pub fn failure(final_answer: impl Into<String>, total_steps: u32) -> Self {
        Self {
            final_answer: final_answer.into(),
            total_steps,
            success: false,
        }
    }

    /// 从 [`AgentEventStream`] 聚合出 [`RunOutcome`]
    ///
    /// 完整消费传入事件流，按以下规则下采样：
    ///
    /// - `Answer.content` → 取作 `final_answer`（每次运行至多一条）
    /// - `RunFinished.success` / `total_steps` → 直接取用
    ///
    /// 若流中未出现 `RunFinished`（例如被中断），则返回 `success=false` /
    /// `total_steps=0` 的结果，`final_answer` 仍为已接收到的 `Answer` 内容。
    ///
    /// # Parameters
    ///
    /// * `stream` - 由 [`Agent::run_stream`](crate::runtime::Agent::run_stream)
    ///   返回的事件流
    ///
    /// # Returns
    ///
    /// 聚合后的运行结果
    pub async fn from_stream(mut stream: AgentEventStream) -> Self {
        let mut final_answer = String::new();
        let mut total_steps: u32 = 0;
        let mut success = false;
        let mut finished = false;

        while let Some(event) = stream.next().await {
            match event {
                AgentStreamEvent::Answer { content, .. } => {
                    final_answer = content;
                }
                AgentStreamEvent::RunFinished {
                    total_steps: steps,
                    finish_reason,
                    ..
                } => {
                    success = matches!(finish_reason, FinishReason::FinalizeAction);
                    total_steps = steps;
                    finished = true;
                }
                _ => {}
            }
        }

        // Stream interrupted without RunFinished: maintain failure semantics
        if !finished {
            success = false;
        }

        Self {
            final_answer,
            total_steps,
            success,
        }
    }
}
