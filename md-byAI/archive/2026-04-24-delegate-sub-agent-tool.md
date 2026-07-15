# DelegateSubAgentTool — 子智能体委托工具实现

## 1. 功能说明

将子智能体包装为 `rig::Tool`（工具名 `delegate_sub_agent`），注入到父智能体的 `ToolSet` 中，
LLM 通过 function calling 选择并调用子智能体。每次调用会构建一个完整的子 `AgentRuntime`
并执行委派任务。

### 核心设计

- **单工具多子智能体**：一个 `DelegateSubAgentTool` 实例服务所有已注册的子智能体
- **LLM 选择**：通过 `sub_agent_name` 参数指定目标子智能体
- **完整运行时**：每次调用创建独立的子 `AgentRuntime`（无状态共享）
- **能力声明**：子智能体的 `tools`、`skills` 等字段作为白名单传递给子运行时

## 2. 新增内容

### 新增文件

| 文件 | 说明 |
|------|------|
| `agent-runtime/src/sub_agent/tools/mod.rs` | 工具子模块声明与重导出 |
| `agent-runtime/src/sub_agent/tools/delegate_sub_agent.rs` | `DelegateSubAgentTool` 实现 |

### 新增类型

| 类型 | 说明 |
|------|------|
| `DelegateSubAgentTool` | 子智能体委托工具，实现 `rig::tool::Tool` trait |
| `DelegateSubAgentArgs` | 工具输入参数（`sub_agent_name` + `task`） |
| `DelegateSubAgentOutput` | 工具输出（`sub_agent_name` + `success` + `answer` / `error`） |

## 3. 修改内容

| 文件 | 改动 |
|------|------|
| `agent-runtime/src/sub_agent/mod.rs` | 注册 `tools` 子模块，重导出 `DelegateSubAgentTool` |
| `agent-runtime/src/lib.rs` | 重导出 `DelegateSubAgentTool` |
| `agent-runtime/src/runtime/builder.rs` | `load_sub_agents()` 改为接受 `&mut ToolSet` 参数，注册 `DelegateSubAgentTool` |

### Builder 变更详情

`load_sub_agents()` 方法签名从：

```rust
async fn load_sub_agents(&mut self) -> Result<(), Error>
```

变更为：

```rust
async fn load_sub_agents(&mut self, toolset: &mut ToolSet) -> Result<(), Error>
```

内部新增逻辑：

```rust
let delegate_tool = DelegateSubAgentTool::new(
    &self.sub_agent_configs,
    Arc::clone(&self.model),
    Arc::clone(&self.publisher),
    max_depth,
);
toolset.add_tool(delegate_tool);
```

## 4. 注意事项

1. **子智能体事件转发**：当前子智能体使用 `NoopEventPublisher`，不转发事件。
   `publisher` 字段已预留，后续可通过 `ForwardingEventPublisher` 实现事件透传。

2. **规则和 MCP 解析**：子智能体的 `rules` 和 `mcp_tools` 字段目前仅作为名称列表存储，
   尚未解析为实际的 `RuleConfig` / `McpServiceConfig`。后续需要实现名称到配置的解析。

3. **模型继承**：子智能体默认继承父智能体的 model。若 `SubAgentConfig.model` 有值，
   会写入 snapshot 的 `model_config.model_code`，但实际的模型切换需要上层支持。

4. **递归深度**：`max_depth` 从父智能体的 `max_delegation_depth` 获取，
   子智能体的 `max_delegation_depth` 会自动减 1，防止无限递归。

5. **与旧 Scheduler 的关系**：`DelegateSubAgentTool` 是子智能体调用的**新路径**，
   直接在 `agent-runtime` 内部完成运行时构建和执行。旧的 `SubAgentScheduler` trait
   和 `PlatformSubAgentScheduler` 仍然存在，用于 `PlannerDecision::DelegateSubAgent`
   决策分支。两条路径暂时共存，后续可逐步废弃旧路径。

## 5. 使用示例

### 示例 1：通过 Builder 便捷 API 注入子智能体

```rust
use agent_runtime::{AgentRuntime, RuntimeServices, NoopEventPublisher};
use agent_runtime::types::sub_agent::{SubAgentConfig, SubAgentSource};

let sub_agents = vec![SubAgentConfig {
    name: "code-reviewer".to_string(),
    description: "Reviews code for quality".to_string(),
    tools: vec!["read_file".to_string()],
    system_prompt: "You are a code review expert.".to_string(),
    source: SubAgentSource::Injected,
    ..Default::default()
}];

let runtime = AgentRuntime::builder(snapshot, model, publisher, services)
    .with_sub_agent_configs(sub_agents)
    .build()
    .await?;
// LLM 现在可以调用 delegate_sub_agent(sub_agent_name="code-reviewer", task="...")
```

### 示例 2：通过本地扫描加载子智能体

```rust
let runtime = AgentRuntime::builder(snapshot, model, publisher, services)
    .with_local_scan(true)  // 扫描 .codebuddy/agents/ 目录
    .build()
    .await?;
// 所有 .md 文件中的子智能体自动注册为 delegate_sub_agent 工具
```

### 示例 3：通过底层 API 注入预配置的 SubAgentRegistry

```rust
use agent_runtime::sub_agent::SubAgentRegistry;

let mut registry = SubAgentRegistry::new(".codebuddy/agents")
    .with_local_scan(true);
registry.scan().await?;
registry.add_sub_agents(db_sub_agents);

let runtime = AgentRuntime::builder(snapshot, model, publisher, services)
    .with_sub_agent_registry(registry)
    .build()
    .await?;
```

## 6. 兼容性说明

- **向后兼容**：旧的 `PlannerDecision::DelegateSubAgent` / `FanOutSubAgents` 决策分支
  仍然通过 `SubAgentScheduler` trait 工作，不受影响。
- **新路径**：`DelegateSubAgentTool` 是新增的调用路径，LLM 通过 function calling 触发。
  两条路径暂时共存。
- **测试**：117 个单元测试全部通过，新增 4 个 `DelegateSubAgentTool` 相关测试。

## 7. 调用链路对比

### 旧路径（Planner 决策分支）

```
LLM → PlannerDecision::DelegateSubAgent
  → AgentRuntime.run_loop()
    → SubAgentScheduler.delegate()
      → PlatformSubAgentScheduler.build_sub_agent_runtime()
        → AgentRuntime::builder().build()
          → runtime.run()
```

### 新路径（Tool function calling）

```
LLM → function_call("delegate_sub_agent", {sub_agent_name, task})
  → DelegateSubAgentTool.call()
    → build_child_runtime()
      → AgentRuntime::builder()
          .with_tool_whitelist(config.tools)
          .with_skill_whitelist(config.skills)
          .build()
    → runtime.run()
```

新路径更简洁，不需要 Scheduler 抽象层，直接在 `agent-runtime` 内部完成。
