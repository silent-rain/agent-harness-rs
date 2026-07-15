# P0 阶段重构：Runner 抽象 + Callbacks 机制 + 存储层抽象

## 1. 功能说明

本次 P0 阶段重构对 `agent-base` crate 进行了三项核心架构改进：

1. **Runner trait 抽象**：从 `AgentRuntime` 中提取执行循环为独立的 `DefaultRunner`，定义 `Runner` / `ManagedRunner` / `SteerableRunner` 三层 trait
2. **Callbacks 机制**：添加生命周期钩子（before/after agent、model、tool），支持短路执行和结果替换
3. **存储层抽象**：定义 `StorageBackend` 统一 trait，将记忆读写、知识检索、运行记录和审计日志聚合为一个解耦接口

## 2. 新增内容

### 2.1 Runner 抽象层

| 文件 | 说明 |
|------|------|
| `runtime/runner/mod.rs` | Runner / ManagedRunner / SteerableRunner trait 定义 + 重导出 |
| `runtime/runner/default_runner.rs` | DefaultRunner 实现（从 AgentRuntime::run() 提取） |
| `runtime/runner/run_status.rs` | RunStatus 枚举（运行状态机） |

**三层 trait 设计**：

```text
Runner              ← 基础：run() 启动执行循环
  └─ ManagedRunner  ← 管理：pause() / resume() / stop()
       └─ SteerableRunner ← 操控：inject_message() 运行中注入消息
```

### 2.2 Callbacks 机制

| 文件 | 说明 |
|------|------|
| `runtime/runner/callback/mod.rs` | 回调参数/结果结构体 + 回调 trait 定义 |
| `runtime/runner/callback/registry.rs` | CallbackRegistry 注册表 + fire_xxx 方法 |

**6 个回调点**：

| 回调 | 触发时机 | 能力 |
|------|---------|------|
| `before_agent` | Agent 执行前 | 短路执行、替换初始输入 |
| `after_agent` | Agent 执行后 | 替换最终回答 |
| `before_model` | LLM 调用前 | 短路决策、替换请求 |
| `after_model` | LLM 调用后 | 替换决策结果 |
| `before_tool` | 工具执行前 | 短路执行、替换工具结果 |
| `after_tool` | 工具执行后 | 审计、告警 |

### 2.3 存储层抽象

| 文件 | 说明 |
|------|------|
| `runtime/storage/mod.rs` | 模块声明与重导出 |
| `runtime/storage/backend.rs` | StorageBackend trait + 请求结构体 + NoopStorageBackend |
| `runtime/storage/delegating.rs` | DelegatingStorageBackend（委托到现有 trait 实现） |

**StorageBackend 方法**：

| 方法 | 原 trait |
|------|---------|
| `write_memory()` | `AgentMemoryWriter::write_stream()` |
| `write_memory_batch()` | 批量写入（默认逐条委托） |
| `recall_memory()` | `AgentMemoryRetriever::recall()` |
| `query_knowledge()` | `AgentKnowledgeRetriever::query()` |
| `create_run_record()` | `AgentRunRecorder::create()` |
| `finish_run_record()` | `AgentRunRecorder::finish()` |
| `log_tool_audit()` | `AgentToolAuditLogger::log()` |

## 3. 修改内容

### 3.1 AgentRuntime

- 执行循环从 `AgentRuntime::run_inner()` 委托到 `DefaultRunner`
- 添加 `#[allow(dead_code)]` 注解保护未公开使用的私有方法
- 清理未使用导入（`futures::StreamExt`、`tracing` 等）

### 3.2 AgentRuntimeBuilder

- `RuntimeServices` 新增 `sub_agent_scheduler` 字段
- 新增 6 个回调注册 API：`on_before_agent()` / `on_after_agent()` / `on_before_model()` / `on_after_model()` / `on_before_tool()` / `on_after_tool()`
- 构建 `DefaultRunner` 时传递 `callbacks` 字段

### 3.3 DefaultRunner

- 新增 `callbacks: Arc<CallbackRegistry>` 字段
- 在 `run_inner()` 中插入回调调用点
- 在 `execute_step()` 中处理 `DelegateSubAgent` / `FanOutSubAgents` 决策分支
- 新增 `find_sub_agent_config()` 辅助方法

### 3.4 lib.rs

- 新增 `storage` 模块重导出
- 新增 `CallbackRegistry` / `StorageBackend` 等类型重导出

## 4. 注意事项

1. **向后兼容**：`StorageBackend` 目前仅作为抽象定义，`DefaultRunner` 仍通过 `RuntimeServices` 中的独立 trait 访问存储。后续 P1 阶段将把 `DefaultRunner` 切换为使用 `StorageBackend`
2. **回调错误处理**：`before_xxx` 回调的错误会中断执行循环（返回 `AgentError`），`after_xxx` 回调的错误仅记录警告
3. **已有测试失败**：`test_build_system_prompt_with_skills` 和 `test_build_system_prompt_with_rules_and_skills` 是已有问题，与本次修改无关

## 5. 使用示例

### 5.1 注册回调（Builder 风格）

```rust
use agent_base::runtime::AgentRuntimeBuilder;
use agent_base::runtime::runner::callback::{BeforeToolArgs, BeforeToolResult};

// 自定义 before_tool 回调：拒绝特定工具调用
struct DenyDangerousTool;

#[async_trait]
impl agent_base::runtime::runner::callback::BeforeToolCallback for DenyDangerousTool {
    async fn call(&self, args: BeforeToolArgs) -> AgentResult<BeforeToolResult> {
        if args.tool_code.starts_with("dangerous_") {
            Ok(BeforeToolResult {
                custom_result: Some(json!({"error": "工具被安全策略拒绝"})),
            })
        } else {
            Ok(BeforeToolResult { custom_result: None })
        }
    }
}

let runtime = AgentRuntimeBuilder::new(config, model)
    .on_before_tool(DenyDangerousTool)
    .build()
    .await?;
```

### 5.2 使用 StorageBackend（解耦存储实现）

```rust
use agent_base::runtime::storage::{StorageBackend, WriteMemoryRequest, RecallMemoryRequest};

async fn process_with_storage(backend: &dyn StorageBackend) -> AgentResult<()> {
    // 写入记忆
    backend.write_memory(WriteMemoryRequest {
        agent_id: 42,
        session_id: "sess_001".into(),
        run_id: "run_001".into(),
        entry: MemoryEntry {
            memory_type: MemoryType::Message,
            role: MessageRole::User,
            content: "用户问题".into(),
            content_json: None,
            importance_score: 0.8,
            tool_name: None,
            tool_call_id: None,
        },
    }).await?;

    // 召回记忆
    let result = backend.recall_memory(RecallMemoryRequest {
        agent_id: 42,
        session_id: "sess_001".into(),
        query: "相关上下文".into(),
        top_k: 5,
    }).await?;

    Ok(())
}
```

### 5.3 从 RuntimeServices 桥接（渐进迁移）

```rust
use agent_base::runtime::storage::DelegatingStorageBackend;

// 从现有的 RuntimeServices 创建委托后端
let services = RuntimeServices::noop();
let backend = DelegatingStorageBackend::from_runtime_services(&services);

// 统一使用 StorageBackend 接口
let result = backend.recall_memory(RecallMemoryRequest {
    agent_id: 1,
    session_id: "s1".into(),
    query: "test".into(),
    top_k: 5,
}).await?;
```

## 6. 兼容性说明

| 变更 | 影响 | 迁移方式 |
|------|------|---------|
| `DefaultRunner::new()` 新增 `callbacks` 参数 | `builder.rs` 中构建调用需更新 | 已在 builder 中完成适配 |
| `RuntimeServices` 新增 `sub_agent_scheduler` 字段 | 所有构造 `RuntimeServices` 的代码需更新 | `noop()` 方法已提供默认值 |
| `AgentDefinition` 新增 `max_delegation_depth` 字段 | 反序列化需兼容 | 使用 `#[serde(default)]` 提供默认值 3 |
| `StorageBackend` trait 新增 | 无破坏性影响 | 仅作为新抽象定义，不影响现有代码 |
| `CallbackRegistry` 新增 | 无破坏性影响 | 空注册表不触发任何回调 |
