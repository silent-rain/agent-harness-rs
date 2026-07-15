//! 链式智能体
//!
//! 提供的主要内容：
//! - [`ChainAgent`] - 按序执行多个子 [`Agent`]，前者输出作为后者输入

use std::sync::Arc;

use async_trait::async_trait;
use tracing::info;

use crate::{
    error::{AgentResult, Error},
    runtime::agent::traits::Agent,
    session::RunOutcome,
};

/// 链式智能体
///
/// 将多个子 [`Agent`] 按序串联，前者的 `final_answer` 作为后者的 `user_input`。
/// 任一子智能体失败则整条链失败。`total_steps` 为所有子步骤之和。
///
/// ## 典型场景
///
/// 分析 → 规划 → 执行 → 审查 的流水线任务。
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use async_trait::async_trait;
/// use agent_base::runtime::{Agent, ChainAgent};
/// use agent_base::error::AgentResult;
/// use agent_base::session::RunOutcome;
///
/// struct StepAgent(&'static str);
///
/// #[async_trait]
/// impl Agent for StepAgent {
///     fn name(&self) -> &str { self.0 }
///     async fn run(&self, _session_id: &str, input: &str) -> AgentResult<RunOutcome> {
///         Ok(RunOutcome::success(format!("[{}]{}", self.0, input), 1))
///     }
/// }
///
/// let chain = ChainAgent::new("review_pipeline", vec![
///     Arc::new(StepAgent("analyzer")) as Arc<dyn Agent>,
///     Arc::new(StepAgent("planner")),
///     Arc::new(StepAgent("executor")),
/// ]);
/// assert_eq!(chain.len(), 3);
/// ```
pub struct ChainAgent {
    /// 子智能体列表（按顺序执行）
    steps: Vec<Arc<dyn Agent>>,
    /// 智能体名称
    name: String,
}

impl ChainAgent {
    /// 创建链式智能体
    ///
    /// # Parameters
    ///
    /// * `name` - 智能体名称
    /// * `steps` - 子智能体列表，按顺序执行
    pub fn new(name: impl Into<String>, steps: Vec<Arc<dyn Agent>>) -> Self {
        Self {
            steps,
            name: name.into(),
        }
    }

    /// 子智能体数量
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[async_trait]
impl Agent for ChainAgent {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(&self, session_id: &str, user_input: &str) -> AgentResult<RunOutcome> {
        if self.steps.is_empty() {
            return Err(Error::Internal("ChainAgent 子智能体列表为空".to_string()));
        }

        let mut current_input = user_input.to_string();
        let mut total_steps: u32 = 0;

        for (index, step) in self.steps.iter().enumerate() {
            info!(
                chain_name = %self.name,
                step_index = index,
                step_name = %step.name(),
                "ChainAgent 执行子智能体"
            );

            let outcome = step.run(session_id, &current_input).await?;

            if !outcome.success {
                return Ok(RunOutcome::failure(outcome.final_answer, total_steps));
            }

            total_steps = total_steps.saturating_add(outcome.total_steps);
            current_input = outcome.final_answer;
        }

        Ok(RunOutcome::success(current_input, total_steps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 简易桩智能体，用于测试链式语义
    struct StubAgent {
        name: String,
        suffix: String,
    }

    #[async_trait]
    impl Agent for StubAgent {
        fn name(&self) -> &str {
            &self.name
        }

        async fn run(&self, _session_id: &str, user_input: &str) -> AgentResult<RunOutcome> {
            Ok(RunOutcome::success(
                format!("{}-{}", user_input, self.suffix),
                1,
            ))
        }
    }

    #[tokio::test]
    async fn chain_propagates_output_to_next_step() {
        let chain = ChainAgent::new(
            "test_chain",
            vec![
                Arc::new(StubAgent {
                    name: "a".into(),
                    suffix: "A".into(),
                }) as Arc<dyn Agent>,
                Arc::new(StubAgent {
                    name: "b".into(),
                    suffix: "B".into(),
                }),
            ],
        );

        let outcome = chain.run("sess-1", "seed").await.unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.total_steps, 2);
        assert_eq!(outcome.final_answer, "seed-A-B");
    }

    #[tokio::test]
    async fn chain_empty_returns_error() {
        let chain = ChainAgent::new("empty", vec![]);
        let err = chain.run("sess-1", "x").await;
        assert!(err.is_err());
    }
}
