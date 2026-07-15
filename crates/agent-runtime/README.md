# agent-runtime

智能体运行时基座库，以 `rig-core` 为核心，提供 Agent Runtime、Planner、Executor、Policy、Streaming 等核心能力。

## 定位

`agent-runtime` 是 Agent Harness 的**运行时内核层**，定义了智能体的核心抽象与执行引擎。它不关心智能体"从哪里来"（数据库 / 配置文件），只关心智能体"如何运行"。

## 核心模块

| 模块        | 说明                                                                   |
| ----------- | ---------------------------------------------------------------------- |
| `runtime`   | 核心运行循环与事件发布器（`Agent`、`AgentBuilder`、`RuntimeServices`） |
| `model`     | LLM 模型适配层（`ModelAdapter`、`StreamChunk`、`MockModel`）           |
| `tool`      | 工具池与内置工具实现（`ToolPool`、`rig::Tool` trait）                  |
| `types`     | 核心领域模型（`AgentDefinitionSnapshot` 等）                           |
| `hook`      | 智能体生命周期钩子（`AgentHook`、`HookRegistry`）                      |
| `session`   | 统一流式事件协议（`AgentStreamEvent`）                                 |
| `message`   | 对话消息中立载体（`ConversationMessage`、`MessageRole`）               |
| `context`   | 对话消息上下文（`MessageContext`）                                     |
| `prompt`    | 系统提示词构建（`SystemPrompt`）                                       |
| `knowledge` | 知识检索抽象（`AgentKnowledgeRetriever`）                              |
| `openai`    | OpenAI Chat Completions 协议产物与流式适配器                           |
| `error`     | 统一错误类型（`AgentError`）                                           |
| `prelude`   | SDK 常用类型的一键导入                                                 |

### 可选能力模块（Feature-gated）

| 模块        | Feature     | 说明                                                                                        |
| ----------- | ----------- | ------------------------------------------------------------------------------------------- |
| `command`   | `command`   | 命令注册与执行                                                                              |
| `rule`      | `rule`      | 规则引擎                                                                                    |
| `skill`     | `skill`     | 本地技能加载器与技能工具（含 zip/sha256 校验）                                              |
| `sub_agent` | `sub_agent` | 子智能体编排                                                                                |
| `mcp`       | `mcp`       | MCP 连接注册表（SSE / HTTP / stdio 传输）                                                   |
| `a2a`       | `a2a`       | A2A（Agent-to-Agent）协议支持：`A2aServer` 将本地 Agent 暴露为服务，`A2AAgent` 调用远程服务 |

## Feature Flags

```toml
[features]
default = []

# 按需启用单个能力
command   = []
rule      = []
skill     = ["dep:zip", "dep:sha2", "dep:hex"]
sub_agent = []
mcp       = ["dep:rmcp", "rig-core/rmcp", ...]
a2a       = ["dep:a2a-rs-core", "dep:a2a-rs-client", "dep:a2a-rs-server", ...]

# 便捷聚合：启用全部能力
full = ["command", "rule", "skill", "sub_agent", "mcp", "a2a"]
```

## 设计原则

1. **以 `rig-core` 为核心** — 工具系统直接使用 `rig::ToolSet`，用户只需传入 `CompletionModel` 实例
2. **动作层结构化，展示层自然语言** — 所有可执行动作通过 `function calling` 或 JSON Schema 约束的结构化协议触发，禁止解析自由文本驱动行为
3. **流式优先** — 运行时原生支持 SSE 流式事件输出
4. **可组合** — 支持 `LlmAgent`、`ChainAgent`、`ParallelAgent` 等多种组合模式

## 快速使用

### 构建并运行一个基础 Agent

```rust
use std::env;
use agent_runtime::{
    runtime::{Agent, AgentBuilder},
    types::runtime::{AgentConfig, AgentDefinition, ModelConfig},
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
            api_key: env::var("OPENAI_API_KEY")?,
            ..Default::default()
        },
    };

    let agent = AgentBuilder::new(config)?.build().await?;
    let outcome = agent.run("session-1", "Hello!").await?;
    println!("{}", outcome.final_answer);
    Ok(())
}
```

### 流式输出

```rust
use agent_runtime::session::AgentStreamEvent;
use futures::StreamExt;

let mut stream = agent.run_stream("session-1", "Tell me a story")?;
while let Some(event) = stream.next().await {
    if let AgentStreamEvent::TextDelta { delta, .. } = event {
        print!("{delta}");
    }
}
```

### A2A Server：将 Agent 暴露为远程服务（`feature = "a2a"`）

```rust
use std::sync::Arc;
use agent_runtime::{a2a::A2aServer, runtime::Agent};

// 一行启动，自动生成 AgentCard
A2aServer::new(Arc::new(agent), bundle)
    .serve("0.0.0.0:3000")
    .await?;
```

### A2AAgent：像本地 Agent 一样调用远程服务（`feature = "a2a"`）

```rust
use agent_runtime::{a2a::A2AAgent, runtime::Agent};

// 自动发现 AgentCard，一行连接
let remote = A2AAgent::connect("http://remote-agent:3000").await?;
let outcome = remote.run("session-1", "What is 2 + 2?").await?;
println!("{}", outcome.final_answer);
```

## 示例

项目提供了多个示例，可通过以下命令运行：

```bash
# 基础 Agent（非流式）
cargo run --example basic_agent --features rule

# 基础 Agent（流式）
cargo run --example basic_agent_streaming --features rule

# 基础 Agent + OpenAI 流式
cargo run --example basic_agent_openai_streaming

# 全能力 Agent（非流式）
cargo run --example full_agent --features full

# 全能力 Agent（流式）
cargo run --example full_agent_streaming --features full

# 工具调用流式
cargo run --example tool_calling_streaming

# 本地知识源 Agent
cargo run --example loca_source_agent --features full

# A2A Server：将本地 Agent 暴露为 A2A 服务
cargo run --example a2a_server --features a2a

# A2A Client：调用远程 A2A Agent（需先启动 a2a_server）
cargo run --example a2a_client --features a2a
```
