# AgentPlanner 抽象移除 — 直接持有 CompletionModel

## 功能说明

移除 `AgentPlanner` trait 抽象层，让 `AgentRuntime` 直接持有 rig `CompletionModel` 实例（通过 object-safe 的 `ModelAdapter` trait 进行类型擦除）。

**核心变化**：用户不再需要实现任何 trait，只需传入 rig 的 `CompletionModel` 实例即可运行智能体。LLM 调用和决策解析逻辑全部内联到 `AgentRuntime` 内部。

## 新增内容

| 文件 | 说明 |
|------|------|
| `planner/model_adapter.rs` | `ModelAdapter` trait + blanket impl（自动为所有 `CompletionModel` 实现） |

### `ModelAdapter` trait

```rust
#[async_trait]
pub trait ModelAdapter: Send + Sync + 'static {
    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<CompletionResponse<()>>;
}

// blanket impl: 任何 CompletionModel 自动实现 ModelAdapter
#[async_trait]
impl<M: CompletionModel + Send + Sync + 'static> ModelAdapter for M { ... }
```

## 删除内容

| 文件/类型 | 说明 |
|-----------|------|
| `planner/rig_planner.rs` | 整个文件删除，逻辑内联到 `agent_runtime.rs` |
| `AgentPlanner` trait | 删除，被 `ModelAdapter` 替代 |
| `PlannerInput` struct | 删除，不再需要（runtime 内部直接构建 `CompletionRequest`） |
| `RigAgentPlanner<M>` | 删除，逻辑内联到 `AgentRuntime::call_model()` |
| `RigPlannerConfig` | 删除，配置直接从 `snapshot.model_config` 读取 |

## 修改内容

### `agent_runtime.rs`

- `planner: Arc<dyn AgentPlanner>` → `model: Arc<dyn ModelAdapter>`
- 新增内部方法：
  - `call_model()` — 构建 `CompletionRequest`，调用 `ModelAdapter::completion`，解析为 `PlannerDecision`
  - `convert_messages()` — `ConversationMessage` → rig `Message` 转换
  - `parse_decision()` — `AssistantContent` → `PlannerDecision` 解析
  - `map_tool_call_to_decision()` — tool call 名称/参数 → `PlannerDecision` 映射

### `planner/mod.rs`

- 移除 `AgentPlanner` trait、`PlannerInput` struct、`rig_planner` 子模块
- 新增 `model_adapter` 子模块，导出 `ModelAdapter`
- 保留 `PlannerDecision`、`ConversationMessage`、`SubAgentDelegation`

### `lib.rs`

- 移除导出：`AgentPlanner`、`PlannerInput`、`RigAgentPlanner`、`RigPlannerConfig`
- 新增导出：`ModelAdapter`

### `resolver.rs`（agent-platform）

- `planner: Arc<dyn AgentPlanner>` → `model: Arc<dyn ModelAdapter>`
- `with_planner()` → `with_model()`

### `scheduler.rs`（agent-platform）

- `planner: Arc<dyn AgentPlanner>` → `model: Arc<dyn ModelAdapter>`

### `agent_run.rs`（agent-service）

- `planner: Arc<dyn AgentPlanner>` → `model: Arc<dyn ModelAdapter>`

### 示例文件

- `basic_agent.rs` — `EchoPlanner` → `EchoModel`（实现 `ModelAdapter`）
- `streaming_agent.rs` — `StreamingPlanner` → `StreamingModel`
- `tool_agent.rs` — `ToolPlanner` → `ToolModel`

### 集成测试

- `integration_test.rs` — `EchoPlanner` → `EchoModel`

## 注意事项

1. **不向后兼容**：所有使用 `AgentPlanner` 的代码需要迁移
2. rig 的 `CompletionModel` 不是 object-safe（有关联类型），因此需要 `ModelAdapter` 作为类型擦除层
3. `ModelAdapter` 的 blanket impl 使得用户**完全不需要手动实现**此 trait
4. `PlannerDecision` 和 `ConversationMessage` 保留为 runtime 内部类型

## 使用示例

### 示例 1：最简接入（使用 OpenAI）

```rust
use std::sync::Arc;
use rig_core::providers::openai;
use rig_core::client::CompletionClient;
use agent_runtime::{AgentRuntime, RuntimeServices, ModelAdapter};
use agent_runtime::runtime::NoopEventPublisher;
use rig_core::tool::ToolSet;

let model = openai::Client::from_env().completion_model("gpt-4o");
let model: Arc<dyn ModelAdapter> = Arc::new(model);

let runtime = AgentRuntime::new(
    snapshot,
    model,
    Arc::new(ToolSet::default()),
    Arc::new(NoopEventPublisher),
    RuntimeServices::noop(),
);

let answer = runtime.run("session-1", "你好", 12345).await?;
```

### 示例 2：使用 agent-platform 解析器

```rust
use agent_platform::AgentRuntimeResolver;

let model = openai::Client::from_env().completion_model("gpt-4o");
let model: Arc<dyn ModelAdapter> = Arc::new(model);

let resolver = AgentRuntimeResolver::with_model(model);
let (runtime, _guards) = resolver.resolve(snapshot, RuntimeServices::noop()).await?;
let answer = runtime.run("session-1", "你好", 12345).await?;
```

### 示例 3：测试用 mock Model

```rust
use agent_runtime::planner::ModelAdapter;
use rig_core::completion::{CompletionRequest, CompletionResponse, message::{AssistantContent, Text}};
use rig_core::one_or_many::OneOrMany;

struct EchoModel;

#[async_trait::async_trait]
impl ModelAdapter for EchoModel {
    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<CompletionResponse<()>> {
        Ok(CompletionResponse {
            choice: OneOrMany::one(AssistantContent::Text(Text {
                text: "Echo response".to_string(),
            })),
            usage: Default::default(),
            raw_response: (),
            message_id: None,
        })
    }
}
```

## 兼容性说明

- **不向后兼容**：`AgentPlanner` trait 已完全移除
- 迁移路径：将 `Arc<dyn AgentPlanner>` 替换为 `Arc<dyn ModelAdapter>`
- 对于真实 LLM provider：直接传入 rig model 实例（blanket impl 自动适配）
- 对于测试 mock：实现 `ModelAdapter` trait（返回 `CompletionResponse<()>`）
