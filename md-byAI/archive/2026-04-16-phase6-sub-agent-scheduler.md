# Phase 6：子智能体调度能力

## 功能说明

Phase 6 实现了多 Agent 协作的核心调度能力，允许主智能体将子任务委派给专门的子智能体执行，支持单次委派和并发扇出两种模式。

## 新增内容

### 1. `agent-runtime/src/scheduler/mod.rs` — 子智能体调度器 trait

| 类型 | 说明 |
|------|------|
| `SubAgentScheduler` | 调度器 trait，定义委派接口 |
| `DelegationRequest` | 单次委派请求（含深度控制） |
| `DelegationResult` | 委派执行结果 |
| `NoopSubAgentScheduler` | Noop 实现（测试/不需要子智能体时使用） |

### 2. `agent-platform/src/scheduler.rs` — 平台级调度器实现

| 类型 | 说明 |
|------|------|
| `PlatformSubAgentScheduler` | 通过 `AgentDefinitionProvider` 加载子智能体快照并执行 |

### 3. `agent-runtime/src/runtime/agent_runtime.rs` — 运行时集成

- `AgentRuntime` 新增 `sub_agent_scheduler` 字段
- 新增 `with_sub_agent_scheduler()` 构建方法
- `DelegateSubAgent` 动作：查找子智能体配置 → 构建 `DelegationRequest` → 调用调度器
- `FanOutSubAgents` 动作：并发构建多个 `DelegationRequest` → 调用 `delegate_fan_out`

## 修改内容

### `agent-runtime/src/lib.rs`

- 新增 `pub mod scheduler;`
- 重导出 `DelegationRequest`、`DelegationResult`、`NoopSubAgentScheduler`、`SubAgentScheduler`

### `agent-platform/src/lib.rs`

- 新增 `pub mod scheduler;`
- 重导出 `PlatformSubAgentScheduler`

## 注意事项

### 递归深度控制

`DelegationRequest` 包含 `depth`（当前深度）和 `max_depth`（最大深度）字段。
`PlatformSubAgentScheduler` 在执行前检查 `depth >= max_depth`，超限时拒绝委派并返回失败结果。

`max_depth` 来自数据库 `t_agent_sub_agent_rel.max_depth` 字段（默认 3）。

### 子智能体别名匹配

`DelegateSubAgent` 动作中的 `sub_agent_code` 通过以下规则匹配 `SubAgentConfig`：

1. 优先匹配 `alias`（别名）
2. 回退匹配 `sub_agent_id.to_string()`

### 子智能体记忆隔离

子智能体使用 `NoopMemoryWriter` 和 `NoopMemoryRetriever`，不共享主智能体的记忆层。
如需记忆共享，可在 `PlatformSubAgentScheduler` 中注入真实记忆实现。

### FanOut 结果合并

`FanOutSubAgents` 并发执行所有委派，等待全部完成后将结果合并为一条消息追加到主智能体的对话历史。
单个子智能体失败不影响其他子智能体的执行。

## 使用示例

### 示例 1：创建带子智能体调度能力的运行时

```rust
use std::sync::Arc;
use agent_platform::{
    AgentRuntimeResolver, PlatformSubAgentScheduler,
    provider::CompositeAgentDefinitionProvider,
};

// 构建调度器
let scheduler = Arc::new(PlatformSubAgentScheduler::new(
    Arc::clone(&provider),
    Arc::clone(&planner),
    user_id,
));

// 装配运行时
let (runtime, _guards) = resolver.resolve_async(snapshot).await?;
let runtime = runtime.with_sub_agent_scheduler(scheduler);

// 运行（当 LLM 决策 DelegateSubAgent 时，调度器自动执行）
let answer = runtime.run("session-1", "请让代码审查智能体分析这段代码").await?;
```

### 示例 2：自定义子智能体调度器

```rust
use async_trait::async_trait;
use agent_runtime::scheduler::{SubAgentScheduler, DelegationRequest, DelegationResult};
use agent_runtime::error::AgentResult;

struct MyScheduler;

#[async_trait]
impl SubAgentScheduler for MyScheduler {
    async fn delegate(&self, request: DelegationRequest) -> AgentResult<DelegationResult> {
        // 自定义委派逻辑
        println!("委派给子智能体: {}", request.sub_agent_code);
        Ok(DelegationResult::success(
            request.delegation_id,
            request.sub_agent_code,
            "自定义结果",
        ))
    }
}
```

### 示例 3：FanOut 并发委派

当 LLM 决策 `FanOutSubAgents` 时，运行时自动并发执行：

```rust
// LLM 返回的决策（由 Planner 解析）
PlannerDecision::FanOutSubAgents {
    delegations: vec![
        SubAgentDelegation {
            sub_agent_code: "search-agent".to_string(),
            delegation_id: "del-001".to_string(),
            task: "搜索相关文档".to_string(),
            context_json: None,
        },
        SubAgentDelegation {
            sub_agent_code: "analysis-agent".to_string(),
            delegation_id: "del-002".to_string(),
            task: "分析数据趋势".to_string(),
            context_json: Some(r#"{"period": "2024Q1"}"#.to_string()),
        },
    ],
}
// 运行时自动并发执行两个子智能体，结果合并后追加到对话历史
```

## 兼容性说明

- `AgentRuntime::new()` 和 `AgentRuntime::without_memory()` 默认使用 `NoopSubAgentScheduler`，向后兼容
- 通过 `.with_sub_agent_scheduler(scheduler)` 链式方法注入真实调度器
- `SubAgentScheduler::delegate_fan_out` 提供默认实现（基于 `futures::join_all`），无需手动实现
