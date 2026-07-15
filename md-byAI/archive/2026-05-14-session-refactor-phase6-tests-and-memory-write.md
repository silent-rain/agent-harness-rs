# Session 重构 Phase 6：测试 mod 下沉 & memory 写入路径补全

> 提交时间：2026-05-14
> 影响范围：`crates/core/agent-base`、`crates/core/agent-platform`
> 性质：跟随 Phase 5 的两项收尾优化

## 1. 功能说明

承接 [Phase 5 文档](session-refactor-phase5-optimizations.md) 第 8 节"后续工作"，本轮兑现两项：

| #   | 任务                                                                                                                                                                                               | 类型     |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| A   | 把 [`event.rs`](../crates/core/agent-base/src/event.rs) 中的 `tests` mod 整体下沉到文件末尾，移除 `#[allow(clippy::items_after_test_module)]`                                                      | 代码组织 |
| B   | 在 `default_runner` 中通过 `SessionService::record_memory` 补齐 `memory_*` 事件写入路径，让 [`DbAgentMemoryRetriever`](../crates/core/agent-platform/src/memory/db_memory.rs) 真正能召回到历史记忆 | 功能补全 |

执行后效果：

- `event.rs` 的 `tests` mod 不再夹在 publisher 实现中间，符合 Rust 社区惯例；
- `default_runner` 在每轮对话结束时把用户输入与最终回答写为 `memory_message` 事件，`DbAgentMemoryRetriever::recall` 不再返回空集；
- `agent-base` / `agent-platform` / `entity` / `migration` 在 `cargo clippy --all-targets -D warnings` 下保持**零告警**。

## 2. 新增内容

### 2.1 `SessionService::record_memory` 方法（带默认 noop 实现）

[`crates/core/agent-base/src/runtime/session.rs`](../crates/core/agent-base/src/runtime/session.rs)

```rust
#[async_trait]
pub trait SessionService: Send + Sync + 'static {
    // ...existing methods...

    /// 追加一条记忆事件到会话事件流
    ///
    /// `event_type` 在持久化实现中为 `memory_<MemoryType-snake_case>`（如 `memory_message`），
    /// `payload_json` 为 `MemoryEventPayload { run_id, entry }` 的 JSON 序列化。
    ///
    /// 默认为 noop，仅 [`agent_platform::DbSessionService`] 需覆盖以落库。
    async fn record_memory(
        &self,
        _session_id: &str,
        _run_id: &str,
        _entry: &MemoryEntry,
    ) -> AgentResult<()> {
        Ok(())
    }
}
```

设计要点：

- **使用 default impl**：所有现有实现（`InMemorySessionService` / 测试 mod 的 `CountingSessionService`）零侵入获得 noop 行为；
- **只有 `DbSessionService` 实际覆盖**，落到 `t_agent_session_event` 中 `event_type = memory_*` 的行；
- **写入与召回结构对齐**：与 `crate::memory::MemoryEventPayload`（`{ run_id, entry: MemoryEntry }`）保持一致，`DbAgentMemoryRetriever::recall` 直接 `serde_json::from_str::<MemoryEventPayload>(...)` 即可还原。

### 2.2 `memory_event_type` 辅助函数

[`crates/core/agent-platform/src/session/db_session_service.rs`](../crates/core/agent-platform/src/session/db_session_service.rs)

```rust
/// 把 [`MemoryType`] 转换为 `event_type` 列存储值（形如 `memory_message`）。
fn memory_event_type(memory_type: &MemoryType) -> String {
    let snake = serde_json::to_value(memory_type)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "unknown".to_string());
    format!("memory_{snake}")
}
```

借助 `MemoryType` 自身的 `#[serde(rename_all = "snake_case")]` 派生，无需写一遍 6 个变体的 match。命名规则：

| `MemoryType` 变体 | `event_type` 值           |
| ----------------- | ------------------------- |
| `Message`         | `memory_message`          |
| `ToolResult`      | `memory_tool_result`      |
| `RagResult`       | `memory_rag_result`       |
| `SubAgentResult`  | `memory_sub_agent_result` |
| `PlanStep`        | `memory_plan_step`        |
| `Reflection`      | `memory_reflection`       |

均匹配 `DbAgentMemoryRetriever` 的 `event_type LIKE 'memory_%'`。

## 3. 修改内容

### 3.1 `event.rs` 测试 mod 下沉

| 位置                                                               | 变更                                          |
| ------------------------------------------------------------------ | --------------------------------------------- |
| `event.rs:258-357` 原 `mod tests` 段                               | **删除**                                      |
| `event.rs:260` 原 `#[allow(clippy::items_after_test_module)]` 注解 | **删除**                                      |
| `event.rs` 文件末尾                                                | 追加 `mod tests` 整段（5 个原有测试用例不变） |

新文件结构：

```text
1. 类型定义 / impl
2. AgentEventPublisher trait
3. NoopEventPublisher / ChannelEventPublisher / ForwardingEventPublisher 实现
4. #[cfg(test)] mod tests   ← 整段位于文件末尾
```

### 3.2 `DbSessionService::record_memory` 实现

[`crates/core/agent-platform/src/session/db_session_service.rs`](../crates/core/agent-platform/src/session/db_session_service.rs)

```rust
async fn record_memory(
    &self,
    session_id: &str,
    run_id: &str,
    entry: &MemoryEntry,
) -> AgentResult<()> {
    let session_model = self.require_session(session_id).await?;

    let payload = MemoryEventPayload {
        run_id: run_id.to_string(),
        entry: entry.clone(),
    };
    let payload_json = serde_json::to_string(&payload)
        .map_err(|e| Error::Internal(format!("序列化记忆事件失败: {e}")))?;

    let event_type = memory_event_type(&entry.memory_type);

    self.append_raw_event(
        session_id,
        session_model.agent_id,
        session_model.user_id,
        Some(run_id.to_string()),
        &event_type,
        payload_json,
    )
    .await
}
```

复用了 Phase 4 抽出的 `require_session` / `append_raw_event` 辅助方法，无重复逻辑。

### 3.3 `default_runner` 调用 `record_memory`

[`crates/core/agent-base/src/runtime/runner/default_runner.rs`](../crates/core/agent-base/src/runtime/runner/default_runner.rs)

在 `core_run` 的两个关键节点同步追加 `memory_message` 事件：

**A. 用户输入入站后**（紧跟 `append_event(UserMessage)` 之后）：

```rust
let user_memory = MemoryEntry {
    memory_type: MemoryType::Message,
    role: MessageRole::User,
    content: user_input.to_string(),
    content_json: None,
    importance_score: 0.5,
    tool_name: None,
    tool_call_id: None,
};
if let Err(e) = self
    .services
    .session
    .record_memory(session_id, &run_id, &user_memory)
    .await
{
    warn!(run_id = %run_id, error = %e, "记录用户记忆失败");
}
```

**B. 最终回答产出后**（紧跟 `append_event(AnswerDelta)` 之后）：

```rust
let assistant_memory = MemoryEntry {
    memory_type: MemoryType::Message,
    role: MessageRole::Assistant,
    content: answer.clone(),
    content_json: None,
    importance_score: 0.7,
    tool_name: None,
    tool_call_id: None,
};
if let Err(e) = self
    .services
    .session
    .record_memory(session_id, run_id, &assistant_memory)
    .await
{
    warn!(run_id = %run_id, error = %e, "记录助手记忆失败");
}
```

**重要性评分约定**：

- 用户输入：`0.5`（中等）—— 反映"这是上下文锚点"
- 助手回答：`0.7`（较高）—— 反映"这是事实输出，召回价值更高"
- 后续若引入工具结果记忆，建议 `0.4`，反思总结建议 `0.8`

### 3.4 接口兼容性影响

| 实现                                                        | 是否需要改动                   |
| ----------------------------------------------------------- | ------------------------------ |
| `agent-base::InMemorySessionService`                        | ❌ 不需要（继承 default noop） |
| `agent-base::runtime::agent::tests::CountingSessionService` | ❌ 不需要（继承 default noop） |
| `agent-platform::DbSessionService`                          | ✅ 已实现                      |

## 4. 注意事项

### 4.1 写入失败不阻断主流程

`record_memory` 的两处调用都用 `if let Err(e) = ...` 包装，写入失败仅打印 `warn!` 日志，不向上传播。这与 `append_event` 的失败策略一致——**记忆系统是辅助能力，不应让其故障打断对话主流程**。

### 4.2 召回与写入双侧 payload 契约

写入方与召回方共用 [`crate::memory::MemoryEventPayload`](../crates/core/agent-platform/src/memory/db_memory.rs)：

```rust
pub struct MemoryEventPayload {
    pub run_id: String,
    pub entry: MemoryEntry,
}
```

**任何修改这个结构的改动都属于 breaking change**，需要同时考虑历史数据向后兼容。当前实现使用 `filter_map(... ok())`，未知格式的旧行会被静默跳过，不会引发解码错误；这给了未来演化空间。

### 4.3 `event_type` 命名约定

写入侧（`memory_event_type`）和召回侧（`event_type LIKE 'memory_%'`）必须保持一致。如未来要新增 `MemoryType` 变体（例如 `UserPreference`），只需让其 `#[serde(rename_all = "snake_case")]` 自然派生即可，无需调整任一侧的代码。

### 4.4 两类事件并存的语义

某条对话发生时，事件流中会同时出现：

| 时序 | event_type       | payload                                                  |
| ---- | ---------------- | -------------------------------------------------------- |
| T0   | `user_message`   | `AgentStreamEvent::UserMessage` JSON                     |
| T0+ε | `memory_message` | `MemoryEventPayload { entry: { role: user, ... } }`      |
| ...  | ...              | ...                                                      |
| Tn   | `answer_delta`   | `AgentStreamEvent::AnswerDelta` JSON                     |
| Tn+ε | `memory_message` | `MemoryEventPayload { entry: { role: assistant, ... } }` |

这是**有意的双写**：`user_message` / `answer_delta` 用于事件回放（按原始 schema 还原），`memory_*` 专为召回优化（独立 payload schema、独立筛选条件、独立 importance 字段）。两者互不干涉，调用方按需查询。

### 4.5 `event.rs` 文件结构变更影响

虽然 `tests` mod 物理位置发生变化，但 `crate::event::*` 公开 API 完全不变。下沉只影响代码可读性与 clippy lint，不影响任何外部消费者。

## 5. 使用示例

### 示例 A：写入与召回的端到端流程

```rust,no_run
use std::sync::Arc;
use sea_orm::DatabaseConnection;
use agent_base::{
    memory::{AgentMemoryRetriever, MemoryEntry, MemoryType},
    message::MessageRole,
    runtime::session::SessionService,
};
use agent_platform::{DbAgentMemoryRetriever, DbSessionService};

# async fn demo(db: Arc<DatabaseConnection>) -> anyhow::Result<()> {
let session_service = DbSessionService::new(db.clone());
let retriever = DbAgentMemoryRetriever::new(db);

// 1) 创建会话
let _ = session_service.create_session(123, 456, "sess-001").await?;

// 2) 写入一条用户记忆
let entry = MemoryEntry {
    memory_type: MemoryType::Message,
    role: MessageRole::User,
    content: "我喜欢喝咖啡".to_string(),
    content_json: None,
    importance_score: 0.5,
    tool_name: None,
    tool_call_id: None,
};
session_service
    .record_memory("sess-001", "run-001", &entry)
    .await?;

// 3) 召回（按 id 倒序，top 5）
let recalled = retriever
    .recall(123, "sess-001", "用户偏好", 5)
    .await?;
assert_eq!(recalled.entries.len(), 1);
assert_eq!(recalled.entries[0].content, "我喜欢喝咖啡");
# Ok(()) }
```

### 示例 B：自定义 SessionService 实现（继承默认 record_memory noop）

```rust,no_run
use async_trait::async_trait;
use agent_base::{
    error::AgentResult,
    event::{AgentStreamEvent, StateMap},
    runtime::session::{
        RunFinishEntry, RunRecordEntry, Session, SessionService, SessionStatus,
        ToolAuditEntry,
    },
};

struct LoggingSessionService;

#[async_trait]
impl SessionService for LoggingSessionService {
    async fn create_session(&self, agent_id: i64, user_id: i32, session_id: &str) -> AgentResult<Session> {
        Ok(Session::new(session_id, agent_id, user_id))
    }
    async fn get_session(&self, _: i64, _: i32, _: &str) -> AgentResult<Option<Session>> { Ok(None) }
    async fn list_sessions(&self, _: i64, _: i32) -> AgentResult<Vec<Session>> { Ok(vec![]) }
    async fn delete_session(&self, _: i64, _: i32, _: &str) -> AgentResult<()> { Ok(()) }
    async fn append_event(&self, sid: &str, _e: &AgentStreamEvent) -> AgentResult<()> {
        println!("session {sid}: event appended");
        Ok(())
    }
    async fn update_session_status(&self, _: &str, _: SessionStatus) -> AgentResult<()> { Ok(()) }
    async fn update_session_state(&self, _: &str, _: StateMap) -> AgentResult<()> { Ok(()) }
    async fn create_run_record(&self, _: RunRecordEntry) -> AgentResult<()> { Ok(()) }
    async fn finish_run_record(&self, _: RunFinishEntry) -> AgentResult<()> { Ok(()) }
    async fn log_tool_audit(&self, _: ToolAuditEntry) -> AgentResult<()> { Ok(()) }
    // 注意：未实现 record_memory，自动继承 trait 的 default noop 实现
}
```

### 示例 C：手动写入不同类型的记忆事件

```rust,no_run
use std::sync::Arc;
use sea_orm::DatabaseConnection;
use agent_base::{
    memory::{MemoryEntry, MemoryType},
    message::MessageRole,
    runtime::session::SessionService,
};
use agent_platform::DbSessionService;

# async fn demo(db: Arc<DatabaseConnection>) -> anyhow::Result<()> {
let svc = DbSessionService::new(db);

// 工具结果记忆（event_type = memory_tool_result）
let tool_memory = MemoryEntry {
    memory_type: MemoryType::ToolResult,
    role: MessageRole::Tool,
    content: "查询结果：今日深圳 26°C 多云".to_string(),
    content_json: Some(r#"{"city":"shenzhen","temp":26,"weather":"cloudy"}"#.to_string()),
    importance_score: 0.4,
    tool_name: Some("get_weather".to_string()),
    tool_call_id: Some("call_abc123".to_string()),
};
svc.record_memory("sess-001", "run-001", &tool_memory).await?;

// 反思记忆（event_type = memory_reflection）
let reflection = MemoryEntry {
    memory_type: MemoryType::Reflection,
    role: MessageRole::Assistant,
    content: "用户连续询问咖啡，应主动推荐相关内容".to_string(),
    content_json: None,
    importance_score: 0.8,
    tool_name: None,
    tool_call_id: None,
};
svc.record_memory("sess-001", "run-001", &reflection).await?;
# Ok(()) }
```

## 6. 兼容性说明

| 维度                              | 影响                                                                                      |
| --------------------------------- | ----------------------------------------------------------------------------------------- |
| `SessionService` trait            | **非 breaking**：`record_memory` 带 default noop 实现                                     |
| `MemoryEntry` / `MemoryType` 结构 | 零变化                                                                                    |
| `MemoryEventPayload` 结构         | 零变化（仍为 `{ run_id, entry }`）                                                        |
| `event.rs` 公开 API               | 零变化（仅 `tests` mod 物理位置变更）                                                     |
| `DbAgentMemoryRetriever` 行为     | **行为变化**：在已部署 `default_runner` 后，`recall` 不再返回空集（按 id 倒序的最近记忆） |
| 数据库 schema                     | 零变化                                                                                    |

## 7. 验证

| 检查项                            | 命令                                                                                                         | 结果                                                  |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------- |
| workspace 编译                    | `cargo check --workspace`                                                                                    | ✅ 0 error / 0 warning                                |
| 4 核心包严格 clippy（含测试代码） | `cargo clippy -p agent-base -p agent-platform -p entity -p migration --all-targets --no-deps -- -D warnings` | ✅ 全绿（`#[allow(items_after_test_module)]` 已移除） |
| `agent-base` 单元测试             | `cargo test -p agent-base --lib`                                                                             | ✅ 230 passed / 0 failed                              |
| `agent-platform` 单元测试         | `cargo test -p agent-platform --lib`                                                                         | ✅ 2 passed / 0 failed                                |
| `event.rs` 5 个迁移到末尾的测试   | `cargo test -p agent-base --lib event::`                                                                     | ✅ 5 passed / 0 failed                                |

## 8. 后续工作

1. **`DbAgentMemoryRetriever` 升级语义召回**：当前按 `id` 倒序取 top-k，待接入 rag-core 后改为向量召回 + 重要性加权；`MemoryEntry` 已预留 `importance_score` / `content_json` 字段，无需改 schema。
2. **覆盖更多记忆类型的写入路径**：当前仅写入了 `MemoryType::Message`（用户输入 + 助手回答）；后续可在 `ToolCallFinished` / `RagFinished` / `SubAgentFinished` 等事件中追加 `ToolResult` / `RagResult` / `SubAgentResult` 类型记忆。
3. **`AgentRef` 强类型封装**：把 `(agent_id: i64, user_id: i32)` 二元组封装为带语义的值对象，进一步压缩 `SessionService` 接口的参数列。
4. **重要性评分动态化**：当前用硬编码常量（0.5 / 0.7），可考虑根据消息长度、含工具调用数量、用户标记等因素动态计算。
