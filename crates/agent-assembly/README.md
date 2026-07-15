# agent-assembly

**智能体装配层** — 负责从数据库/配置源装配 `AgentBundle`，并构建可运行的 Agent 实例。

## 概述

`agent-assembly` 是 Agent Harness 架构中的**装配层（Assembly Layer）**，位于运行时基座 `agent-base` 与服务层 `agent-service` 之间。它的核心职责是：

1. 从数据库加载 Agent 配置（模型绑定、工具、规则、技能、MCP 服务等）
2. 将配置组装为 `AgentBundle`（运行时快照）
3. 提供会话持久化服务（Session DAO）

## 架构定位

```text
agent-service          ← HTTP/SSE 对外接口
  └── agent-assembly   ← 本 crate（装配层）
        ├── loader/    ← 各类资源加载器（Tool / Rule / Skill / MCP / Command / SubAgent）
        ├── session/   ← 会话持久化（Session / Event / State DAO）
        ├── agent/     ← 数据库驱动的 AgentSource
        ├── knowledge/ ← 知识库适配器
        └── agent-base ← 运行时基座（不含 DB 依赖）
```

## 核心模块

| 模块        | 说明                                                                    |
| ----------- | ----------------------------------------------------------------------- |
| `agent`     | `DbAgentSource` — 从数据库加载 Agent 配置并构建 `AgentBundle`           |
| `loader`    | 资源加载器集合：Tool、Rule、Skill、MCP、Command、SubAgent、ModelBinding |
| `session`   | 会话持久化：`SessionDao`、`SessionEventDao`、`SessionStateDao`          |
| `knowledge` | `AgentKnowledgeAdapter` — 将 RAG 检索能力适配到 Agent 运行时            |
| `enums`     | 装配层业务枚举定义                                                      |
| `error`     | 统一错误类型 `Error` 与 `PlatformResult`                                |

## 快速开始

### 环境变量

```bash
cd agent-harness/crates/agent-assembly
cp .env.example .env
# 编辑 .env 填入数据库连接和 LLM API Key
```

### 运行示例

```bash
# 从数据库加载 Agent 并运行

# 基础
cargo run -p agent-assembly --example db_agent -- --prompt "帮我写一首诗"

# 指定 session-id
cargo run -p agent-assembly --example db_agent -- --session-id "my-session-1" --prompt "你好"

# 基础流式
cargo run -p agent-assembly --example db_agent_streaming -- --session-id "my-session-2" --prompt "你好"

# 基础流式 Agent + OpenAI
cargo run -p agent-assembly --example db_agent_openai_streaming -- --session-id "my-session-2" --prompt "你好"
```

## 使用示例

```rust,no_run
use agent_assembly::DbAgentSource;
use agent_base::runtime::AgentSource;

// 1. 构建数据库源
let source = DbAgentSource::new(db.clone(), agent_id, user_id);

// 2. 加载完整装配包
let bundle = source.load().await?;

// 3. 构建并运行 Agent
let agent = Agent::from_bundle(bundle)?;
let answer = agent.run("session-1", "你好").await?;
```

## 依赖关系

- **`agent-base`** — 运行时基座，提供 Agent Runtime、Planner、Executor 等核心能力
- **`agent-entity`** — Agent 域数据库实体定义（SeaORM Entity）
- **`database`** — 数据库连接池
- **`rag-core`** — RAG 知识检索核心
- **`rig-core`** — LLM 推理引擎
