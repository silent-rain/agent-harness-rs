# MCP 模块重构：目录化 + 内省工具

## 功能说明

将原有的 `mcp.rs` 单文件模块重构为 `mcp/` 目录模块，并新增两个 LLM 可调用的 MCP 内省工具，使 Agent 能够在运行时动态查询已连接的 MCP 服务及其可用工具。

## 新增内容

### 文件结构

```
crates/core/agent-base/src/mcp/
├── mod.rs          # 模块入口，re-export 公共类型
├── registry.rs     # 原 mcp.rs 内容（McpRegistry、McpConnection）
└── tools.rs        # MCP 内省工具（ListMcpServicesTool、ListMcpServiceToolsTool）
```

### 新增工具

1. **`ListMcpServicesTool`** (`list_mcp_services`)
   - 参数：无
   - 返回：所有已连接 MCP 服务的名称、传输类型、可用工具数量
   - 用途：让 LLM 了解当前有哪些 MCP 服务可用

2. **`ListMcpServiceToolsTool`** (`list_mcp_service_tools`)
   - 参数：`service_name: String`
   - 返回：指定服务下所有可用工具的名称和描述
   - 用途：让 LLM 查询特定服务的工具能力

### 新增类型

- `McpServiceInfo` — MCP 服务摘要信息（name, transport_type, tool_count）
- `McpToolInfo` — MCP 工具摘要信息（name, description）
- `ListMcpServicesArgs` / `ListMcpServicesOutput` — 工具参数和输出
- `ListMcpServiceToolsArgs` / `ListMcpServiceToolsOutput` — 工具参数和输出

## 修改内容

### `mcp/registry.rs`（原 `mcp.rs`）

- 更新模块文档，反映新的架构
- **`build_toolset()`** 方法重构：在构建 ToolSet 时，除了注入远端 MCP 工具外，自动注入两个内省工具
- 内省工具采用**快照模式**：在 `build_toolset()` 时捕获当前服务和工具信息，无需持有 `McpRegistry` 引用

### `lib.rs`

- 更新 `mcp` 模块的 re-export，新增 `ListMcpServicesTool` 和 `ListMcpServiceToolsTool`

## 注意事项

1. 两个内省工具仅在 `feature = "mcp"` 启用时可用
2. 内省工具采用快照模式，数据在 `build_toolset()` 时固定，不会反映运行时动态变化（当前架构中 MCP 连接在 `build()` 后不可变，所以不是问题）
3. 内省工具始终自动注入，无需通过 `tool_configs` 配置
4. 对外部 API 完全兼容，`McpRegistry` 的公共接口不变
5. `agent_base::mcp::McpRegistry` 路径保持不变

## 使用示例

### 示例 1：默认集成（自动注入，无需额外配置）

```rust
use std::sync::Arc;
use agent_base::{AgentRuntime, RuntimeServices, NoopEventPublisher};
use agent_base::types::AgentDefinitionSnapshot;
use agent_base::planner::ModelAdapter;

async fn example(
    snapshot: AgentDefinitionSnapshot,
    model: Arc<dyn ModelAdapter>,
) -> agent_base::error::AgentResult<()> {
    // build() 自动连接 MCP 服务并注入内省工具
    let runtime = AgentRuntime::builder(
        snapshot, model, Arc::new(NoopEventPublisher), RuntimeServices::noop()
    )
    .build()
    .await?;
    // LLM 现在可以调用 list_mcp_services 和 list_mcp_service_tools
    Ok(())
}
```

### 示例 2：手动构建 McpRegistry

```rust
use agent_base::mcp::McpRegistry;
use agent_base::types::McpServiceConfig;

async fn example() -> agent_base::error::AgentResult<()> {
    let configs: Vec<McpServiceConfig> = vec![/* ... */];
    let mut registry = McpRegistry::new();
    registry.connects(configs).await?;

    // build_toolset() 返回的 ToolSet 已包含内省工具
    let toolset = registry.build_toolset().await;
    Ok(())
}
```

### 示例 3：直接使用内省工具（测试场景）

```rust
use std::collections::HashMap;
use agent_base::mcp::{ListMcpServicesTool, ListMcpServiceToolsTool};
use agent_base::mcp::tools::{McpServiceInfo, McpToolInfo};

fn example() {
    // 构建 ListMcpServicesTool
    let services = vec![McpServiceInfo {
        name: "filesystem".to_string(),
        transport_type: "stdio".to_string(),
        tool_count: 3,
    }];
    let _tool = ListMcpServicesTool::new(services);

    // 构建 ListMcpServiceToolsTool
    let mut map = HashMap::new();
    map.insert("filesystem".to_string(), vec![
        McpToolInfo { name: "read_file".to_string(), description: "Read a file".to_string() },
    ]);
    let _tool = ListMcpServiceToolsTool::new(map);
}
```

## 兼容性说明

- **完全向后兼容**：所有现有的公共 API 路径保持不变
- `agent_base::mcp::McpRegistry` — 路径不变
- `agent_base::McpRegistry` — re-export 不变
- `McpRegistry::build_toolset()` — 签名不变，返回的 `ToolSet` 中额外包含两个内省工具
- 不启用 `mcp` feature 时，整个 `mcp` 模块不参与编译，无影响
