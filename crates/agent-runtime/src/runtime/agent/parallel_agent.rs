//! 并行智能体
//!
//! 提供的主要内容：
//! - [`ParallelAgent`] - 并发执行多个子 [`Agent`]，结果按定义顺序拼接

use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::{
    error::{AgentResult, Error},
    runtime::agent::traits::Agent,
    session::RunOutcome,
};

/// 并行智能体
///
/// 将多个子 [`Agent`] 并发执行，结果按子智能体定义的顺序拼接（`\n---\n` 分隔）。
/// 任一子智能体抛出错误即整体失败。
///
/// ## 典型场景
///
/// 多视角分析、独立任务同时推进。
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use agent_runtime::error::AgentResult;
/// use agent_runtime::runtime::{Agent, ParallelAgent};
/// use agent_runtime::session::RunOutcome;
///
/// struct ReviewAgent { name: String }
///
/// #[async_trait::async_trait]
/// impl Agent for ReviewAgent {
///     fn name(&self) -> &str { &self.name }
///     async fn run(&self, _session_id: &str, input: &str) -> AgentResult<RunOutcome> {
///         Ok(RunOutcome::success(format!("[{}] {}", self.name, input), 1))
///     }
/// }
///
/// let parallel = ParallelAgent::new("multi_review", vec![
///     Arc::new(ReviewAgent { name: "reviewer_a".into() }) as Arc<dyn Agent>,
///     Arc::new(ReviewAgent { name: "reviewer_b".into() }),
///     Arc::new(ReviewAgent { name: "reviewer_c".into() }),
/// ])
/// .with_max_concurrency(2);
/// ```
pub struct ParallelAgent {
    /// 子智能体列表
    branches: Vec<Arc<dyn Agent>>,
    /// 最大并发度（0 表示不限）
    max_concurrency: usize,
    /// 智能体名称
    name: String,
}

impl ParallelAgent {
    /// 创建并行智能体
    ///
    /// # Parameters
    ///
    /// * `name` - 智能体名称
    /// * `branches` - 子智能体列表
    pub fn new(name: impl Into<String>, branches: Vec<Arc<dyn Agent>>) -> Self {
        Self {
            branches,
            max_concurrency: 0,
            name: name.into(),
        }
    }

    /// 设置最大并发度（0 表示不限）
    pub fn with_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = max_concurrency;
        self
    }

    /// 子智能体数量
    pub fn len(&self) -> usize {
        self.branches.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }
}

#[async_trait]
impl Agent for ParallelAgent {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(&self, session_id: &str, user_input: &str) -> AgentResult<RunOutcome> {
        if self.branches.is_empty() {
            return Err(Error::Internal(
                "ParallelAgent 子智能体列表为空".to_string(),
            ));
        }

        let semaphore = if self.max_concurrency > 0 {
            Some(Arc::new(Semaphore::new(self.max_concurrency)))
        } else {
            None
        };

        let mut futures = Vec::with_capacity(self.branches.len());

        for (index, branch) in self.branches.iter().enumerate() {
            let branch = Arc::clone(branch);
            let session_id = session_id.to_string();
            let user_input = user_input.to_string();
            let parent_name = self.name.clone();
            let semaphore = semaphore.clone();

            futures.push(async move {
                // 限流
                let _permit = match semaphore {
                    Some(sem) => Some(
                        sem.acquire_owned()
                            .await
                            .map_err(|e| Error::Internal(format!("并发许可获取失败: {e}")))?,
                    ),
                    None => None,
                };

                info!(
                    parent = %parent_name,
                    branch_index = index,
                    branch_name = %branch.name(),
                    "ParallelAgent 启动分支"
                );

                branch.run(&session_id, &user_input).await
            });
        }

        let results = join_all(futures).await;

        let mut answers: Vec<String> = Vec::with_capacity(results.len());
        let mut total_steps: u32 = 0;
        let mut all_success = true;

        for (index, result) in results.into_iter().enumerate() {
            match result {
                Ok(outcome) => {
                    if !outcome.success {
                        all_success = false;
                        warn!(
                            parent = %self.name,
                            branch_index = index,
                            "ParallelAgent 分支执行失败"
                        );
                    }
                    total_steps = total_steps.saturating_add(outcome.total_steps);
                    answers.push(outcome.final_answer);
                }
                Err(e) => return Err(e),
            }
        }

        let merged = answers.join("\n---\n");
        if all_success {
            Ok(RunOutcome::success(merged, total_steps))
        } else {
            Ok(RunOutcome::failure(merged, total_steps))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn parallel_runs_branches_concurrently_and_merges_outputs() {
        let parallel = ParallelAgent::new(
            "test_parallel",
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

        let outcome = parallel.run("sess-1", "seed").await.unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.total_steps, 2);
        assert_eq!(outcome.final_answer, "seed-A\n---\nseed-B");
    }

    #[tokio::test]
    async fn parallel_respects_max_concurrency() {
        let parallel = ParallelAgent::new(
            "test_parallel",
            vec![
                Arc::new(StubAgent {
                    name: "a".into(),
                    suffix: "A".into(),
                }) as Arc<dyn Agent>,
                Arc::new(StubAgent {
                    name: "b".into(),
                    suffix: "B".into(),
                }),
                Arc::new(StubAgent {
                    name: "c".into(),
                    suffix: "C".into(),
                }),
            ],
        )
        .with_max_concurrency(1);

        let outcome = parallel.run("sess-1", "seed").await.unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.total_steps, 3);
        assert_eq!(outcome.final_answer, "seed-A\n---\nseed-B\n---\nseed-C");
    }

    #[tokio::test]
    async fn parallel_empty_returns_error() {
        let parallel = ParallelAgent::new("empty", vec![]);
        assert!(parallel.run("sess-1", "x").await.is_err());
    }
}
