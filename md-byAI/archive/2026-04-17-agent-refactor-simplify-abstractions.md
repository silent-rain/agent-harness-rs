# Agent 重构：精简抽象层与统一运行时服务

> **日期**：2026-04-17
> **适用仓库**：`nucleus-rs`
> **目标读者**：Rust 开发人员、Agent 模块维护者

---

## 1. 功能说明

本次重构对 Agent 智能体系统进行了全面的 API 精简，核心目标是：

- **降低接入成本**：开发者只需了解 `AgentRuntime::new()` + `RuntimeServices` 即可构建运行时
- **减少概念数量**：删除了不必要的包装层抽象
- **让每个模块职责清晰明了**：保留跨层边界抽象，删除仅包装一层但不增加职责的抽象

**本次重构不向后兼容。**

---

## 2. 新增内容

### 2.1 `RuntimeServices` — 运行时服务聚合

新增 `RuntimeServices` 结构体，聚合所有可选的运行时依赖：

```rust
use agent_runtime::RuntimeServices;

// 全部使用 Noop 实现（测试/SDK 场景）
let services = RuntimeServices::noop();

// 按需替换具体实现
let services = RuntimeServices {
    memory_writer: Arc::new(my_memory_writer),
    memory_retriever: Arc::new(my_memory_retriever),
    knowledge_retriever: Arc::new(my_knowledge_retriever),
    sub_agent_scheduler: Arc::new(my_scheduler),
    run_recorder: Arc::new(my_recorder),
    tool_audit_logger: Arc::new(my_logger),
};

// 也可以用 struct update 语法只替换部分
let services = RuntimeServices {
    run_recorder: Arc::new(my_recorder),
    ..RuntimeServices::noop()
};
```

### 2.2 `ToolCallResult` — 运行时内部类型

工具执行结果从公开的 `ActionObservation` 降级为 `AgentRuntime` 内部私有类型 `ToolCallResult`，不再暴露给外部。

---

## 3. 修改内容

### 3.1 `AgentRuntime` 构造方式变更

**旧 API（已删除）：**

```rust
// 多种构造入口
let runtime = AgentRuntime::without_memory(snapshot, planner, toolset, publisher);
let runtime = AgentRuntime::new(snapshot, planner, toolset, publisher, memory_writer, memory_retriever);
let runtime = runtime
    .with_knowledge_retriever(knowledge_retriever)
    .with_sub_agent_scheduler(scheduler)
    .with_run_recorder(recorder)
    .with_tool_audit_logger(logger);
```

**新 API（唯一入口）：**

```rust
let runtime = AgentRuntime::new(
    snapshot,
    planner,
    toolset,
    publisher,
    RuntimeServices::noop(),  // 或自定义 services
);
```

### 3.2 `executor` 模块已删除

| 删除项 | 说明 |
|--------|------|
| `AgentActionDispatcher` | 工具执行逻辑内联到 `AgentRuntime` |
| `AgentToolSet` 类型别名 | 直接使用 `rig::tool::ToolSet` |
| `executor` 模块 | 整个模块已移除 |

### 3.3 `ActionObservation` 从公开 API 移除

`ActionObservation` 不再从 `planner` 模块导出，也不再从 `agent_runtime` crate 根导出。工具执行结果改为运行时内部类型。

### 3.4 `AgentRuntimeResolver` 简化

| 旧 API | 新 API |
|--------|--------|
| `resolve(snapshot)` (同步) | 已删除 |
| `resolve_async(snapshot)` (异步) | `resolve(snapshot, services)` (异步) |
| `ResolverConfig` 结构体 | 已删除，改为 `register_builtin_tools: bool` 参数 |

**新用法：**

```rust
let resolver = AgentRuntimeResolver::with_planner(planner);
let (runtime, mcp_guards) = resolver.resolve(snapshot, services).await?;
```

---

## 4. 注意事项

1. **不向后兼容**：所有使用旧 API 的代码必须迁移
2. **`rig::tool::ToolSet` 直接使用**：不再有 `AgentToolSet` 别名，需要 `use rig_core::tool::ToolSet`
3. **`RuntimeServices::noop()`** 是测试和 SDK 场景的推荐入口
4. **`RuntimeServices` 实现了 `Clone`**：可以安全地在多个运行时之间共享
5. **`RuntimeServices` 实现了 `Default`**：`Default::default()` 等价于 `RuntimeServices::noop()`

---

## 5. 使用示例

### 5.1 最简单的智能体（测试/SDK 场景）

```rust
use std::sync::Arc;
use rig_core::tool::ToolSet;
use agent_runtime::{
    AgentRuntime, RuntimeServices,
    runtime::NoopEventPublisher,
};

let runtime = AgentRuntime::new(
    snapshot,
    Arc::new(my_planner),
    Arc::new(ToolSet::default()),
    Arc::new(NoopEventPublisher),
    RuntimeServices::noop(),
);
let answer = runtime.run("session-1", "你好", 12345).await?;
```

### 5.2 带完整平台能力的智能体

```rust
use agent_runtime::{AgentRuntime, RuntimeServices};

let services = RuntimeServices {
    memory_writer: Arc::new(db_memory_writer),
    memory_retriever: Arc::new(db_memory_retriever),
    knowledge_retriever: Arc::new(knowledge_adapter),
    sub_agent_scheduler: Arc::new(platform_scheduler),
    run_recorder: Arc::new(db_run_recorder),
    tool_audit_logger: Arc::new(db_tool_audit_logger),
};

let runtime = AgentRuntime::new(snapshot, planner, toolset, publisher, services);
let answer = runtime.run("session-1", "分析这段代码", user_id).await?;
```

### 5.3 通过 Resolver 装配（平台层）

```rust
use agent_platform::AgentRuntimeResolver;
use agent_runtime::RuntimeServices;

let resolver = AgentRuntimeResolver::with_planner(planner);
let (runtime, mcp_guards) = resolver.resolve(snapshot, services).await?;
let answer = runtime.run("session-1", "你好", user_id).await?;
// mcp_guards 需要持有以保持 MCP 连接存活
```

---

## 6. 兼容性说明

| 组件 | 变更类型 | 影响 |
|------|----------|------|
| `AgentRuntime::new()` | 签名变更 | 所有调用方需更新 |
| `AgentRuntime::without_memory()` | 已删除 | 改用 `new() + RuntimeServices::noop()` |
| `AgentRuntime::with_xxx()` | 已删除 | 改用 `RuntimeServices` 字段 |
| `AgentActionDispatcher` | 已删除 | 无需替代，逻辑已内联 |
| `AgentToolSet` | 已删除 | 直接使用 `rig::tool::ToolSet` |
| `ActionObservation` | 已内收 | 不再公开，无需使用 |
| `ResolverConfig` | 已删除 | 改用构造函数参数 |
| `resolver.resolve()` (同步) | 已删除 | 改用 `resolver.resolve()` (异步) |
| `resolver.resolve_async()` | 重命名 | 改为 `resolver.resolve()` |
| `RuntimeServices` | 新增 | 统一的服务聚合对象 |

### 架构变更对比

**重构前：**

```text
AgentRuntime
├── new(snapshot, planner, toolset, publisher, memory_writer, memory_retriever)
├── without_memory(snapshot, planner, toolset, publisher)
├── with_knowledge_retriever(...)
├── with_sub_agent_scheduler(...)
├── with_run_recorder(...)
└── with_tool_audit_logger(...)

executor/
├── AgentToolSet (= rig::ToolSet)
└── AgentActionDispatcher

planner/
└── ActionObservation (public)
```

**重构后：**

```text
AgentRuntime
└── new(snapshot, planner, toolset, publisher, services)

RuntimeServices
├── memory_writer
├── memory_retriever
├── knowledge_retriever
├── sub_agent_scheduler
├── run_recorder
└── tool_audit_logger

RuntimeServices::noop()  // 全部 Noop 默认值
RuntimeServices::default()  // 等价于 noop()
```
