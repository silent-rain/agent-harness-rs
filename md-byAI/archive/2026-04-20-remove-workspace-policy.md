# 移除 WorkspaceRuntimePolicy / WorkspaceGuard / t_agent_workspace

## 变更日期

2026-04-20

## 变更概述

移除了 `WorkspaceRuntimePolicy`、`WorkspaceGuard`、`policy` 模块以及对应的数据库表 `t_agent_workspace`，降低系统复杂度。

## 变更原因

`WorkspaceGuard` 的路径校验机制存在根本性缺陷：

1. **对 `run_command` 工具形同虚设** — `sh -c` 可执行任意命令，完全绕过路径白名单/黑名单
2. **对结构化工具价值有限** — `read_file`/`list_dir` 等工具的路径校验在没有沙箱的情况下意义不大
3. **增加了不必要的复杂度** — 每个工具都需要注入 `Arc<WorkspaceGuard>`，`ToolRegistry` 需要接收 `WorkspaceRuntimePolicy`
4. **真正的安全边界应该是沙箱**（Docker/Firecracker），而非应用层路径校验

## 删除的文件

- `crates/core/agent-base/src/policy.rs` — `WorkspaceGuard` 实现
- `crates/core/entity/src/agent/workspace.rs` — `t_agent_workspace` 表 Entity 定义
- `crates/core/migration/src/agent/workspace.rs` — `t_agent_workspace` 表迁移脚本

## 修改的文件

### agent-base

| 文件 | 变更 |
|------|------|
| `domain.rs` | 删除 `WorkspaceRuntimePolicy` 结构体及其 impl；删除 `AgentDefinitionSnapshot.workspace_policy` 字段 |
| `lib.rs` | 移除 `policy` 模块声明、`WorkspaceGuard` 导出、`WorkspaceRuntimePolicy` 重导出 |
| `tools/mod.rs` | `ToolRegistry` 从有状态结构体简化为无字段 unit struct |
| `tools/read_file.rs` | 移除 `guard` 字段和 `check_path` 调用 |
| `tools/list_dir.rs` | 移除 `guard` 字段和 `check_path` 调用 |
| `tools/search_files.rs` | 移除 `guard` 字段和 `check_path` 调用 |
| `tools/run_command.rs` | 移除 `guard` 字段和 `check_terminal`/`check_path` 调用 |
| `runtime/agent_runtime.rs` | `ToolRegistry` 构造不再传入 `workspace_policy` |
| `mcp.rs` | 修复预先存在的重复 `Default` impl 和 `new()` 循环调用问题 |
| `examples/basic_agent.rs` | 移除 `workspace_policy` 字段 |
| `examples/streaming_agent.rs` | 移除 `workspace_policy` 字段 |
| `examples/tool_agent.rs` | 移除 `workspace_policy` 字段 |

### agent-platform

| 文件 | 变更 |
|------|------|
| `provider/db_provider.rs` | 删除 `load_workspace` 方法、`build_workspace_policy` 函数、`workspace` entity 导入 |
| `provider/local_provider.rs` | 移除 `WorkspaceRuntimePolicy` 导入和 `workspace_policy` 字段 |

### entity

| 文件 | 变更 |
|------|------|
| `agent/mod.rs` | 移除 `workspace` 模块声明、`AgentWorkspace` re-export、文档注释 |
| `agent/agent_config.rs` | 移除 workspace 外键关联的文档注释 |

### migration

| 文件 | 变更 |
|------|------|
| `lib.rs` | 移除 `agent::workspace::Migration` 注册 |
| `agent/mod.rs` | 移除 `workspace` 模块声明和依赖关系文档 |
| `agent/agent_config.rs` | 移除 workspace 表关联的文档注释 |

## 简化效果

### Before

```rust
// 工具需要注入 guard
let registry = ToolRegistry::new(snapshot.workspace_policy.clone());
// 每个工具都持有 Arc<WorkspaceGuard>
pub struct ReadFileTool {
    guard: Arc<WorkspaceGuard>,
}
// 快照需要 workspace_policy 字段
pub struct AgentDefinitionSnapshot {
    pub workspace_policy: WorkspaceRuntimePolicy,
    // ...
}
```

### After

```rust
// 工具注册表无状态
let registry = ToolRegistry;
// 工具是简单的 unit struct
pub struct ReadFileTool;
// 快照更精简
pub struct AgentDefinitionSnapshot {
    // 无 workspace_policy
    // ...
}
```

## 兼容性说明

- **不向后兼容** — `WorkspaceRuntimePolicy`、`WorkspaceGuard`、`policy` 模块均已删除
- `AgentDefinitionSnapshot` 不再包含 `workspace_policy` 字段
- `ToolRegistry::new()` 不再需要参数，改为 unit struct
- 所有工具构造函数不再需要 `guard` 参数
- `t_agent_workspace` 表的 Entity 和 Migration 已删除（需要重新建库或手动 drop table）

## 后续规划

真正的安全边界应通过沙箱实现：

- **短期**：通过 system prompt 引导 LLM 行为边界
- **中期**：引入 Docker 容器沙箱执行命令
- **长期**：完全沙箱化（Firecracker / gVisor）
