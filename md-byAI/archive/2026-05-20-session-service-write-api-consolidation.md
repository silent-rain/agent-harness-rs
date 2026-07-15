---
title: SessionService 写入入口收敛重构
date: 2026-05-20
scope: agent-base / agent-platform
breaking: yes
---

# SessionService 写入入口收敛

## 1. 功能说明

把 `SessionService` 的 4 个写入相关 trait 方法（`append_event` / `record_memory` /
`create_run_record` / `finish_run_record`）统一收敛到 **唯一一个底层入口**
`append_record(SessionEventRecord)`，其它 4 个方法退化为基于 `append_record`
的 `default` 便捷方法。

## 2. 背景与动机

重构前，`SessionService` trait 暴露了 4 个写入方法，且 `DbSessionService`
中还存在 2 个内部 helper（`append_run_event` / `append_raw_event`）。它们的
本质都是同一种操作：**把一条事件落到 `t_agent_session_event` 表**，区别仅
在于 `event_type` 字符串和 `payload` 序列化形态。

```text
重构前：
  ┌───────────────────────┐
  │ append_event          │──┐
  │ record_memory         │──┤
  │ create_run_record     │──┼──► event_dao.append(NewSessionEvent {...})
  │ finish_run_record     │──┤
  │ append_run_event*     │──┤    (* DbSessionService 内部 helper)
  │ append_raw_event*     │──┘
  └───────────────────────┘

  6 个方法 / 5 段近乎雷同的 INSERT 准备代码
```

```text
重构后：
  ┌──────────────────────────────────────────────┐
  │ append_event       (default 便捷方法)       │──┐
  │ record_memory      (default 便捷方法)       │──┤
  │ create_run_record  (default 便捷方法)       │──┼──► append_record(SessionEventRecord) ──► event_dao.append(...)
  │ finish_run_record  (default 便捷方法)       │──┘                ▲
  └──────────────────────────────────────────────┘                  唯一需要实现覆盖的写入入口
```

主要收益：

1. **新写一个 `SessionService` 实现只需写 1 段 INSERT**——不再重复 5 段。
2. 事件分类知识（`event_type` 字符串、`run_id` 提取规则、记忆 payload
   包装）全部上提到 `agent-base`，下游存储实现不再持有这层概念，便于扩展。
3. 新增事件分类只需在 trait 上加便捷方法，不会引发实现层扩散。

## 3. 新增内容

### 3.1 新模块 `agent_base::session::event_record`

文件：`crates/core/agent-base/src/session/event_record.rs`

| 名称 | 类型 | 说明 |
|---|---|---|
| `SessionEventRecord<'a>` | struct | 事件写入统一契约（`session_id` / `run_id` / `event_type` / `payload_json`） |
| `MemoryEventPayload` | struct | 记忆事件 payload 包装（**从 `agent-platform::memory` 上提**） |
| `event_type_name(&AgentStreamEvent) -> &'static str` | fn | `AgentStreamEvent` → snake_case 事件类型字符串 |
| `extract_run_id(&AgentStreamEvent) -> Option<&str>` | fn | 提取事件中的 `run_id` |
| `memory_event_type(&MemoryType) -> String` | fn | `MemoryType` → `memory_<snake_case>` |
| `RUN_RECORD_CREATED` | const | `"run_record_created"` |
| `RUN_RECORD_FINISHED` | const | `"run_record_finished"` |

### 3.2 `SessionService` 新增方法

```rust
#[async_trait]
pub trait SessionService: Send + Sync + 'static {
    // 唯一需要实现覆盖的写入入口
    async fn append_record(&self, record: SessionEventRecord<'_>) -> AgentResult<()>;

    // 兜底反查（用于 finish_run_record），默认 noop
    async fn find_session_id_by_run_id(&self, _run_id: &str) -> AgentResult<Option<String>> {
        Ok(None)
    }

    // 以下 4 个方法变为基于 append_record 的 default 便捷方法
    async fn append_event(&self, session_id: &str, event: &AgentStreamEvent) -> AgentResult<()> { ... }
    async fn record_memory(&self, session_id: &str, run_id: &str, entry: &MemoryEntry) -> AgentResult<()> { ... }
    async fn create_run_record(&self, entry: RunRecordEntry) -> AgentResult<()> { ... }
    async fn finish_run_record(&self, entry: RunFinishEntry) -> AgentResult<()> { ... }
}
```

## 4. 修改内容

| 文件 | 改动 |
|---|---|
| `agent-base/src/session/event_record.rs` | **新增**：契约 + 工具函数 + 单元测试 |
| `agent-base/src/session/mod.rs` | 加入 `event_record` 子模块声明与重导出 |
| `agent-base/src/session/service.rs` | `SessionService` 写入接口收敛；`InMemorySessionService` 4 个写入实现合并为 1 个 `append_record` |
| `agent-platform/src/session/db_session_service.rs` | 删除 `append_event` / `record_memory` / `create_run_record` / `finish_run_record` / `append_run_event` / `append_raw_event` / `event_type_name` / `extract_run_id` / `memory_event_type`；仅保留 `append_record` + `find_session_id_by_run_id` |
| `agent-platform/src/memory/db_memory.rs` | 删除本地的 `MemoryEventPayload` 定义，改为从 `agent_base::session::MemoryEventPayload` 引入 |
| `agent-platform/src/memory/mod.rs` | 不再 re-export `MemoryEventPayload` |
| `agent-platform/src/lib.rs` | 不再 re-export `MemoryEventPayload` |

净行数变化：`db_session_service.rs` 由 463 行降至 ~290 行，`agent-platform` 侧
不再持有事件分类规则；新增 `event_record.rs` ~190 行，承载契约与对应单测。

## 5. 注意事项

### 5.1 行为不变

- 事件落库表（`t_agent_session_event`）不变；
- 事件类型字符串完全保留（`user_message` / `memory_*` / `run_record_created`
  / `run_record_finished` / ...）；
- 召回侧（`DbAgentMemoryRetriever::recall`）的 `event_type LIKE 'memory_%'`
  过滤逻辑不变；
- `finish_run_record` 通过 `find_session_id_by_run_id` 反查会话的语义不变。

### 5.2 Breaking Change

按用户要求，**不保留向后兼容**：

- `SessionService` trait 签名变化：自定义实现需要从"实现 4 个写入方法"切换为
  "实现 1 个 `append_record` + 按需覆盖 `find_session_id_by_run_id`"；
- `agent_platform::MemoryEventPayload` 已删除，使用方需改用
  `agent_base::session::MemoryEventPayload`。

项目内部已经全部迁移完毕，编译与单元测试（`agent-base`：256 通过；
`agent-platform`：2 通过）均零报错零警告。

### 5.3 `InMemorySessionService` 的简化

新实现仅在 payload 形如 `AgentStreamEvent` 时才把事件压入 `Session.events`，
其它类型事件（`memory_*` / `run_record_*`）静默忽略。这与内存场景的语义一致：
内存实现仅用于测试和无持久化场景，不需要承载记忆/运行记录的查询能力。

## 6. 使用示例

### 示例 A：自定义最小化的 `SessionService` 实现

```rust
use agent_base::{
    error::AgentResult,
    session::{
        AgentStreamEvent, Session, SessionEventRecord, SessionService, SessionStatus, StateMap,
    },
};
use async_trait::async_trait;
use tracing::info;

/// 仅打印日志的 SessionService（不做任何持久化）
pub struct LoggingSessionService;

#[async_trait]
impl SessionService for LoggingSessionService {
    // ── 写入入口：唯一需要实现的方法 ──
    async fn append_record(&self, record: SessionEventRecord<'_>) -> AgentResult<()> {
        info!(
            session_id = %record.session_id,
            run_id = ?record.run_id,
            event_type = %record.event_type,
            "append_record"
        );
        Ok(())
    }

    // ── CRUD：按需实现（此处省略具体细节） ──
    async fn create_session(&self, _: i64, _: i64, _: &str) -> AgentResult<Session> { todo!() }
    async fn get_session(&self, _: i64, _: i64, _: &str) -> AgentResult<Option<Session>> { Ok(None) }
    async fn list_sessions(&self, _: i64, _: i64) -> AgentResult<Vec<Session>> { Ok(vec![]) }
    async fn delete_session(&self, _: i64, _: i64, _: &str) -> AgentResult<()> { Ok(()) }
    async fn update_session_status(&self, _: &str, _: SessionStatus) -> AgentResult<()> { Ok(()) }
    async fn update_session_state(&self, _: &str, _: StateMap) -> AgentResult<()> { Ok(()) }
}

// 自动获得 4 个便捷方法（append_event / record_memory / create_run_record / finish_run_record）
// 全部会落到 append_record 中。
async fn demo(svc: &LoggingSessionService) -> AgentResult<()> {
    svc.append_event(
        "sess-1",
        &AgentStreamEvent::UserMessage {
            run_id: "run-1".into(),
            content: "hello".into(),
        },
    )
    .await
}
```

### 示例 B：主流程中调用便捷方法（与重构前完全一致）

```rust
use agent_base::{memory::MemoryEntry, RuntimeServices};

async fn announce_user_input(
    services: &RuntimeServices,
    session_id: &str,
    run_id: &str,
    user_input: &str,
) {
    // 三个便捷方法对应三种语义；底层均通过 append_record 落到同一张表
    let user_event = agent_base::session::AgentStreamEvent::UserMessage {
        run_id: run_id.to_string(),
        content: user_input.to_string(),
    };
    let _ = services.session.append_event(session_id, &user_event).await;

    let memory = MemoryEntry::user_message(user_input);
    let _ = services.session.record_memory(session_id, run_id, &memory).await;
}
```

### 示例 C：直接使用底层入口写入自定义事件类型

```rust
use agent_base::session::{SessionEventRecord, SessionService};

async fn write_custom_event(svc: &dyn SessionService) -> agent_base::error::AgentResult<()> {
    // 假设上层有自己的扩展事件类型，直接走 append_record 即可
    let payload = serde_json::json!({"foo": "bar"});
    svc.append_record(SessionEventRecord {
        session_id: "sess-1",
        run_id: Some("run-1"),
        event_type: "custom_audit",
        payload_json: payload.to_string(),
    })
    .await
}
```

## 7. 兼容性说明

| 项 | 影响 |
|---|---|
| `SessionService` trait | **breaking**：写入接口由 4 改为 1，自定义实现需迁移 |
| `RuntimeServices` 上的便捷方法（`record_run_started` / `record_run_finished` / `try_append_event` / `try_record_memory`） | **零影响**：底层调用对象不变 |
| `DbSessionService` 落库表（`t_agent_session_event`） | **零影响**：表结构、`event_type` 字符串、`payload_json` 内容完全保持不变 |
| `DbAgentMemoryRetriever::recall` | **零影响**：过滤条件与反序列化结构完全保持不变 |
| `agent_platform::MemoryEventPayload` | **breaking**：路径已变为 `agent_base::session::MemoryEventPayload`，外部使用方需更新引用 |

## 8. 涉及组件

- **agent-base**
  - `session::event_record`（新增模块）
  - `session::service::SessionService` / `InMemorySessionService`
- **agent-platform**
  - `session::DbSessionService`
  - `memory::DbAgentMemoryRetriever`
- **未受影响**：`runtime::services::RuntimeServices` 的对外便捷方法签名保持不变；
  `LlmAgent` 等主流程调用方零改动。

## 9. 验证

```bash
# 编译（workspace + 所有 targets）：成功
cargo check --workspace --all-targets

# Clippy（核心两个 crate）：零 warning
cargo clippy --package agent-base --package agent-platform --all-targets

# 单元测试
cargo test --package agent-base --lib       # 256 passed
cargo test --package agent-platform --lib   #   2 passed
```
