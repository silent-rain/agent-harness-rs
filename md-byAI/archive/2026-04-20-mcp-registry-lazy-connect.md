# McpRegistry 延迟连接优化 — 2026-04-20

## 1. 功能说明

将 `McpRegistry` 的 API 从「创建即连接」改为「配置注册 → 延迟连接」两阶段模式，
统一 `from_configs` 和 `new` + `connect` 两种使用路径的行为一致性。

核心变更：

- **`from_configs` 改为同步方法**：仅注册配置到 pending 队列，不执行 I/O 连接
- **新增 `connect_all`**：批量连接所有 pending 配置（异步）
- **新增 `connect_service`**：按名称连接单个 pending 服务（异步）
- **新增 `from_configs_connected`**：便捷方法，等价于旧的 `from_configs`（创建 + 连接）
- **新增 Builder 方法**：`with_configs` / `with_config` 支持链式追加配置
- **`AgentRuntimeBuilder::build` 优化**：使用 builder 中已存储的 registry，避免重复创建

## 2. 新增内容

### 2.1 `McpRegistry::from_configs`（同步版本）

```rust
// 仅注册配置，不连接（同步）
let registry = McpRegistry::from_configs(&configs);
assert_eq!(registry.pending_count(), configs.len());
assert_eq!(registry.connection_count(), 0);
```

### 2.2 `McpRegistry::connect_all`

```rust
let mut registry = McpRegistry::from_configs(&configs);
registry.connect_all().await?;
// 所有 pending 配置已连接
assert_eq!(registry.pending_count(), 0);
```

### 2.3 `McpRegistry::connect_service`

```rust
let mut registry = McpRegistry::from_configs(&configs);
// 按需连接单个服务
registry.connect_service("my-service").await?;
// 其他服务稍后连接
registry.connect_all().await?;
```

### 2.4 `McpRegistry::from_configs_connected`（便捷方法）

```rust
// 一步到位（向后兼容旧 from_configs 行为）
let registry = McpRegistry::from_configs_connected(&configs).await?;
```

### 2.5 Builder 方法

```rust
let registry = McpRegistry::new()
    .with_client(custom_client)
    .with_configs(&base_configs)
    .with_config(extra_config);
// 配置已注册，稍后连接
```

### 2.6 `McpRegistry::pending_count`

```rust
let registry = McpRegistry::from_configs(&configs);
println!("待连接: {}", registry.pending_count());
```

## 3. 修改内容

### 3.1 `McpRegistry` 结构体

| 变更 | 说明 |
|------|------|
| 新增 `pending_configs` 字段 | `Vec<McpServiceConfig>`，存储待连接的服务配置 |
| `from_configs` 签名变更 | `async fn` → `fn`（同步，不再执行连接） |

### 3.2 `AgentRuntimeBuilder::new`

```rust
// 旧代码（编译错误：borrow after move）
Self {
    snapshot,
    mcp_registry: McpRegistry::from_configs(&snapshot.mcp_services), // ← async 调用
    ...
}

// 新代码（同步，无 borrow 问题）
let mcp_registry = McpRegistry::from_configs(&snapshot.mcp_services);
let tool_registry = ToolRegistry::from_configs(&snapshot.tool_configs);
Self { snapshot, mcp_registry, tool_registry, ... }
```

### 3.3 `AgentRuntimeBuilder::build`

```rust
// 旧代码：重新创建 registry
let mcp_registry = McpRegistry::from_configs(&self.snapshot.mcp_services).await...;

// 新代码：使用 builder 中已存储的 registry
if let Err(e) = self.mcp_registry.connect_all().await { ... }
let mcp_toolset = self.mcp_registry.build_toolset().await;
```

## 4. 注意事项

1. **API 不向后兼容**：`from_configs` 不再是 async，旧代码中的 `.await` 需要移除
2. **替代方案**：如需旧行为（创建即连接），使用 `from_configs_connected`
3. **pending 语义**：`connect_all` 会 `take` 所有 pending 配置，调用后 pending 队列清空
4. **连接失败处理**：`connect_all` 中单个服务连接失败只记录 warning，不影响其他服务

## 5. 使用示例

### 示例 1：分步构建（推荐）

```rust
use agent_base::mcp::McpRegistry;

let mut registry = McpRegistry::from_configs(&configs);
registry.connect_all().await?;
let toolset = registry.build_toolset().await;
```

### 示例 2：快捷方式（一步到位）

```rust
let registry = McpRegistry::from_configs_connected(&configs).await?;
let toolset = registry.build_toolset().await;
```

### 示例 3：按需连接 + 动态追加

```rust
let mut registry = McpRegistry::new()
    .with_configs(&base_configs)
    .with_config(extra_config);

// 先连接关键服务
registry.connect_service("critical-service").await?;

// 稍后连接其余服务
registry.connect_all().await?;
```

## 6. 兼容性说明

| 组件 | 兼容性 | 迁移方式 |
|------|--------|---------|
| `McpRegistry::from_configs` | ❌ 不兼容 | 移除 `.await`，或改用 `from_configs_connected` |
| `McpRegistry::new` | ✅ 兼容 | 无需修改 |
| `McpRegistry::connect` | ✅ 兼容 | 无需修改 |
| `McpRegistry::with_client` | ✅ 兼容 | 无需修改 |
| `AgentRuntimeBuilder` | ✅ 内部兼容 | 无需修改（内部已适配） |
