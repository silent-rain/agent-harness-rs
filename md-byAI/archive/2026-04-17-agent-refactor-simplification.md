# Agent 重构文档 — 2026-04-17

## 1. 功能说明

本次重构对 `agent-runtime` 和 `agent-platform` 两个核心 crate 进行了架构级优化，
目标是**降低智能体接入成本、消除不必要的抽象、让各模块职责清晰明了**。

核心变更：

- **删除 `AgentPlanner` 抽象层**：LLM 调用逻辑内联到 `AgentRuntime`，用户无需感知 Planner
- **引入 `ToolRegistry`**：工具集由运行时根据 `snapshot.tool_configs` 自动构建，用户无需手动传入 `ToolSet`
- **引入 `McpRegistry`**：MCP 连接生命周期由内部管理，对外透明（无需 `McpConnectionGuard`）
- **简化 `AgentRuntime::new` 签名**：只需 `(snapshot, model, publisher, services)` 四个参数
- **重命名 `ToolDefinition` → `ToolConfig`**：避免与 `rig::completion::ToolDefinition` 混淆

## 2. 新增内容

### 2.1 `ToolRegistry`（`agent-runtime/src/tools/mod.rs`）

根据 `ToolConfig` 配置自动构建 `ToolSet` 的注册表。

```rust
use agent_runtime::tools::ToolRegistry;
use agent_runtime::domain::ToolConfig;

// 根据配置构建工具集
let configs: Vec<ToolConfig> = snapshot.tool_configs.clone();
let toolset = ToolRegistry::build_from_configs(&configs, workspace_guard);

// 或者注册所有内置工具
let toolset = ToolRegistry::build_all(workspace_guard);
```

### 2.2 `McpRegistry`（`agent-runtime/src/mcp/mod.rs`）

独立管理 MCP 服务连接生命周期的注册表。

```rust
use agent_runtime::mcp::McpRegistry;
use rig_core::tool::ToolSet;

let registry = McpRegistry::from_configs(&snapshot.mcp_services).await?;
let mut toolset = ToolSet::default();
let count = registry.inject_all_tools(&mut toolset).await?;
println!("Injected {} MCP tools", count);
```

### 2.3 `AgentRuntime::with_extra_tools`

支持注入额外工具（如自定义工具、MCP 工具）的构造方法。

```rust
let mut extra_toolset = ToolSet::default();
extra_toolset.add_tool(MyCustomTool);

let runtime = AgentRuntime::with_extra_tools(
    snapshot, model, publisher, services, extra_toolset,
);
```

### 2.4 `AgentRuntime::with_mcp_registry`

设置 MCP 注册表，持有连接生命周期。

```rust
let runtime = AgentRuntime::with_extra_tools(...)
    .with_mcp_registry(Arc::new(mcp_registry));
```

## 3. 修改内容

### 3.1 `domain/mod.rs`

| 变更 | 说明 |
|------|------|
| `ToolDefinition` → `ToolConfig` | 避免与 `rig::completion::ToolDefinition` 混淆 |
| `AgentDefinitionSnapshot.tools` → `tool_configs` | 字段名同步更新 |

### 3.2 `AgentRuntime::new` 签名变更

**旧签名：**

```rust
pub fn new(
    snapshot: AgentDefinitionSnapshot,
    model: Arc<dyn ModelAdapter>,
    toolset: Arc<ToolSet>,           // ← 需要外部构建
    publisher: Arc<dyn EventPublisher>,
    services: RuntimeServices,
) -> Self
```

**新签名：**

```rust
pub fn new(
    snapshot: AgentDefinitionSnapshot,
    model: Arc<dyn ModelAdapter>,
    publisher: Arc<dyn EventPublisher>,
    services: RuntimeServices,
) -> Self
// 工具集由内部根据 snapshot.tool_configs 自动构建
```

### 3.3 `AgentRuntimeResolver::resolve` 返回值变更

**旧返回值：**

```rust
pub async fn resolve(...) -> PlatformResult<(AgentRuntime, Vec<McpConnectionGuard>)>
```

**新返回值：**

```rust
pub async fn resolve(...) -> PlatformResult<AgentRuntime>
// MCP 连接生命周期由 AgentRuntime 内部持有
```

### 3.4 `PlatformSubAgentScheduler` 简化

- 移除 `builtin_toolset` 和 `build_sub_agent_toolset` 函数
- 新增 `build_sub_agent_runtime` 方法，内部处理 MCP 工具注入
- 工具集由 `AgentRuntime` 自动构建

### 3.5 删除的类型/函数

| 删除项 | 替代方案 |
|--------|---------|
| `McpToolsetBuilder` | `McpRegistry` |
| `McpConnectionGuard` | `McpRegistry` 内部管理 |
| `builtin_toolset()` | `ToolRegistry::build_from_configs()` |
| `AgentRuntimeResolver::with_model()` | `AgentRuntimeResolver::new()` |

## 4. 注意事项

1. **不向后兼容**：本次重构不保持向后兼容，所有使用旧 API 的代码需要更新
2. **MCP feature gate**：`McpRegistry` 仍然在 `#[cfg(feature = "mcp")]` 下，`agent-platform` 默认启用
3. **工具自动注册**：`AgentRuntime::new` 会根据 `snapshot.tool_configs` 自动注册内置工具，如需自定义工具请使用 `with_extra_tools`
4. **MCP 连接生命周期**：`McpRegistry` 持有所有 MCP 连接，当 `AgentRuntime` drop 时连接自动关闭

## 5. 使用示例

### 示例 1：最简智能体（无工具）

```rust
use agent_runtime::{AgentRuntime, RuntimeServices, NoopEventPublisher};

let runtime = AgentRuntime::new(
    snapshot,
    model,
    Arc::new(NoopEventPublisher),
    RuntimeServices::noop(),
);

let answer = runtime.run("session-1", "你好", user_id).await?;
```

### 示例 2：带自定义工具的智能体

```rust
use agent_runtime::AgentRuntime;
use rig_core::tool::ToolSet;

let mut extra_toolset = ToolSet::default();
extra_toolset.add_tool(CalculatorTool);
extra_toolset.add_tool(WeatherTool);

let runtime = AgentRuntime::with_extra_tools(
    snapshot, model, publisher, services, extra_toolset,
);

let answer = runtime.run("session-1", "计算 2+3", user_id).await?;
```

### 示例 3：通过 Resolver 装配（平台层）

```rust
use agent_platform::AgentRuntimeResolver;

let resolver = AgentRuntimeResolver::new(model);
let runtime = resolver.resolve(snapshot, services).await?;
// MCP 连接已自动建立并注入，无需关心 guard

let answer = runtime.run("session-1", "你好", user_id).await?;
```

## 6. 兼容性说明

| 组件 | 兼容性 | 迁移方式 |
|------|--------|---------|
| `AgentRuntime::new` | ❌ 不兼容 | 移除 `toolset` 参数 |
| `AgentRuntimeResolver::resolve` | ❌ 不兼容 | 返回值从元组改为单值 |
| `AgentRuntimeResolver::with_model` | ❌ 不兼容 | 改用 `::new()` |
| `ToolDefinition` | ❌ 不兼容 | 改用 `ToolConfig` |
| `snapshot.tools` | ❌ 不兼容 | 改用 `snapshot.tool_configs` |
| `McpToolsetBuilder` | ❌ 已删除 | 改用 `McpRegistry` |
| `McpConnectionGuard` | ❌ 已删除 | 不再需要，由内部管理 |
| `builtin_toolset()` | ❌ 已删除 | 改用 `ToolRegistry::build_from_configs()` |
| `ModelAdapter` trait | ✅ 兼容 | 无变化 |
| `EventPublisher` trait | ✅ 兼容 | 无变化 |
| `RuntimeServices` | ✅ 兼容 | 无变化 |
