
# ToolRegistry 白名单重构：取消内置工具，统一配置驱动

> **日期**：2026-04-21
> **适用仓库**：`nucleus-rs`
> **目标读者**：Rust 开发人员、Agent 模块维护者

---

## 1. 功能说明

将 `ToolRegistry` 从"内置工具 + 候选池 + 配置列表"三层模式，简化为"候选池 + 白名单"两层模式。

**核心改进**：

- **取消内置工具概念**：所有工具统一通过 `with_tool()` 注册到候选池，不再有"始终包含"的特殊通道
- **白名单语义严格**：`Option<HashSet<String>>` 替代 `Vec<ToolConfig>`，明确区分"未设置"和"空列表"
- **配置驱动**：工具的可用性完全由 `snapshot.tool_configs`（来自 `t_agent_tool_registry` + `t_agent_tool_rel`）决定

---

## 2. 新增内容

### 2.1 `ToolRegistry::with_tool()`

替代原有的 `with_tools()` 和 `with_builtin_tools()`，统一注册入口。

```rust
let registry = ToolRegistry::new()
    .add_tool(ReadFileTool)
    .add_tool(ListDirTool)
    .add_tool(RunCommandTool);
```

### 2.2 `ToolRegistry::with_whitelist()`

替代原有的 `from_configs()`，从 `&[ToolConfig]` 中提取 `tool_code` 构建白名单。

```rust
let registry = ToolRegistry::new()
    .add_tool(ReadFileTool)
    .add_tool(ListDirTool)
    .with_whitelist(&snapshot.tool_configs)
    .build_toolset();
```

### 2.3 `ToolRegistry::merge()`

支持将外部 `ToolRegistry` 的候选工具合并到当前注册表，白名单保持不变。

```rust
let extra = ToolRegistry::new().add_tool(CalculatorTool);
let mut main_registry = ToolRegistry::new()
    .add_tool(ReadFileTool)
    .with_whitelist(&configs);
main_registry.merge(extra);
```

---

## 3. 修改内容

### 3.1 移除的 API

| 移除的方法 | 替代方式 |
|---|---|
| `ToolRegistry::from_configs(&configs)` | `ToolRegistry::new().with_whitelist(&configs)` |
| `ToolRegistry::with_builtin_tools(tool)` | `ToolRegistry::with_tool(tool)` |
| `ToolRegistry::with_tools(tool)` | `ToolRegistry::with_tool(tool)` |

### 3.2 白名单语义变更

| 场景 | 旧行为 | 新行为 |
|------|--------|--------|
| `tool_configs` 为空 | 只返回内置工具，候选池被忽略 | `Some(empty_set)` → 不返回任何候选工具 |
| 未设置配置（`new()`） | 与空列表行为相同 | `None` → 候选池全部包含 |
| 配置非空 | 从候选池中按 `tool_code` 筛选 | 行为不变 |

### 3.3 `AgentRuntimeBuilder` 变更

- `new()` 中 `ToolRegistry::from_configs(...)` → `ToolRegistry::new().with_whitelist(...)`
- `with_tool_registry()` 从"替换"改为"合并"（`merge`），外部注入的工具仍受白名单约束

### 3.4 文件变更清单

| 文件 | 变更 |
|------|------|
| `agent-base/src/tools/mod.rs` | 重写 `ToolRegistry`：移除 `builtin_tools`、`from_configs`、`with_builtin_tools`、`with_tools`；新增 `with_tool`、`with_whitelist`、`merge` |
| `agent-base/src/runtime/agent_runtime.rs` | `AgentRuntimeBuilder::new()` 和 `with_tool_registry()` 适配新 API |
| `agent-base/examples/tool_agent.rs` | `with_builtin_tools` → `with_tool` |
| `agent-service/src/service/agent_run.rs` | 修复 `build().await` 返回值缺少 `?` 的 bug |

---

## 4. 注意事项

- **白名单为空 ≠ 未设置白名单**：`with_whitelist(&[])` 会导致不返回任何工具，而 `ToolRegistry::new()` 不设白名单则全部包含
- `with_tool_registry()` 现在是**合并**而非**替换**，外部注入的工具仍受 `snapshot.tool_configs` 白名单约束
- 如果某个工具的 `tool_code`（即 `Tool::NAME`）不在 `t_agent_tool_registry` 中注册并绑定到智能体，该工具将不会被包含

---

## 5. 使用示例

### 场景 1：标准运行时构建（自动白名单）

```rust
// build() 内部自动从 snapshot.tool_configs 设置白名单
let runtime = AgentRuntime::builder(snapshot, model, publisher, services)
    .build()
    .await?;
```

### 场景 2：注入额外自定义工具

```rust
// 自定义工具也需要在 snapshot.tool_configs 中有对应的 tool_code
let extra = ToolRegistry::new().add_tool(CalculatorTool);
let runtime = AgentRuntime::builder(snapshot, model, publisher, services)
    .with_tool_registry(extra)
    .build()
    .await?;
```

### 场景 3：测试场景（无白名单，全部包含）

```rust
// 不设白名单，所有注册的工具都会被包含
let registry = ToolRegistry::new()
    .add_tool(ReadFileTool)
    .add_tool(ListDirTool);
let toolset = registry.build_toolset();
```

---

## 6. 兼容性说明

| 组件 | 变更类型 | 影响 |
|------|----------|------|
| `ToolRegistry::from_configs()` | **已移除** | 改用 `ToolRegistry::new().with_whitelist()` |
| `ToolRegistry::with_builtin_tools()` | **已移除** | 改用 `with_tool()` |
| `ToolRegistry::with_tools()` | **已移除** | 改用 `with_tool()` |
| `ToolRegistry::with_tool()` | **新增** | 统一的工具注册入口 |
| `ToolRegistry::with_whitelist()` | **新增** | 显式设置白名单 |
| `ToolRegistry::merge()` | **新增** | 合并外部注册表的候选工具 |
| `AgentRuntimeBuilder::with_tool_registry()` | **行为变更** | 从"替换"改为"合并" |

### 架构变更对比

**重构前：**

```text
ToolRegistry
├── builtin_tools: ToolSet          ← 始终包含，不受配置约束
├── available_tools: HashMap        ← 候选池
├── tool_configs: Vec<ToolConfig>   ← 配置列表（空=全部包含）
├── from_configs(&configs)          ← 构造
├── with_builtin_tools(tool)        ← 注册内置工具
├── with_tools(tool)                ← 注册候选工具
└── build_toolset()                 ← 内置 + 按配置筛选候选
```

**重构后：**

```text
ToolRegistry
├── candidate_tools: HashMap                ← 统一候选池
├── whitelist: Option<HashSet<String>>      ← None=全部, Some=严格过滤
├── new()                                   ← 构造（无白名单）
├── with_tool(tool)                         ← 注册工具
├── with_whitelist(&configs)                ← 设置白名单
├── merge(other)                            ← 合并外部候选工具
└── build_toolset()                         ← 按白名单筛选
```
