//! 工具调用重试配置
//!
//! 控制单次 `CallableTool` 调用的失败重试策略，对齐 trpc-agent-go 的
//! `WithToolCallRetry` 能力。
//!
//! 提供的主要内容：
//! - `ToolCallRetryConfig`: 重试参数结构
//! - `BackoffStrategy`: 退避策略枚举

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 退避策略
///
/// 决定重试之间的等待时长计算方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum BackoffStrategy {
    /// 固定间隔：每次都等待 `base_delay`
    Fixed,
    /// 指数退避：第 N 次重试等待 `base_delay * 2^N`，并以 `max_delay` 封顶
    #[default]
    Exponential,
}

/// 工具调用重试配置
///
/// 仅作用于**单次** `CallableTool` 执行。重试只在工具内部抛错时触发，
/// 不影响 LLM 决策循环本身的步数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRetryConfig {
    /// 最大重试次数（不含首次调用）
    ///
    /// `0` 表示不重试，仅执行一次。
    pub max_retries: u32,
    /// 基础退避时长（毫秒）
    pub base_delay_ms: u64,
    /// 最大退避时长（毫秒），用于封顶指数退避
    pub max_delay_ms: u64,
    /// 退避策略
    pub strategy: BackoffStrategy,
}

impl Default for ToolCallRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 0,
            base_delay_ms: 200,
            max_delay_ms: 5_000,
            strategy: BackoffStrategy::Exponential,
        }
    }
}

impl ToolCallRetryConfig {
    /// 是否启用重试（`max_retries > 0`）
    pub fn is_enabled(&self) -> bool {
        self.max_retries > 0
    }

    /// 计算第 `attempt` 次重试前的等待时长
    ///
    /// `attempt` 从 `1` 开始计数，对应"第 1 次重试"。
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let base = self.base_delay_ms.max(1);
        let raw = match self.strategy {
            BackoffStrategy::Fixed => base,
            BackoffStrategy::Exponential => {
                // 用 saturating shift 防止溢出
                let shift = (attempt.saturating_sub(1)).min(20) as u64;
                base.saturating_mul(1u64 << shift)
            }
        };
        Duration::from_millis(raw.min(self.max_delay_ms.max(base)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_disabled() {
        let cfg = ToolCallRetryConfig::default();
        assert!(!cfg.is_enabled());
    }

    #[test]
    fn fixed_backoff_returns_constant_delay() {
        let cfg = ToolCallRetryConfig {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 10_000,
            strategy: BackoffStrategy::Fixed,
        };
        assert_eq!(cfg.delay_for(1), Duration::from_millis(100));
        assert_eq!(cfg.delay_for(3), Duration::from_millis(100));
    }

    #[test]
    fn exponential_backoff_grows_and_caps() {
        let cfg = ToolCallRetryConfig {
            max_retries: 10,
            base_delay_ms: 100,
            max_delay_ms: 800,
            strategy: BackoffStrategy::Exponential,
        };
        assert_eq!(cfg.delay_for(1), Duration::from_millis(100));
        assert_eq!(cfg.delay_for(2), Duration::from_millis(200));
        assert_eq!(cfg.delay_for(3), Duration::from_millis(400));
        // 4 次后会被 max_delay 封顶
        assert_eq!(cfg.delay_for(5), Duration::from_millis(800));
    }
}
