> 重构日期：2026-05-14
> 影响范围：`agent-runtime` runtime 模块（破坏性变更）

## 1. 功能说明

将 `agent-runtime` 的运行时抽象由"`Agent` 结构体 + `Runner` trait + `ExecutionStrategy` trait"三层揉合架构，重构为单层"`Agent` trait + 多种实现"的扁平架构。

参考 `demo/trpc-agent-go` 的设计，让 `LlmAgent`（原 `DefaultRunner`）、`ChainAgent`（原 `ChainStrategy`）、`ParallelAgent`（原 `ParallelStrategy`）成为 `Agent` trait 的兄弟实现。同时把原 `default_runner.rs`（1729 行）按职责拆分为五个独立组件，并引入 `LoopShape` / `ToolExecutionPolicy` 两个可插拔扩展点，为未来的 Cycle / Ralph / Graph 等执行模式铺路。

## 2. 新增内容

### 2.1 `runtime::agent` — 顶层抽象与默认实现

| Item | 说明 |
|------|------|
| `trait Agent` | 顶层智能体抽象（所有执行模式的统一接口） |
| `trait ManagedAgent` | 扩展能力：取消 / 状态查询 |
| `trait SteerableAgent` | 扩展能力：运行中注入消息 |
| `LlmAgent` | LLM 推理智能体（默认 ReAct 循环） |
| `ChainAgent` | 链式串联多个 Agent，前者输出 → 后者输入 |
| `ParallelAgent` | 并发执行多个 Agent，结果按定义顺序拼接 |

### 2.2 `runtime::llm` — LlmAgent 内部组件（原 `default_runner.rs` 拆分）

| 组件 | 职责 |
|------|------|
| `ModelCaller` | LLM 流式调用 + 增量聚合 + 决策解析 |
| `ToolExecutor` | 单工具调用 + 重试 + 事件 + 钩子 + 审计 + 记忆（实现 `ToolExecutionRunner`） |
| `DecisionDispatcher` | 非工具决策分支（RAG / Memory / Plan / Finalize） |
| `RunJournal` | 运行记录创建与完成 |
| `HookPipeline` | Agent / Model 级钩子触发与事件副作用 |

### 2.3 `runtime::tool_policy` — 工具执行策略（可插拔）

| Item | 说明 |
|------|------|
| `trait ToolExecutionPolicy` | 决定一组并行工具如何被实际执行 |
| `trait ToolExecutionRunner` | 单工具执行器接口（`ToolExecutor` 实现） |
| `SequentialPolicy` | 严格按调用顺序逐个执行 |
| `BoundedParallelPolicy` | 信号量限流的并发执行（默认） |

### 2.4 `runtime::loop_shape` — 循环形态（可插拔）

| Item | 说明 |
|------|------|
| `trait LoopShape` | 决定主循环"继续 / 停止"判定 |
| `LoopAction` / `LoopState` | 循环决策的输入与输出 |
| `ReactLoop` | 默认实现：遇 `Finalize` 即停 |

未来可新增 `SingleShotLoop` / `RalphLoop` / `CycleLoop` 等。

### 2.5 `runtime::outcome::RunOutcome`

`Agent::run` 由"返回 `String`"升级为"返回 `RunOutcome { final_answer, total_steps, success }`"，避免组合场景下信息丢失。

## 3. 修改内容

### 3.1 删除项

- `runtime::strategies` 整个模块（`ExecutionStrategy` / `LlmLoopStrategy` / `ChainStrategy` / `ParallelStrategy` / `StrategyContext` / `StrategyOutcome`）
- `runtime::runner::DefaultRunner` / `runtime::runner::Runner` / `ManagedRunner` / `SteerableRunner` trait
- 旧的 `Agent` 结构体（`runtime/agent.rs`，623 行）

### 3.2 替换映射

| 旧 API | 新 API |
|--------|--------|
| `Agent`（struct） | `LlmAgent`（struct）+ `Agent`（trait） |
| `Agent::builder(...)` | `AgentBuilder::new(...)` |
| `Agent::run` 返回 `String` | `Agent::run` 返回 `RunOutcome` |
| `agent.run_stream(_, _, _, buffer)` 返回 `mpsc::Receiver` | `agent.run_stream(_, _, _)` 返回 `AgentEventStream` |
| `DefaultRunner::with_max_parallel_tools(n)` | `LlmAgent::with_max_parallel_tools(n)` 或 `with_tool_policy(...)` |
| `LlmLoopStrategy` / `ChainStrategy` / `ParallelStrategy` | `LlmAgent` / `ChainAgent` / `ParallelAgent` |

### 3.3 字段去重

旧 `Agent` 与 `DefaultRunner` 持有 9 字段重复（model / agent_config / services / toolset / publisher / max_turns / rule_configs / skill_configs / command_configs），新架构下仅 `LlmAgent` 持有，消除重复。

## 4. 注意事项

1. **破坏性变更**：本次不向后兼容，所有外部调用方需迁移（已同步更新 `agent-service` 与 examples）。
2. **`run_stream` 返回类型变更**：从 `mpsc::Receiver<AgentStreamEvent>` 改为 `Pin<Box<dyn Stream<...>>>`。调用方需引入 `futures::StreamExt` 或 `tokio_stream::StreamExt` 以使用 `.next().await`。
3. **`run` 返回类型变更**：从 `String` 改为 `RunOutcome`，需通过 `.final_answer` 获取文本。
4. **`ManagedAgent::cancel` / `SteerableAgent::enqueue_user_message` 当前是 TODO**：原 `DefaultRunner` 的对应实现也是占位，本次重构保持一致，待后续补全。

## 5. 使用示例

### 5.1 单个 LLM Agent（默认 ReAct）

```rust,ignore
use agent_runtime::{Agent, AgentBuilder, NoopEventPublisher, RuntimeServices};
use std::sync::Arc;

let agent = AgentBuilder::new(snapshot, model, Arc::new(NoopEventPublisher), RuntimeServices::noop())
    .with_max_turns(10)
    .build()
    .await?;

let outcome = agent.run("session-1", "你好", 12345).await?;
println!("{} (steps={})", outcome.final_answer, outcome.total_steps);
```

### 5.2 自定义工具执行策略（顺序执行）

```rust,ignore
use agent_runtime::runtime::SequentialPolicy;
use std::sync::Arc;

let agent: agent_runtime::LlmAgent = AgentBuilder::new(/* ... */).build().await?;
let agent = agent.with_tool_policy(Arc::new(SequentialPolicy));
```

### 5.3 链式组合多个 Agent

```rust,ignore
use agent_runtime::{Agent, ChainAgent};
use std::sync::Arc;

let analyzer: Arc<dyn Agent> = Arc::new(build_llm_agent("analyzer").await?);
let planner:  Arc<dyn Agent> = Arc::new(build_llm_agent("planner").await?);
let executor: Arc<dyn Agent> = Arc::new(build_llm_agent("executor").await?);

let pipeline = ChainAgent::new("review_pipeline", vec![analyzer, planner, executor]);
let outcome = pipeline.run("session-1", "评审这段代码", 12345).await?;
```

### 5.4 并行多视角分析

```rust,ignore
use agent_runtime::{Agent, ParallelAgent};
use std::sync::Arc;

let reviewer_a: Arc<dyn Agent> = Arc::new(build_llm_agent("reviewer_a").await?);
let reviewer_b: Arc<dyn Agent> = Arc::new(build_llm_agent("reviewer_b").await?);

let multi = ParallelAgent::new("multi_review", vec![reviewer_a, reviewer_b])
    .with_max_concurrency(2);

let outcome = multi.run("session-1", "评审需求", 12345).await?;
// outcome.final_answer 为两个分支结果按 `\n---\n` 拼接
```

## 6. 兼容性说明

- **MSRV / 依赖**：未引入新的外部依赖。
- **二进制兼容**：破坏性变更，需重新编译所有上游 crate。
- **行为兼容**：
  - `LlmAgent` 的 ReAct 循环行为与原 `DefaultRunner::run_inner` **完全等价**；
  - 工具并发上限默认仍为 8（原 `DefaultRunner` 默认值）；
  - 所有 7 个 `PlannerDecision` 分支均按原语义实现（CallTool / CallToolsParallel / QueryKnowledge / RecallMemory / UpdatePlan / Finalize）；
  - 所有事件（`RunStarted` / `StepStarted` / `ToolCallStarted` / `ToolCallFinished` / `RagStarted` / `RagFinished` / `AnswerDelta` / `ReasoningDelta` / `RunFinished` / `Error` / `Warning`）发布顺序与时机保持不变；
  - 所有 6 个钩子点（`before_agent` / `after_agent` / `before_model` / `after_model` / `before_tool` / `after_tool`）触发时机保持不变。
- **测试**：`cargo test -p agent-runtime --lib` **229 个用例全部通过**。

## 7. 未来扩展指南

要新增执行模式（如 Ralph 持续循环），仅需：

1. 新建 `loop_shape::ralph_loop::RalphLoop`，实现 `LoopShape` trait；
2. 在构建 `LlmAgent` 时调用 `.with_loop_shape(Arc::new(RalphLoop))` 即可。

要新增执行编排（如 Graph 拓扑），仅需新建 `agent::graph_agent::GraphAgent` 实现 `Agent` trait，无需触碰任何现有模块。
