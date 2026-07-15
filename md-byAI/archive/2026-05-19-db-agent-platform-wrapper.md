# DbAgent: 数据库驱动的智能体一键装配

> 模块路径：`crates/core/agent-platform/src/agent/`
>
> 目标读者：使用 `agent-platform` 装配智能体的 Rust 开发者

## 1. 功能说明

`DbAgent` 是 `agent-platform` 在 `agent-base` 之上提供的**装配层快速通道**：
开发者只需提供 `(database_url, agent_id, user_id)`，即可一次性获得：

- 完整加载好的 [`AgentConfig`]：智能体定义 + 模型绑定（`provider` / `base_url` / `api_key` / `model`）
- 已加载的能力配置：`rules` / `commands` / `skills` / `tools` / `mcp_services` / `sub_agents`
- 已对接 [`DbSessionService`] 的事件入库通道（`t_agent_session_event`）
- 已对接 [`DbAgentMemoryRetriever`] 的会话事件流记忆召回
- 一个实现 [`agent_base::Agent`] trait 的运行时实例，可直接 `run` / `run_stream`

整个加载流程对调用方完全透明：

```text
DbAgentBuilder::new(db, agent_id, user_id).build().await?
```

不再需要手写 `Pool::connect → DbAgentDefinitionProvider::load → DbRuleLoader::load → build_model → AgentBuilder::new → with_rule_configs → build` 等 50+ 行模板代码。

## 2. 新增内容

### 2.1 模块结构

```text
agent-platform/src/agent/
├── mod.rs          # 模块声明与重导出
├── builder.rs      # DbAgentBuilder
├── db_agent.rs     # DbAgent（实现 Agent trait）
└── publisher.rs    # SessionPersistingPublisher
```

### 2.2 核心类型

| 类型                           | 角色                              |
| ------------------------------ | --------------------------------- |
| [`DbAgentBuilder`]             | 入口构建器，负责加载与装配        |
| [`DbAgent`]                    | 实现 [`Agent`] trait 的运行时包装 |
| [`SessionPersistingPublisher`] | 事件入库 + 可选转发 publisher     |
| [`CapabilitySummary`]          | 能力数量摘要                      |

### 2.3 关键 API

- `DbAgentBuilder::new(db, agent_id, user_id) -> Self`
- `DbAgentBuilder::with_max_turns(u32)`
- `DbAgentBuilder::with_forward_publisher(Arc<dyn AgentEventPublisher>)`
- `DbAgentBuilder::with_local_scan(bool)` —— 叠加本地 `.codebuddy/*` 扫描
- `DbAgentBuilder::with_auto_create_session(bool)` —— 自动确保会话记录存在
- `DbAgentBuilder::build() -> AgentResult<DbAgent>` —— 单实例多会话
- `DbAgentBuilder::build_for_session(&str) -> AgentResult<LlmAgent>` —— 一会话一实例

## 3. 修改内容

| 文件                                  | 变更                                       |
| ------------------------------------- | ------------------------------------------ |
| `agent-platform/src/lib.rs`           | 新增 `pub mod agent;` 与对外重导出         |
| `agent-platform/examples/db_agent.rs` | 用 `DbAgentBuilder` 替换原本手写的装配模板 |

不影响任何已有 API：`DbAgentDefinitionProvider`、`DbRuleLoader` 等仍以原方式可用。

## 4. 注意事项

### 4.1 事件落库去重

`agent-base` 内部已经在三个事件上直接调用 `services.session.append_event(...)`：

- `UserMessage`（`LlmAgent::run_stream_inner`）
- `AnswerDelta`（`ModelCaller`）
- `ToolCallFinished`（`ToolExecutor`）

`SessionPersistingPublisher` 默认通过 `skip_internal: true` 跳过这三种事件，避免在 `t_agent_session_event` 中产生重复行。如需关闭去重，调用 `SessionPersistingPublisher::skip_internal(false)`。

### 4.2 `run_stream` 与异步装配

`Agent::run_stream` 在 trait 定义上是同步签名（返回 `AgentResult<AgentEventStream>`），但 `DbAgent` 内部需要异步调用 `services.session.create_session(...)` 以确保会话记录存在。当前实现使用 `futures::executor::block_on` 完成这次轻量异步调用，在已有 tokio runtime 的环境中可能引发警告。

**建议**：调用 `run_stream` 前先通过 `DbAgentBuilder::with_auto_create_session(false)`，并在外层显式管理会话生命周期，即可彻底避免阻塞。

### 4.3 子智能体加载

子智能体由 [`DbAgentDefinitionProvider`] 内部装配并已写入 `AgentConfig` 的依赖图，因此 `DbAgentBuilder` 不再二次加载。`CapabilitySummary.sub_agents` 仅记录通过 `DbAgentBuilder::with_*_configs` 显式追加的子智能体数量。

### 4.4 模型构建

当前 `DbAgentBuilder` 默认按 OpenAI Chat Completions 协议构建 `ModelAdapter`，从 `t_agent_llm_model.api_key` 与 `base_url` 直接读取。其他协议（Anthropic / Gemini）需要在外层先构建好 `Arc<dyn ModelAdapter>`，再使用 `agent-base::AgentBuilder` 自行装配（`DbAgentBuilder` 暂未提供模型注入入口）。

## 5. 使用示例

### 示例 1：最简调用（CLI / 一次性运行）

```rust,no_run
use agent_platform::DbAgentBuilder;
use database::{Options, Pool};

# async fn main_inner() -> agent_base::error::AgentResult<()> {
let db = Pool::connect("sqlite::memory:".to_string(), Options::default())
    .await
    .expect("connect db");

let agent = DbAgentBuilder::new(db, 1001, 42).build().await?;

let outcome = agent.run("session-cli", 42, "你好").await?;
println!("{}", outcome.final_answer);
# Ok(())
# }
```

适用场景：脚本、单次命令行运行。

### 示例 2：长连接 + SSE 事件流转发

```rust,no_run
use std::sync::Arc;

use agent_base::{ChannelEventPublisher, runtime::Agent};
use agent_platform::DbAgentBuilder;
use tokio::sync::mpsc;

# async fn main_inner(db: sea_orm::DatabaseConnection) -> agent_base::error::AgentResult<()> {
let (tx, mut rx) = mpsc::channel(64);
let forward = Arc::new(ChannelEventPublisher::new(tx));

let agent = DbAgentBuilder::new(db, 1001, 42)
    .with_forward_publisher(forward) // 落库 + 推送到 SSE 通道
    .with_max_turns(8)
    .build()
    .await?;

let agent = Arc::new(agent);
let bg = tokio::spawn({
    let agent = Arc::clone(&agent);
    async move {
        let _ = agent.run("session-sse", 42, "帮我查询订单").await;
    }
});

while let Some(event) = rx.recv().await {
    // 转发给前端 SSE / WebSocket
    println!("event: {:?}", event);
}
let _ = bg.await;
# Ok(())
# }
```

适用场景：HTTP 服务承载长连接，事件需要既落库又流式推送。

### 示例 3：复用 `agent-base` 类型（自定义钩子 / 工具）

```rust,no_run
use std::sync::Arc;

use agent_base::{
    AgentBuilder, ChannelEventPublisher, hook::AgentHook,
    runtime::Agent,
};
use agent_platform::DbAgentBuilder;

# async fn main_inner(db: sea_orm::DatabaseConnection) -> agent_base::error::AgentResult<()> {
// 直接拿到内部 LlmAgent，进一步链式追加自定义工具或钩子
let llm_agent = DbAgentBuilder::new(db, 1001, 42)
    .build_for_session("session-rich")
    .await?;

// LlmAgent 已经是 Agent trait，直接 run / run_stream 即可
let outcome = llm_agent.run("session-rich", 42, "请帮我列出风险").await?;
println!("steps={}, ans={}", outcome.total_steps, outcome.final_answer);
# Ok(())
# }
```

适用场景：需要在 `DbAgent` 之外再注入业务自定义工具或钩子。

## 6. 兼容性说明

- **API**：完全增量，未改动 `DbAgentDefinitionProvider`、`DbRuleLoader`、`DbSessionService` 等已有公共类型
- **数据库表**：未引入新表；`SessionPersistingPublisher` 仅复用 `t_agent_session_event` 已有 schema
- **Crate 依赖**：未新增 workspace 级 crate；`futures` 已是既有依赖
- **Feature flag**：保持 `agent-base = { workspace = true, features = ["mcp"] }` 不变，未引入新 feature
- **Rust 版本**：使用 `if let ... && let ...` chain，需 edition 2024（项目当前已是 `edition = "2024"`）

## 7. 涉及组件

| 组件                                                                                 | 角色                                            |
| ------------------------------------------------------------------------------------ | ----------------------------------------------- |
| `agent-base::AgentBuilder`                                                           | 真正完成工具集 / 钩子 / 子 Agent 装配           |
| `agent-base::LlmAgent`                                                               | ReAct 推理循环                                  |
| `agent-platform::DbAgentDefinitionProvider`                                          | 加载 `t_agent_config` + 模型绑定 + 子智能体     |
| `agent-platform::DbRuleLoader` / `DbCommandLoader` / `DbSkillLoader` / `DbMcpLoader` | 加载各资源域配置                                |
| `agent-platform::DbSessionService`                                                   | 会话主表 + 事件表 + 状态表的统一服务            |
| `agent-platform::DbAgentMemoryRetriever`                                             | 从 `t_agent_session_event` 召回 `memory_*` 事件 |

[`AgentConfig`]: ../crates/core/agent-base/src/types/runtime/mod.rs
[`Agent`]: ../crates/core/agent-base/src/runtime/agent/traits.rs
[`DbAgent`]: ../crates/core/agent-platform/src/agent/db_agent.rs
[`DbAgentBuilder`]: ../crates/core/agent-platform/src/agent/builder.rs
[`SessionPersistingPublisher`]: ../crates/core/agent-platform/src/agent/publisher.rs
[`DbSessionService`]: ../crates/core/agent-platform/src/session/db_session_service.rs
[`DbAgentMemoryRetriever`]: ../crates/core/agent-platform/src/memory/db_memory.rs
[`CapabilitySummary`]: ../crates/core/agent-platform/src/agent/db_agent.rs
