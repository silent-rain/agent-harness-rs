# agent-runtime 工具系统重构：以 rig-core 为核心

## 功能说明

将 `agent-runtime` 的工具处理系统从自研的 `ToolExecutor` / `AgentFunctionRegistry` 架构，
全面切换为以 `rig-core` 为核心的 `rig::Tool` / `rig::ToolSet` 架构。
同时，MCP 工具调用也通过 `rig-core` 的 `McpTool` / `McpClientHandler` 原生支持。

## 新增内容

### `agent-runtime`

| 新增项 | 说明 |
|--------|------|
| `executor::AgentToolSet` | `rig::ToolSet` 的类型别名，作为统一工具注册表 |
| `tools::builtin_toolset(guard)` | 一键构建包含所有内置工具的 `ToolSet` |
| `tools::ReadFileTool::new(guard)` | 构造时注入 `WorkspaceGuard` |
| `tools::ListDirTool::new(guard)` | 构造时注入 `WorkspaceGuard` |
| `tools::RunCommandTool::new(guard)` | 构造时注入 `WorkspaceGuard` |
| `tools::SearchFilesTool::new(guard)` | 构造时注入 `WorkspaceGuard` |

### `agent-platform`

| 新增项 | 说明 |
|--------|------|
| `resolver::AgentRuntimeResolver::build_toolset` | 基于 `WorkspaceGuard` 构建 `ToolSet` |

## 修改内容

### `agent-runtime/Cargo.toml`

- `rig-core` 从 `optional` 改为**必选依赖**（核心依赖）
- 移除 `rig-planner` feature（rig-core 现在是核心，无需 feature 控制）
- `mcp` feature 改为通过 `rig-core/rmcp` 启用

### `src/executor/mod.rs`

**删除：**

- `ToolExecutor` trait
- `AgentFunctionSchema` 结构体
- `AgentFunctionRegistry` 结构体

**新增：**

- `AgentToolSet = rig::ToolSet`（类型别名）
- `AgentActionDispatcher::new(toolset)` — 参数从 `(registry, guard)` 改为 `(toolset)`
- 工具调用改为 `toolset.call(name, args)` — 直接委托给 rig

### `src/planner/mod.rs`

- `PlannerInput::function_schemas: Vec<AgentFunctionSchema>` → `tool_definitions: Vec<rig::ToolDefinition>`
- 移除 `AgentFunctionSchema` 导入
- `rig_planner` 模块从 `#[cfg(feature = "rig-planner")]` 改为无条件导出

### `src/planner/rig_planner.rs`

- 移除 `convert_tool_definitions` 方法（不再需要 Schema 转换）
- `plan()` 直接使用 `input.tool_definitions`（已是 `rig::ToolDefinition`）

### `src/tools/*.rs`（4 个工具文件）

所有工具从实现 `ToolExecutor` trait 改为实现 `rig::Tool` trait：

| 变化点 | 旧实现 | 新实现 |
|--------|--------|--------|
| 工具 trait | `ToolExecutor` | `rig::tool::Tool` |
| 工具名称 | `fn tool_code() -> &str` | `const NAME: &'static str` |
| Schema 定义 | `fn function_schema() -> AgentFunctionSchema` | `async fn definition() -> ToolDefinition` |
| 执行方法 | `async fn execute(json, guard)` | `async fn call(args: Self::Args)` |
| 安全守卫 | 每次调用时传入 `&WorkspaceGuard` | 构造时注入 `Arc<WorkspaceGuard>` |
| 参数类型 | 手动 `serde_json::from_str` | rig 自动反序列化为 `Self::Args` |
| 返回类型 | `ActionObservation` | `Self::Output`（自动序列化为 JSON） |

### `src/runtime/agent_runtime.rs`

- `registry: Arc<AgentFunctionRegistry>` → `toolset: Arc<AgentToolSet>`
- `AgentRuntime::new(...)` 参数 `registry` → `toolset`
- `AgentRuntime::without_memory(...)` 参数 `registry` → `toolset`
- `PlannerInput::function_schemas` → `tool_definitions`（通过 `toolset.get_tool_definitions().await` 获取）
- `AgentActionDispatcher::new(registry, guard)` → `AgentActionDispatcher::new(toolset)`

### `agent-platform/Cargo.toml`

- 移除 `rig-planner` feature
- `rig-core` 从 `optional` 改为必选依赖

### `agent-platform/src/resolver.rs`

- `build_registry()` → `build_toolset()`
- 使用 `builtin_toolset(guard)` 自动注册所有内置工具
- `WorkspaceGuard` 从快照的 `workspace_policy` 构建

## 注意事项

1. **`rig::Tool` 关联类型可见性**：`Args` 和 `Output` 关联类型必须是 `pub`，否则编译报 `E0446`。
2. **`WorkspaceGuard` 注入时机**：守卫现在在工具构造时注入（`Arc<WorkspaceGuard>`），而非每次调用时传入。这意味着同一个工具实例绑定到固定的工作区策略。
3. **MCP 支持**：启用 `mcp` feature 后，可通过 `rig-core` 的 `McpClientHandler` 连接 MCP server，`McpTool` 实现了 `ToolDyn`，可直接通过 `toolset.add_tool_boxed(mcp_tool)` 注册。
4. **`rag-service` 编译错误**：工作区中 `rag-service/src/service/dataset_retrieval.rs` 存在预先存在的语法错误，与本次重构无关。

## 使用示例

### 示例 1：注册内置工具并运行 Agent

```rust,no_run
use std::sync::Arc;
use agent_runtime::{
    AgentRuntime, NoopEventPublisher, WorkspaceGuard,
    domain::{AgentDefinitionSnapshot, WorkspaceRuntimePolicy},
    tools::builtin_toolset,
};

async fn run_agent(snapshot: AgentDefinitionSnapshot, planner: Arc<dyn agent_runtime::AgentPlanner>) {
    // 构建工作区守卫
    let guard = Arc::new(WorkspaceGuard::new(snapshot.workspace_policy.clone()));

    // 一键构建包含所有内置工具的 ToolSet
    let toolset = Arc::new(builtin_toolset(guard));

    let runtime = AgentRuntime::without_memory(
        snapshot,
        planner,
        toolset,
        Arc::new(NoopEventPublisher),
    );

    let result = runtime.run("session-1", "列出 /tmp 目录的内容").await;
    println!("{:?}", result);
}
```

### 示例 2：自定义工具实现 rig::Tool

```rust,no_run
use std::sync::Arc;
use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use agent_runtime::{error::AgentError, policy::WorkspaceGuard};

pub struct MyCustomTool {
    guard: Arc<WorkspaceGuard>,
}

#[derive(Deserialize)]
pub struct MyArgs {
    input: String,
}

#[derive(Serialize)]
pub struct MyOutput {
    result: String,
}

impl Tool for MyCustomTool {
    const NAME: &'static str = "my_custom_tool";
    type Error = AgentError;
    type Args = MyArgs;
    type Output = MyOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "my_custom_tool".to_string(),
            description: "自定义工具示例".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "输入内容" }
                },
                "required": ["input"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(MyOutput {
            result: format!("处理结果: {}", args.input),
        })
    }
}

// 注册到 ToolSet
fn register_custom_tool(guard: Arc<WorkspaceGuard>) -> rig::tool::ToolSet {
    let mut toolset = rig::tool::ToolSet::default();
    toolset.add_tool(MyCustomTool { guard });
    toolset
}
```

### 示例 3：启用 MCP 工具（需 `mcp` feature）

```rust,no_run
#[cfg(feature = "mcp")]
async fn setup_mcp_tools() -> rig::tool::ToolSet {
    use rig_core::tool::server::ToolServer;
    use rig_core::tool::rmcp::{McpClientHandler};
    use rmcp::model::ClientInfo;

    // 创建 ToolServer 并获取 handle
    let tool_server_handle = ToolServer::new().run();

    // 创建 MCP 客户端处理器
    let handler = McpClientHandler::new(
        ClientInfo::default(),
        tool_server_handle.clone(),
    );

    // 连接到 MCP server（stdio 传输示例）
    let transport = rmcp::transport::TokioChildProcess::new(
        tokio::process::Command::new("my-mcp-server")
    ).unwrap();

    let _service = handler.connect(transport).await.unwrap();

    // 从 handle 获取 ToolSet（包含 MCP 工具）
    tool_server_handle.get_toolset().await
}
```

## 兼容性说明

| 项目 | 变化 |
|------|------|
| `AgentFunctionRegistry` | **已删除**，替换为 `rig::ToolSet`（`AgentToolSet`） |
| `AgentFunctionSchema` | **已删除**，替换为 `rig::ToolDefinition` |
| `ToolExecutor` trait | **已删除**，替换为 `rig::Tool` trait |
| `PlannerInput::function_schemas` | **已重命名**为 `tool_definitions`，类型改为 `Vec<rig::ToolDefinition>` |
| `AgentRuntime::new(registry, ...)` | **参数变更**：`registry` → `toolset`（类型 `Arc<AgentToolSet>`） |
| `rig-planner` feature | **已移除**，`rig-core` 现在是必选依赖 |
| `RigAgentPlanner` / `RigPlannerConfig` | **无条件导出**（不再需要 feature flag） |
