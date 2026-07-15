# AgentRuntime Builder 模式重构

> **日期**：2026-04-20
> **适用仓库**：`nucleus-rs`
> **目标读者**：Rust 开发人员、Agent 模块维护者

---

## 1. 功能说明

将 `AgentRuntime` 的构建统一为 Builder 模式，移除 `new()` 和 `from_snapshot()` 便捷方法。

**核心改进**：

- **统一入口**：所有构建都通过 `AgentRuntime::builder(...).build().await`
- **自动初始化**：`build()` 内部自动完成：

  1. 从 `snapshot.tool_configs` 构建 `ToolRegistry` 并注入工具
  2. （`mcp` feature 启用时）从 `snapshot.mcp_services` 连接 MCP 服务并注入工具
  3. 将 `ToolSet` 包装为 `Arc<ToolSet>` 生成不可变的 `AgentRuntime`

- **可组合**：可在 `build()` 前链式调用 `.with_tool_registry()` 注入额外工具
- **职责清晰**：Builder 负责所有构建逻辑，`AgentRuntime` 只负责运行

---

## 2. 新增内容

### 2.1 `build()` 自动初始化工具

`build()` 现在是 `async fn`，内部自动完成：

1. 从 `snapshot.tool_configs` 构建 `ToolRegistry` 并注入工具
2. （`mcp` feature 启用时）从 `snapshot.mcp_services` 连接 MCP 服务并注入工具
3. 将 `ToolSet` 包装为 `Arc<ToolSet>` 生成不可变的 `AgentRuntime`

---

## 3. 修改内容

### 3.1 移除的 API

| 移除的方法 | 替代方式 |
|---|---|
| `AgentRuntime::new(snapshot, model, publisher, services)` | `AgentRuntime::builder(snapshot, model, publisher, services).build().await` |
| `AgentRuntime::from_snapshot(snapshot, model, publisher, services).await` | `AgentRuntime::builder(snapshot, model, publisher, services).build().await` |
| `AgentRuntime::with_max_turns(n)` | `AgentRuntime::builder(...).with_max_turns(n).build().await` |

### 3.2 迁移示例

**旧 API：**

```rust
// 简单场景
let runtime = AgentRuntime::new(snapshot, model, publisher, services);

// 自动装配
let runtime = AgentRuntime::from_snapshot(snapshot, model, publisher, services).await;

// 自定义工具
let runtime = AgentRuntime::builder(snapshot, model, publisher, services)
    .with_tool_registry(registry)
    .build();
```

**新 API：**

```rust
// 简单场景（build 自动初始化 snapshot 中的工具）
let runtime = AgentRuntime::builder(snapshot, model, publisher, services)
    .build()
    .await;

// 注入额外工具
let runtime = AgentRuntime::builder(snapshot, model, publisher, services)
    .with_tool_registry(extra_registry)
    .with_max_turns(5)
    .build()
    .await;
```

### 3.3 文件变更清单

| 文件 | 变更 |
|------|------|
| `agent-runtime/src/runtime/agent_runtime.rs` | `build()` → `async fn build()`，自动初始化工具；移除 `from_snapshot`、`new()`、`AgentRuntime::with_max_turns()` |
| `agent-runtime/src/mcp.rs` | 更新注释引用 |
| `agent-runtime/examples/basic_agent.rs` | `new()` → `builder().build().await` |
| `agent-runtime/examples/streaming_agent.rs` | `new()` → `builder().build().await` |
| `agent-runtime/examples/tool_agent.rs` | `.build()` → `.build().await` |
| `agent-platform/src/scheduler.rs` | `from_snapshot()` → `builder().build().await` |
| `agent-platform/tests/integration_test.rs` | `from_snapshot()` → `builder().build().await` |
| `agent-service/src/service/agent_run.rs` | `from_snapshot()` → `builder().build().await` |

---

## 4. 注意事项

- `build()` 现在是 **async** 方法，所有调用方需要 `.await`
- 之前的同步测试（`#[test]`）需改为 `#[tokio::test]`
- `with_mcp_registry()` 和 `with_tool_registry()` 仍可用于注入**额外**工具，`build()` 会在此基础上追加 snapshot 中配置的工具
- `AgentRuntimeBuilder` 不实现 `Clone`（因为 `ToolSet` 不可 Clone）

---

## 5. 使用示例

### 场景 1：简单场景（无额外工具）

```rust
use std::sync::Arc;
use agent_runtime::{AgentRuntime, RuntimeServices, NoopEventPublisher};

let runtime = AgentRuntime::builder(snapshot, model, Arc::new(NoopEventPublisher), RuntimeServices::noop())
    .build()
    .await;
let result = runtime.run("session-1", "你好", 12345).await?;
```

### 场景 2：注入额外自定义工具

```rust
use std::sync::Arc;
use agent_runtime::{AgentRuntime, RuntimeServices, NoopEventPublisher};
use agent_runtime::tools::ToolRegistry;

let extra_tools = ToolRegistry::new()
    .with_builtin_tools(MyCustomTool);

let runtime = AgentRuntime::builder(snapshot, model, Arc::new(NoopEventPublisher), RuntimeServices::noop())
    .with_tool_registry(extra_tools)
    .with_max_turns(20)
    .build()
    .await;
```

### 场景 3：手动注入 MCP 注册表

```rust
use std::sync::Arc;
use agent_runtime::{AgentRuntime, RuntimeServices, NoopEventPublisher};
use agent_runtime::mcp::McpRegistry;

let mcp_registry = McpRegistry::from_configs(&snapshot.mcp_services)
    .await
    .unwrap_or_else(|_| McpRegistry::new());

let runtime = AgentRuntime::builder(snapshot, model, Arc::new(NoopEventPublisher), RuntimeServices::noop())
    .with_mcp_registry(mcp_registry)
    .await
    .build()
    .await;
```

---

## 6. 兼容性说明

| 组件 | 变更类型 | 影响 |
|------|----------|------|
| `AgentRuntime::new()` | **已移除** | 改用 `builder(...).build().await` |
| `AgentRuntime::from_snapshot()` | **已移除** | 改用 `builder(...).build().await`（build 自动初始化） |
| `AgentRuntime::with_max_turns()` | **已移除** | 改用 `builder(...).with_max_turns(n)` |
| `AgentRuntimeBuilder::build()` | **变为 async** | 所有调用方需添加 `.await` |

### 架构变更对比

**重构前：**

```text
AgentRuntimeBuilder
├── new(snapshot, model, publisher, services)
├── with_max_turns(max_turns)
├── with_mcp_registry(registry)
├── with_tool_registry(registry)
└── build() → AgentRuntime              ← 同步，不自动初始化工具

AgentRuntime
├── builder(...)  → AgentRuntimeBuilder
├── from_snapshot(...)  → Self           ← 便捷方法，自动初始化
├── new(...)  → Self                     ← 简单场景，无工具
├── with_max_turns(max_turns)
└── run(session_id, user_input, user_id)
```

**重构后：**

```text
AgentRuntimeBuilder
├── new(snapshot, model, publisher, services)
├── with_max_turns(max_turns)
├── with_mcp_registry(registry)          ← 注入额外 MCP 工具
├── with_tool_registry(registry)         ← 注入额外本地工具
└── async build() → AgentRuntime         ← 自动初始化 snapshot 中的工具

AgentRuntime
├── builder(...)  → AgentRuntimeBuilder  ← 唯一构建入口
└── run(session_id, user_input, user_id)
```
