# A2A（Agent-to-Agent）协议支持

## 功能说明

`agent-runtime` 通过 `feature = "a2a"` 提供完整的 A2A v1.0 双向通信支持，让 Agent 之间的通信像调用本地函数一样简单。

**两个核心组件：**

| 组件        | 类型   | 作用                                                        |
| ----------- | ------ | ----------------------------------------------------------- |
| `A2aServer` | 服务端 | 将本地 `Arc<dyn Agent>` 一键暴露为 A2A v1.0 HTTP 服务       |
| `A2AAgent`  | 客户端 | 将远程 A2A 服务透明代理为本地 `Agent`（实现 `Agent` trait） |

---

## 新增内容

### 模块结构

```text
agent-runtime/src/a2a/
├── mod.rs                  # 模块声明与公共导出
├── card.rs                 # AgentBundle → AgentCard 转换（AgentCardBuilder）
├── server/
│   ├── mod.rs              # server 子模块声明
│   ├── executor.rs         # AgentExecAdapter（impl MessageHandler）
│   └── serve.rs            # A2aServer / MultiA2aServer 启动器
└── client/
    ├── mod.rs              # client 子模块声明
    └── a2a_agent.rs        # A2AAgent（impl Agent）+ A2AAgentBuilder
```

### 新增文件

#### `card.rs` — AgentCard 生成

```rust
pub struct AgentCardBuilder { ... }

impl AgentCardBuilder {
    pub fn from_bundle(bundle: &AgentBundle, base_url: &str) -> Self;
    pub fn with_provider(self, name: &str, url: &str) -> Self;
    pub fn build(self) -> AgentCard;
}
```

- 从 `AgentBundle` 自动提取 `name`、`description`、`skills`（工具列表）
- 生成符合 A2A v1.0 规范的 `AgentCard`
- 支持链式调用设置 `provider`、`icon` 等可选字段

#### `server/executor.rs` — 协议适配器

```rust
pub type EventSenderSlot = Arc<OnceLock<broadcast::Sender<...>>>;

pub struct AgentExecAdapter {
    agent: Arc<dyn Agent>,
    card: AgentCard,
    event_sender_slot: EventSenderSlot,
}

impl MessageHandler for AgentExecAdapter {
    async fn handle(&self, ctx: ExecutionContext, queue: EventQueue) -> Result<()>;
    fn agent_card(&self) -> &AgentCard;
}
```

**事件映射表（`AgentStreamEvent` → A2A 事件）：**

| `AgentStreamEvent`               | A2A 事件                                                     |
| -------------------------------- | ------------------------------------------------------------ |
| `RunStarted`                     | `TaskStatus { state: Working }`                              |
| `TextDelta { delta }`            | `TaskArtifactUpdate { artifact: "answer", append: true }`    |
| `Answer { content }`             | `TaskArtifactUpdate { artifact: "answer", lastChunk: true }` |
| `ToolCallStarted { name }`       | `TaskStatus { state: Working, message: "calling {name}" }`   |
| `ToolCallFinished`               | `TaskStatus { state: Working }`                              |
| `RunFinished { FinalizeAction }` | `TaskStatus { state: Completed }`                            |
| `RunFinished { Error }`          | `TaskStatus { state: Failed }`                               |
| `RunFinished { Cancelled }`      | `TaskStatus { state: Canceled }`                             |

#### `server/serve.rs` — 服务启动器

```rust
pub struct A2aServer { ... }

impl A2aServer {
    pub fn new(agent: Arc<dyn Agent>, bundle: AgentBundle) -> Self;
    pub fn with_card(agent: Arc<dyn Agent>, card: AgentCard) -> Self;
    pub fn bind(self, addr: impl Into<String>) -> Self;
    pub async fn serve(self, addr: impl Into<String>) -> AgentResult<()>;
}

pub struct MultiA2aServer { ... }

impl MultiA2aServer {
    pub fn new(bind_addr: impl Into<String>) -> Self;
    pub fn mount(self, base_path: impl Into<String>, agent: Arc<dyn Agent>, bundle: AgentBundle) -> Self;
    pub async fn serve(self) -> AgentResult<()>;
}
```

#### `client/a2a_agent.rs` — 远程 Agent 代理

```rust
pub struct A2AAgent {
    card: AgentCard,
    client: Arc<A2aClient>,
}

impl A2AAgent {
    pub async fn connect(base_url: impl Into<String>) -> AgentResult<Self>;
    pub fn agent_card(&self) -> &AgentCard;
}

impl Agent for A2AAgent {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    async fn run(&self, session_id: &str, input: &str) -> AgentResult<RunOutcome>;
    fn run_stream(&self, session_id: &str, input: &str) -> AgentResult<AgentEventStream>;
}

pub struct A2AAgentBuilder { ... }

impl A2AAgentBuilder {
    pub fn new(base_url: impl Into<String>) -> Self;
    pub fn with_timeout(self, timeout: Duration) -> Self;
    pub fn with_max_polls(self, max_polls: u32) -> Self;
    pub fn with_poll_interval_ms(self, interval_ms: u64) -> Self;
    pub async fn build(self) -> AgentResult<A2AAgent>;
}
```

**事件映射表（A2A 事件 → `AgentStreamEvent`）：**

| A2A 事件                                             | `AgentStreamEvent`                    |
| ---------------------------------------------------- | ------------------------------------- |
| `Task { state: Working }`                            | `StepStarted`                         |
| `StatusUpdate { state: Working }`                    | （忽略，仅 debug log）                |
| `StatusUpdate { state: Completed }`                  | 触发 `RunFinished { FinalizeAction }` |
| `StatusUpdate { state: Failed }`                     | 触发 `RunFinished { Error }`          |
| `StatusUpdate { state: Canceled }`                   | 触发 `RunFinished { Cancelled }`      |
| `ArtifactUpdate { name: "answer", append }`          | `TextDelta { delta }`                 |
| `ArtifactUpdate { name: "answer", lastChunk: true }` | `Answer { content }`                  |
| `Message { parts }`                                  | `Answer { content }`                  |

---

## 修改内容

### `Cargo.toml`

新增 `a2a` feature 及依赖：

```toml
[features]
a2a = ["dep:a2a-rs-core", "dep:a2a-rs-client", "dep:a2a-rs-server", "dep:async-stream", "dep:futures"]

[dependencies]
a2a-rs-core   = { version = "1.0", optional = true }
a2a-rs-client = { version = "1.0", optional = true }
a2a-rs-server = { version = "1.0", optional = true }
```

### `lib.rs`

新增 feature-gated 模块声明：

```rust
#[cfg(feature = "a2a")]
pub mod a2a;
```

---

## 注意事项

### session_id 语义

`A2AAgent` 使用 `session_id` 作为 A2A 的 `context_id`（而非 `task_id`）：

- `context_id`：跨多次 Task 的持久会话标识（对应 `agent-runtime` 的 `session_id`）
- `task_id`：单次任务标识（每次 `run()` 调用生成新的 UUID）

这确保了多轮对话的历史能被正确关联。

### event_sender_slot 设计

`A2aServer` 在构造时预先保存 `EventSenderSlot`（`Arc<OnceLock<...>>`），
在 `adapter` 被 move 进 `InnerA2aServer` 之后，仍能通过 Arc 共享引用将
`broadcast::Sender` 注入 adapter：

```text
A2aServer::new()
  ├── 创建 AgentExecAdapter
  ├── 克隆 event_sender_slot（Arc）
  └── 保存到 self.event_slot

A2aServer::serve()
  ├── InnerA2aServer::new(self.adapter)  ← adapter 被 move
  ├── inner.get_event_sender() → tx
  └── self.event_slot.set(tx)  ← 通过 Arc 仍然有效
```

### MultiA2aServer P0 限制

当前版本（P0 MVP）`MultiA2aServer` 仅使用第一个挂载的 Agent。
完整多路由支持（通过 axum Router 组合）将在 P3 阶段实现。

---

## 使用示例

### 示例 1：一行启动 A2A Server

```rust
use std::sync::Arc;
use agent_runtime::{
    a2a::A2aServer,
    runtime::{Agent, AgentBuilder},
    types::runtime::{AgentBundle, AgentConfig, AgentDefinition, ModelConfig},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AgentConfig {
        agent_definition: AgentDefinition {
            id: 1,
            name: "my-agent".to_string(),
            description: "A helpful assistant".to_string(),
            system_prompt: "You are a helpful assistant.".to_string(),
            ..Default::default()
        },
        model_config: ModelConfig {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: std::env::var("OPENAI_API_KEY")?,
            ..Default::default()
        },
    };

    let bundle = AgentBundle {
        agent_definition: config.agent_definition.clone(),
        model_config: config.model_config.clone(),
        ..Default::default()
    };

    let agent: Arc<dyn Agent> = Arc::new(
        AgentBuilder::new(config)?.build().await?
    );

    // 一行启动，自动生成 AgentCard
    A2aServer::new(agent, bundle).serve("0.0.0.0:3000").await?;
    Ok(())
}
```

### 示例 2：连接远程 A2A Agent（非流式）

```rust
use agent_runtime::{a2a::A2AAgent, runtime::Agent};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 自动发现 AgentCard，一行连接
    let remote = A2AAgent::connect("http://remote-agent:3000").await?;

    println!("Connected to: {}", remote.name());

    // 像本地 Agent 一样调用
    let outcome = remote.run("session-1", "What is 2 + 2?").await?;
    println!("Answer: {}", outcome.final_answer);

    Ok(())
}
```

### 示例 3：流式调用远程 A2A Agent

```rust
use agent_runtime::{a2a::A2AAgent, runtime::Agent, session::AgentStreamEvent};
use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let remote = A2AAgent::connect("http://remote-agent:3000").await?;

    let mut stream = remote.run_stream("session-2", "Tell me a story")?;

    while let Some(event) = stream.next().await {
        match event {
            AgentStreamEvent::TextDelta { delta, .. } => print!("{delta}"),
            AgentStreamEvent::Answer { content, .. } => {
                println!("\n\nFinal: {content}");
            }
            AgentStreamEvent::RunFinished { finish_reason, .. } => {
                println!("Done: {finish_reason:?}");
            }
            _ => {}
        }
    }

    Ok(())
}
```

### 示例 4：自定义 AgentCard

```rust
use std::sync::Arc;
use agent_runtime::{
    a2a::{A2aServer, card::AgentCardBuilder},
    types::runtime::AgentBundle,
};

# async fn example(agent: Arc<dyn agent_runtime::Agent>, bundle: AgentBundle) -> anyhow::Result<()> {
let card = AgentCardBuilder::from_bundle(&bundle, "http://my-agent.example.com")
    .with_provider("MyTeam", "https://myteam.example.com")
    .build();

A2aServer::with_card(agent, card).serve("0.0.0.0:3000").await?;
# Ok(())
# }
```

### 示例 5：多 Agent 单端口（P0 MVP）

```rust
use std::sync::Arc;
use agent_runtime::a2a::serve::MultiA2aServer;

# async fn example(
#     math_agent: Arc<dyn agent_runtime::Agent>, math_bundle: agent_runtime::types::runtime::AgentBundle,
#     weather_agent: Arc<dyn agent_runtime::Agent>, weather_bundle: agent_runtime::types::runtime::AgentBundle,
# ) -> anyhow::Result<()> {
MultiA2aServer::new("0.0.0.0:3000")
    .mount("/agents/math", math_agent, math_bundle)
    .mount("/agents/weather", weather_agent, weather_bundle)
    .serve()
    .await?;
# Ok(())
# }
```

---

## 兼容性说明

- **Rust 版本**：1.75+（async trait 稳定版）
- **A2A 协议版本**：v1.0（RC 1.0）
- **依赖版本**：`a2a-rs-core/client/server = "1.0.26"`
- **现有 Agent 兼容性**：完全兼容，无需修改任何现有 `Agent` 实现
- **feature 隔离**：`a2a` feature 完全可选，不启用时零编译开销

### 涉及的组件

| 组件               | 变更类型     | 说明                              |
| ------------------ | ------------ | --------------------------------- |
| `agent-runtime`       | 新增 feature | `a2a` feature，默认不启用         |
| `LlmAgent`         | 无变更       | 直接被 `A2aServer` 持有，无需修改 |
| `ChainAgent`       | 无变更       | 同上                              |
| `ParallelAgent`    | 无变更       | 同上                              |
| `AgentBundle`      | 无变更       | 作为 AgentCard 生成的数据源       |
| `AgentStreamEvent` | 无变更       | 作为协议转换的中间格式            |
