# Agent Harness

**Harness Engineering Agent** — 智能体基座工程，包含构建、装配与运行 AI Agent 的完整基础设施。

## 概述

`agent-harness` 是一个独立的智能体工程域，采用 **Harness Engineering（线束工程）** 的设计理念：像电气线束将各种电子元件连接成完整系统一样，将 LLM 模型、工具、规则、技能、知识库等原子能力"编织"成可运行的智能体。

## 架构总览

```text
agent-harness-rs/
├── crates/
│   ├── core/                  ← 核心基础设施层（按职责拆分的子 crate 集合，非独立 crate）
│   │   ├── entity/            ← 数据库实体定义（SeaORM Entity）
│   │   ├── migration/         ← 数据库迁移（Agent 域所有表的 DDL）
│   │   ├── err-code/          ← 统一业务错误码定义
│   │   └── inject/            ← 依赖注入辅助（nject + sea-orm 连接管理）
│   ├── agent-base/            ← 运行时内核（Runtime / Planner / Executor / Streaming / Hook）
│   └── agent-assembly/        ← 装配层（从 DB/配置源组装可运行的 Agent）
└── services/
    └── agent-service/         ← HTTP/SSE 服务层（Controller / DAO / DTO / Router）
```

## 分层设计

| 层级               | Crate            | 职责                                                                      |
| ------------------ | ---------------- | ------------------------------------------------------------------------- |
| **核心基础设施层** | `core/entity`    | Agent 域所有数据库表的 SeaORM Entity 定义                                 |
|                    | `core/migration` | Agent 域数据库表结构迁移（DDL），所有表统一使用 `t_agent_` 前缀           |
|                    | `core/err-code`  | 统一业务错误码（`Error`）定义，对接 `axum-response` / `tonic`             |
|                    | `core/inject`    | 基于 `nject` 的依赖注入 Provider，管理数据库连接与服务装配                |
| **运行时内核**     | `agent-base`     | 定义智能体核心抽象与执行引擎：Runtime、Planner、Executor、Streaming、Hook |
| **装配层**         | `agent-assembly` | 从数据库/配置源加载资源，组装 `AgentBundle`，构建可运行的 Agent 实例      |
| **服务层**         | `agent-service`  | 对外 HTTP/SSE 接口，提供 Agent CRUD、运行、会话管理等 API                 |

## 依赖关系

```text
agent-service
  ├── agent-assembly
  │     ├── agent-base
  │     └── core/entity
  ├── core/entity
  ├── core/err-code
  └── core/inject

core/migration
  └── core/entity
```

- `agent-base` 是纯运行时库，**不依赖数据库**，可独立用于本地/嵌入式场景
- `agent-assembly` 是装配层，负责将数据库配置"编织"为运行时快照
- `core/entity` 与 `core/migration` 提供数据持久化基础设施；`core/err-code` 提供统一错误码；`core/inject` 提供依赖注入装配
- `agent-service` 是面向用户的 HTTP 服务入口

## 核心能力

- 🧠 **多模型适配** — 支持 OpenAI 兼容协议的任意 LLM
- 🔧 **工具调用** — 基于 `rig-core` 的 function calling 机制
- 📏 **规则引擎** — 可配置的行为约束规则
- 🎯 **技能系统** — 本地技能包加载与执行（含 zip/sha256 校验）
- 🔗 **MCP 协议** — 支持 SSE / HTTP / stdio 传输的 MCP 服务连接
- 🤖 **子智能体编排** — 支持 Chain / Parallel / Steerable 等组合模式
- 📚 **知识检索** — RAG 知识库集成
- 💬 **流式输出** — 原生 SSE 流式事件协议
- 🪝 **生命周期钩子** — 可扩展的 Agent 运行时 Hook 机制

## 快速开始

### 环境准备

```bash
cd agent-harness/crates/agent-assembly
cp .env.example .env
# 编辑 .env 填入数据库连接和 LLM API Key
```

### 运行示例

```bash
# 纯运行时示例（无需数据库）
cargo run -p agent-base --example basic_agent --features rule

# 从数据库装配 Agent 并运行
cargo run -p agent-assembly --example db_agent -- --prompt "你好"

# 流式输出
cargo run -p agent-assembly --example db_agent_streaming -- --prompt "你好"
```

## 设计原则

1. **Harness Engineering** — 像线束工程一样，将分散的原子能力编织为完整系统
2. **分层解耦** — 运行时内核不依赖数据库，装配层不依赖 HTTP 框架
3. **流式优先** — 所有运行时交互原生支持 SSE 流式事件
4. **动作结构化** — 所有可执行动作通过 function calling 触发，禁止解析自由文本
5. **可独立部署** — 设计为可独立仓库迁移，与宿主项目松耦合
