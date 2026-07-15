# Session 重构 Phase 3 & 4：废弃表清理与 DbSessionService 落地

> 提交时间：2026-05-14
> 影响范围：`crates/core/entity`、`crates/core/migration`、`crates/core/agent-platform`
> 涉及表：`t_agent_memory_stream`、`t_agent_memory_distilled`、`t_agent_run_record`、`t_agent_tool_audit_log`（**全部下线**）；`t_agent_session_event`（吸纳事件 / 工具审计 / 记忆流）

## 1. 功能说明

本次改造一并完成两个阶段：

- **Phase 3 — 废弃表清理**：根据 Phase 1 的并表决策，**彻底下线** 4 张被 `t_agent_session_*` 体系吸纳的旧表，包括 `entity` / `migration` 全套定义与 `Migrator` 注册。

- **Phase 4 — `DbSessionService` 落地**：在 Phase 2 已沉淀的 `SessionDao` / `SessionEventDao` / `StateDao` 之上组装出 `DbSessionService`，作为 `agent_base::runtime::session::SessionService` trait 的数据库实现。`RuntimeServices.session` 自此可直接注入持久化版本，无须再依赖 `InMemorySessionService`。

合并执行的根本原因：删除旧表会让原本写入旧表的 `DbAgentMemoryWriter` / `DbAgentMemoryRetriever` 编译失败；同时落地 `DbSessionService` 才能为这两个适配器提供新的事件写入通道，避免出现编译断点。

## 2. 新增内容

| 路径                                                                                                                              | 说明                                                    |
| --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| [`crates/core/agent-platform/src/session/db_session_service.rs`](../crates/core/agent-platform/src/session/db_session_service.rs) | `DbSessionService` 实现，落地 `SessionService` 全部接口 |

`DbSessionService` 接口覆盖：

- 会话 CRUD：`create_session` / `get_session` / `list_sessions` / `delete_session`（软删除）
- 事件追加：`append_event` —— 自动序列化 `AgentStreamEvent` 为 JSON，按 snake_case 提取 `event_type`，并写入 `t_agent_session_event`
- 状态管理：`update_session_status`、`update_session_state`（通过 `StateDao::apply_delta` 行级 upsert，**不**整体覆盖）
- 可观测性：`create_run_record` / `finish_run_record` / `log_tool_audit` —— 统一收敛到 `t_agent_session_event`，事件类型 `run_record_created` / `run_record_finished` / `tool_audit`

`finish_run_record` 不携带 `session_id`，通过 `run_id` 反查最早的 `run_record_created` 事件得到归属会话。

## 3. 修改内容

### 3.1 删除项（Phase 3）

| 文件                                                  | 操作     |
| ----------------------------------------------------- | -------- |
| `crates/core/entity/src/agent/memory_distilled.rs`    | **删除** |
| `crates/core/entity/src/agent/memory_stream.rs`       | **删除** |
| `crates/core/entity/src/agent/run_record.rs`          | **删除** |
| `crates/core/entity/src/agent/tool_audit_log.rs`      | **删除** |
| `crates/core/migration/src/agent/memory_distilled.rs` | **删除** |
| `crates/core/migration/src/agent/memory_stream.rs`    | **删除** |
| `crates/core/migration/src/agent/run_record.rs`       | **删除** |
| `crates/core/migration/src/agent/tool_audit_log.rs`   | **删除** |

### 3.2 注册同步

| 文件                                     | 操作                                               |
| ---------------------------------------- | -------------------------------------------------- |
| `crates/core/entity/src/agent/mod.rs`    | 移除 4 张表的 `pub mod` 与 `pub use Entity as ...` |
| `crates/core/migration/src/agent/mod.rs` | 移除 4 张表的 `pub mod` 与文档注释                 |
| `crates/core/migration/src/lib.rs`       | 从 `Migrator::migrations()` 中移除对应 4 行        |

### 3.3 适配层重写（Phase 3 收尾）

| 文件                                                 | 操作                                                                                                                                                                  |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/core/agent-platform/src/memory/db_memory.rs` | **重写**：`DbAgentMemoryWriter` / `DbAgentMemoryRetriever` 不再访问 `t_agent_memory_stream`，改为读写 `t_agent_session_event`（以 `event_type LIKE 'memory_%'` 过滤） |

`DbAgentMemoryWriter` 写入策略：

```text
event_type   = "memory_" + memory_type.to_snake_case()   // 如 memory_message
payload_json = JSON({ run_id, entry: MemoryEntry })
```

`DbAgentMemoryRetriever` 召回策略：

```text
SELECT * FROM t_agent_session_event
WHERE agent_id = ? AND session_id = ? AND deleted = false
  AND event_type LIKE 'memory_%'
ORDER BY id DESC
LIMIT top_k
```

### 3.4 模块导出

| 文件                                            | 操作                                                                                  |
| ----------------------------------------------- | ------------------------------------------------------------------------------------- |
| `crates/core/agent-platform/src/session/mod.rs` | 增加 `pub mod db_session_service;` 与 `pub use db_session_service::DbSessionService;` |
| `crates/core/agent-platform/src/lib.rs`         | `pub use session::DbSessionService;` 加入顶层 facade                                  |

## 4. 注意事项

### 4.1 数据库迁移

- 本次改动**不附带** `.down()` 数据迁移脚本——这是一次**清理性下线**而非降级。生产侧若已有这 4 张表的历史数据，**需要在升级前自行导出/归档**（按 Phase 1 的并表逻辑可平移到 `t_agent_session_event` / `t_agent_session_summary`）。
- 全新部署或测试环境直接重建即可：`Migrator::up()` 已不再创建这 4 张表。

### 4.2 字段映射约定（DbSessionService）

`SessionService` trait 接口签名为 `app_name: &str` / `user_id: &str`（来自 trpc-agent-go 的三元组语义），`DbSessionService` 在边界做以下约定：

- `app_name` → `agent_id` (`i64`，按字面量解析失败则返回 `Error::Internal`)
- `user_id` → `i32`（同上）

该约定在 `parse_agent_id` / `parse_user_id` 私有辅助函数中实现。后续可考虑为 `SessionService` 增设强类型方法以彻底消除字符串解析。

### 4.3 状态写入语义

- `update_session_state` 采用**行级 upsert**（`StateDao::apply_delta`），仅修改 delta 中提到的 key；未在 delta 中出现的旧 key **不会被清空**。
- 删除 key 的语义当前由 `value: Value::Null` 暗示——会以 `value_json = NULL` 写入，但**不会**触发软删除。如需"删除某个 key"，应使用 `StateDao::delete` 直接 API。

### 4.4 兼容性

- `agent-base::AgentMemoryWriter` / `AgentMemoryRetriever` trait **未改动**，签名与语义保持稳定。
- `RuntimeServices` 结构体与构造函数 **未改动**，仅 `session` 字段的可选注入实现增加了 `DbSessionService` 这一选项；既有 `InMemorySessionService` 路径继续可用。
- 所有面向上层（service-hub / agent-service）的 API 保持兼容，无需改动调用方。

### 4.5 弃用提醒

- `AgentMemoryWriter` 在 agent-base 内部已**没有任何调用方**（仅 trait + Noop + db 适配器存在）。其历史职责已被 `SessionService::append_event` 接管，建议在下个清理窗口与 `MemoryDomain.writer` 字段一并移除。
- `AgentMemoryRetriever` 仍被 `default_runner.rs` / `agent.rs` 使用（`recall(...)` 调用），**保留**。

## 5. 使用示例

### 示例 A：在 `RuntimeServices` 中注入 DbSessionService

```rust,no_run
use std::sync::Arc;
use sea_orm::DatabaseConnection;
use agent_base::RuntimeServices;
use agent_platform::DbSessionService;

# fn build_db() -> Arc<DatabaseConnection> { unimplemented!() }
let db = build_db();
let services = RuntimeServices {
    session: Arc::new(DbSessionService::new(db.clone())),
    ..RuntimeServices::noop()
};
// 之后构造 Agent 时把 services 注入即可，事件 / 状态 / 运行记录全部走数据库
```

### 示例 B：直接通过 SessionService 接口创建并查询会话

```rust,no_run
use std::sync::Arc;
use sea_orm::DatabaseConnection;
use agent_base::runtime::session::SessionService;
use agent_platform::DbSessionService;

# async fn demo(db: Arc<DatabaseConnection>) -> anyhow::Result<()> {
let service = DbSessionService::new(db);

// 注意：app_name 解析为 agent_id (i64)，user_id 解析为 i32
let session = service.create_session("12345", "1001", "sess-uuid-aaa").await?;
assert_eq!(session.id, "sess-uuid-aaa");

let loaded = service
    .get_session("12345", "1001", "sess-uuid-aaa")
    .await?;
assert!(loaded.is_some());
# Ok(()) }
```

### 示例 C：手工追加事件并加载会话视图

```rust,no_run
use std::sync::Arc;
use std::collections::HashMap;
use sea_orm::DatabaseConnection;
use agent_base::{
    event::AgentStreamEvent,
    runtime::session::SessionService,
};
use agent_platform::DbSessionService;

# async fn demo(db: Arc<DatabaseConnection>) -> anyhow::Result<()> {
let service = DbSessionService::new(db);

// 创建会话
service.create_session("12345", "1001", "sess-1").await?;

// 追加用户消息事件
service
    .append_event(
        "sess-1",
        &AgentStreamEvent::UserMessage {
            run_id: "run-x".into(),
            content: "你好".into(),
        },
    )
    .await?;

// 增量更新会话状态
let mut delta = HashMap::new();
delta.insert("preferred_lang".to_string(), serde_json::json!("zh"));
service.update_session_state("sess-1", delta).await?;

// 加载完整会话（含全部事件 + 全量 session 级状态）
let session = service
    .get_session("12345", "1001", "sess-1")
    .await?
    .unwrap();
assert_eq!(session.events.len(), 1);
assert_eq!(session.state.get("preferred_lang").unwrap(), "zh");
# Ok(()) }
```

## 6. 兼容性说明

| 维度                      | 评估                                                                                                            |
| ------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `agent-base` 公共 API     | **零破坏**，trait 签名与 DTO 完全保留                                                                           |
| `RuntimeServices`         | **零破坏**，仅扩充可选实现                                                                                      |
| `agent-platform` 公共 API | 删除已废弃的 `DbAgentRunRecorder` / `DbAgentToolAuditLogger`（上一轮存量修复时已下线）；新增 `DbSessionService` |
| service-hub 调用方        | **零破坏**，无需改动                                                                                            |
| 数据库 schema             | `t_agent_memory_*` / `t_agent_run_record` / `t_agent_tool_audit_log` 自此**不再创建**，旧环境需自行迁移历史数据 |

## 7. 相关组件 / 模块

```text
agent-platform
├── memory/
│   └── db_memory.rs            ← Phase 3 重写：基于 t_agent_session_event
├── session/
│   ├── session_dao.rs          ← Phase 2 已落地
│   ├── session_event_dao.rs    ← Phase 2 已落地
│   ├── state_dao.rs            ← Phase 2 已落地
│   ├── enums/
│   │   └── session_status.rs   ← Phase 2 已落地
│   └── db_session_service.rs   ← Phase 4 新增：SessionService 数据库实现
└── lib.rs                      ← 重导出 DbSessionService
```

## 8. 后续工作

1. **强类型 SessionService 接口**：把 `app_name: &str` 替换为 `AgentRef`（持有 `agent_id: i64`），消除字符串解析。
2. **`AgentMemoryWriter` 完整下线**：把内部所有 `write_stream` 调用迁移到 `SessionService::append_event(AgentStreamEvent::...)`，删除 trait + Noop + DbAgentMemoryWriter。
3. **`DbAgentMemoryRetriever` 升级为向量召回**：当前仅按 `id` 倒序，未来接入 rag-core 实现语义召回。
4. **agent-base clippy 历史告警清理**：`needless_borrow` (`default_runner.rs:1005`)、`module_inception` (`skill/installer/mod.rs`)。
