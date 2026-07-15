# MCP Loader 重构：LocalMcpLoader 职责分离

## 功能说明

将 `McpRegistry` 中内嵌的本地配置文件扫描逻辑（`scan()` 方法）提取为独立的 `LocalMcpLoader`，
使 MCP 模块的职责划分与 `rule`、`command` 模块完全对称：

```
rule/loader/local.rs    → LocalRuleLoader::load()    → Vec<RuleConfig>
command/loader/local.rs → LocalCommandLoader::load() → Vec<CommandConfig>
mcp/loader/local.rs     → LocalMcpLoader::load()     → Vec<McpServiceConfig>  ← 新增
```

## 新增内容

### 文件结构

```
crates/core/agent-base/src/mcp/
├── mod.rs          # 新增 loader 子模块声明与 LocalMcpLoader 重导出
├── loader/
│   ├── mod.rs      # 新增：模块入口
│   └── local.rs    # 新增：LocalMcpLoader
├── registry.rs     # 精简：去掉 scan/enable_local/mcp_config_path
└── tools/
    └── ...
```

### `LocalMcpLoader`（`mcp/loader/local.rs`）

| 方法 | 说明 |
|---|---|
| `LocalMcpLoader::default()` | 使用默认路径 `.codebuddy/mcp/mcp.json` |
| `LocalMcpLoader::new(path)` | 指定配置文件路径 |
| `with_workspace_dir(dir)` | 切换工作空间根目录（Builder 风格） |
| `config_path()` | 获取当前配置文件路径 |
| `load()` | 读取并解析 `mcp.json`，返回 `Vec<McpServiceConfig>` |

**设计特点：**

- 无状态：仅在 `load()` 时执行 I/O，可安全丢弃
- 文件不存在时返回空列表（不报错）
- 解析失败时返回 `Error::McpConfig`
- 结果按服务名字典序排列，保证确定性

## 修改内容

### `McpRegistry`（`mcp/registry.rs`）

**删除的字段：**

- `enable_local: bool`
- `mcp_config_path: PathBuf`

**删除的方法：**

- `McpRegistry::new(config_path)` → 改用 `McpRegistry::default()`
- `with_local_scan(enable)` → 由 builder 层通过 `LocalMcpLoader` 控制
- `with_workspace_dir(dir)` → 由 `LocalMcpLoader::with_workspace_dir()` 替代
- `scan()` → 迁移至 `LocalMcpLoader::load()`
- `config_path()` → 不再需要
- `is_local_scan_enabled()` → 不再需要

**保留的职责：** 纯连接管理（`add_configs` / `connect_all` / `build_toolset` 等）

### `builder.rs`（`load_mcp` 方法）

改造前：

```rust
// 通过 McpRegistry 内部的 scan() 读取本地配置
self.mcp_registry = std::mem::take(&mut self.mcp_registry)
    .with_local_scan(self.enable_local_scan);
self.mcp_registry.scan().await?;
```

改造后（与 `load_commands` / `load_rules` 对称）：

```rust
// 通过 LocalMcpLoader 读取本地配置，再注入 registry
if self.enable_local_scan {
    let local_configs = LocalMcpLoader::default().load().await?;
    self.mcp_registry.add_configs(local_configs);
}
```

### `sub_agent/registry.rs`（`load_mcp_pool` 方法）

用 `LocalMcpLoader` 替换了手动读取 `mcp.json` 的 40 行代码，简化为：

```rust
let loader = LocalMcpLoader::new(&self.mcp_config_path);
let all_configs = match loader.load().await { ... };
```

## 注意事项

- `McpRegistry` 不再持有任何文件路径，完全专注于连接生命周期管理
- `LocalMcpLoader` 是无状态的，每次调用 `load()` 都会重新读取文件
- 与 `LocalRuleLoader` / `LocalCommandLoader` 不同，MCP 的"加载"（读配置）和"激活"（建连接）是两个独立步骤

## 使用示例

### 场景 1：默认本地配置加载

```rust,no_run
use agent_base::mcp::{LocalMcpLoader, McpRegistry};

async fn example() -> agent_base::error::AgentResult<()> {
    // Step 1: 加载本地配置（纯 I/O，无副作用）
    let configs = LocalMcpLoader::default().load().await?;

    // Step 2: 注入 registry 并建立连接
    let mut registry = McpRegistry::default();
    registry.add_configs(configs);
    registry.connect_all().await?;

    // Step 3: 构建 ToolSet
    let toolset = registry.build_toolset().await;
    Ok(())
}
```

### 场景 2：自定义工作空间路径

```rust,no_run
use agent_base::mcp::LocalMcpLoader;

async fn example() -> agent_base::error::AgentResult<()> {
    // 从自定义工作空间加载
    let configs = LocalMcpLoader::default()
        .with_workspace_dir("/custom/workspace")
        .load()
        .await?;

    println!("Loaded {} MCP services", configs.len());
    Ok(())
}
```

### 场景 3：通过 AgentBuilder 自动装配（无需手动操作）

```rust,no_run
use agent_base::AgentBuilder;

async fn example() {
    // enable_local_scan(true) 会自动触发 LocalMcpLoader 加载本地 mcp.json
    let agent = AgentBuilder::new(model)
        .enable_local_scan(true)
        .build()
        .await
        .unwrap();
}
```

## 兼容性说明

- **破坏性变更**：`McpRegistry::new(path)` 已删除，改用 `McpRegistry::default()`
- **破坏性变更**：`McpRegistry::with_local_scan()` 已删除
- **破坏性变更**：`McpRegistry::scan()` 已删除，改用 `LocalMcpLoader::load()`
- `AgentBuilder` 的公开 API 无变化（`enable_local_scan`、`with_mcp_registry` 等保持不变）
- `LocalMcpLoader` 已通过 `agent_base::mcp::LocalMcpLoader` 公开导出
