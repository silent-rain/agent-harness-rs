# RigAgentPlanner — rig-core LLM 推理层集成

## 1. 功能说明

本次变更为 `agent-base` 引入了基于 `rig-core` 的真实 LLM 推理层实现，
将 `AgentPlanner` trait 对接到 `rig-core` 的 `CompletionModel`，
实现基于 **function calling 协议**的结构化推理。

核心设计原则：

- **优先 function calling**：LLM 返回 tool call 时，直接映射为 `PlannerDecision`，不解析自由文本
- **纯文本兜底**：LLM 返回纯文本时，包装为 `PlannerDecision::Finalize`
- **可选依赖**：通过 `rig-planner` feature flag 控制，不影响不需要 LLM 的场景（如测试）

---

## 2. 新增内容

### 2.1 文件

| 文件 | 说明 |
|------|------|
| `crates/core/agent-base/src/planner/rig_planner.rs` | `RigAgentPlanner` 实现 |

### 2.2 类型

| 类型 | 说明 |
|------|------|
| `RigAgentPlanner<M>` | 基于 rig-core `CompletionModel` 的 Planner 实现 |
| `RigPlannerConfig` | Planner 配置（温度、max_tokens 等） |

### 2.3 消息转换规则

| `ConversationMessage.role` | rig `Message` 类型 |
|----------------------------|--------------------|
| `System` | `Message::System` |
| `User` | `Message::User { UserContent::Text }` |
| `Assistant` | `Message::Assistant { AssistantContent::Text }` |
| `Tool` | `Message::User { UserContent::ToolResult }` |

### 2.4 Tool Call 映射规则

| tool_name | `PlannerDecision` |
|-----------|-------------------|
| `finalize` | `Finalize { answer }` |
| `update_plan` | `UpdatePlan { plan }` |
| `query_knowledge` | `QueryKnowledge { kb_ids, query, top_k }` |
| `recall_memory` | `RecallMemory { query, top_k }` |
| `delegate_sub_agent` | `DelegateSubAgent { ... }` |
| 其他 | `CallTool { tool_code, call_id, arguments_json }` |

---

## 3. 修改内容

### 3.1 `agent-base/Cargo.toml`

```toml
[dependencies]
rig-core = { workspace = true, optional = true }

[features]
default = []
rig-planner = ["dep:rig-core"]
```

### 3.2 `planner/mod.rs`

- 添加 `#[cfg(feature = "rig-planner")] pub mod rig_planner;`

### 3.3 `lib.rs`

- 添加条件重导出：`#[cfg(feature = "rig-planner")] pub use planner::rig_planner::{RigAgentPlanner, RigPlannerConfig};`

---

## 4. 注意事项

- `RigAgentPlanner` 要求 `M: CompletionModel + Send + Sync + 'static`
- rig-core 的 `CompletionModel` 是泛型，不同 provider（OpenAI、Anthropic、Ollama 等）各有独立类型，无法统一为 `dyn CompletionModel`
- 如需在运行时动态切换 provider，建议在上层（`agent-platform`）封装工厂函数，按 `provider` 字段分支构造不同的 `RigAgentPlanner<M>` 实例
- `AgentDefinitionSnapshot.model_config.model_code` 会通过 `CompletionRequest.model` 字段传递给 rig，覆盖 client 默认模型

---

## 5. 使用示例

### 示例 1：使用 OpenAI 创建 RigAgentPlanner

```rust
use agent_base::{
    RigAgentPlanner, RigPlannerConfig,
    domain::AgentDefinitionSnapshot,
    executor::AgentFunctionRegistry,
    runtime::{AgentRuntime, NoopEventPublisher},
};
use rig_core::providers::openai;
use rig_core::prelude::CompletionClient;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // 初始化 OpenAI client
    let client = openai::Client::new("your-api-key");
    let model = client.completion_model(openai::GPT_4O);

    // 创建 RigAgentPlanner
    let planner = Arc::new(RigAgentPlanner::with_model(model));

    // 构建运行时
    let snapshot = AgentDefinitionSnapshot { /* ... */ };
    let registry = Arc::new(AgentFunctionRegistry::new());
    let publisher = Arc::new(NoopEventPublisher);

    let runtime = AgentRuntime::without_memory(snapshot, planner, registry, publisher);
    let answer = runtime.run("session-1", "帮我分析一下这段代码").await.unwrap();
    println!("{}", answer);
}
```

### 示例 2：使用自定义温度配置

```rust
use agent_base::{RigAgentPlanner, RigPlannerConfig};
use rig_core::providers::openai;
use rig_core::prelude::CompletionClient;

let client = openai::Client::new("your-api-key");
let model = client.completion_model(openai::GPT_4O);

let config = RigPlannerConfig {
    temperature: Some(0.2),   // 低温度，更确定性的输出
    max_tokens: Some(4096),
};

let planner = RigAgentPlanner::new(model, config);
```

### 示例 3：使用 Ollama 本地模型

```rust
use agent_base::RigAgentPlanner;
use rig_core::providers::ollama;
use rig_core::prelude::CompletionClient;

// Ollama 默认连接 localhost:11434
let client = ollama::Client::new();
let model = client.completion_model("qwen2.5-coder:7b");

let planner = RigAgentPlanner::with_model(model);
```

---

## 6. 兼容性说明

- 不启用 `rig-planner` feature 时，`agent-base` 不引入 `rig-core` 依赖，编译产物更小
- 现有测试（`EchoPlanner`、`LoopPlanner`）不受影响，仍可在无 LLM 环境下运行
- `RigAgentPlanner` 实现了 `AgentPlanner` trait，可无缝替换任何 `Arc<dyn AgentPlanner>` 使用场景
