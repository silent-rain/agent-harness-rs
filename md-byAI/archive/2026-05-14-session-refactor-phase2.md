# session-refactor-phase2

## Phase 2 — 会话持久化 DAO 层落地

> **范围**：仅在 [`agent-platform`](../crates/core/agent-platform) 新增 `session/` 子模块，提供 5 张核心表的 DAO 封装；**不动** `agent-base` 的 `SessionService` trait，留待 Phase 4 统一改造。

## 一、功能说明

为 Phase 1 已建好的 5 张表提供一层薄而干净的数据访问 API：

| 表名                    | DAO                         | 主要职责                                                               |
| ----------------------- | --------------------------- | ---------------------------------------------------------------------- |
| `t_agent_session`       | `SessionDao`                | 会话主表 CRUD、按 (agent, user) 列表查询、软删除、`last_event_at` 刷新 |
| `t_agent_session_event` | `SessionEventDao`           | append-only 事件流写入、按会话顺序读取（增量游标）                     |
| `t_agent_session_state` | `StateDao` (Session 作用域) | 三层 KV 之一                                                           |
| `t_agent_user_state`    | `StateDao` (User 作用域)    | 三层 KV 之一（**跨 agent**）                                           |
| `t_agent_global_state`  | `StateDao` (Agent 作用域)   | 三层 KV 之一（agent 私有）                                             |

通过 `StateScopeKey` 把三张状态表的 get / list / upsert / apply_delta / delete 收敛到同一个 DAO，避免上层重复样板代码。

---

## 二、新增内容

### 2.1 模块结构

```text
crates/core/agent-platform/src/session/
├── mod.rs                 // 子模块声明 + 重导出（无业务逻辑）
├── enums/
│   ├── mod.rs             // 仅 pub mod + pub use
│   └── session_status.rs  // SessionStatus 枚举（i16，消除魔数）
├── session_dao.rs         // SessionDao
├── session_event_dao.rs   // SessionEventDao
└── state_dao.rs           // StateDao + StateScopeKey + StateEntry
```

### 2.2 关键类型

**`SessionStatus`**（`#[repr(i16)]`）：

| 值  | 变体       | 含义                       |
| --- | ---------- | -------------------------- |
| 0   | `Archived` | 只读，不再接收新事件       |
| 1   | `Active`   | 默认，正常追加事件         |
| 2   | `Closed`   | 用户主动关闭，仍可只读访问 |

**`StateScopeKey<'a>`** —— 三层状态统一寻址：

```rust
pub enum StateScopeKey<'a> {
    Session { session_id: &'a str, agent_id: i64, user_id: i32 },
    User    { user_id: i32 },                              // 跨 agent
    Agent   { agent_id: i64, user_id: i32 },               // user_id=0 表示全局共享
}
```

**`NewSession` / `SessionUpdate`** —— 显式 DTO，避免 `Option<Option<T>>` 在外层泛滥；`SessionUpdate` 字段全为 `Option<…>`，None 表示「不修改」。

**`NewSessionEvent`** —— 事件追加 DTO；`run_id` 设为 `Option<String>`，允许系统级事件不携带 run。

### 2.3 公共 API

```rust
// SessionDao
SessionDao::create(NewSession) -> Model
SessionDao::get(session_id) -> Option<Model>
SessionDao::list_by_agent_user(agent_id, user_id, page, page_size) -> Vec<Model>
SessionDao::count_by_agent_user(agent_id, user_id) -> u64
SessionDao::update(session_id, SessionUpdate) -> Model
SessionDao::touch_last_event_at(session_id)
SessionDao::soft_delete(session_id)

// SessionEventDao
SessionEventDao::append(NewSessionEvent) -> Model
SessionEventDao::list_by_session(session_id, after_id, limit) -> Vec<Model>

// StateDao
StateDao::get(scope, key) -> Option<StateEntry>
StateDao::list(scope) -> Vec<StateEntry>
StateDao::upsert(scope, key, value_json, expires_at)
StateDao::apply_delta(scope, Vec<(key, Option<value>)>)
StateDao::delete(scope, key)   // 软删除
```

### 2.4 lib.rs 重导出

```rust
pub use session::{
    NewSession, NewSessionEvent, SessionDao, SessionEventDao,
    SessionStatus, SessionUpdate,
    StateDao, StateEntry, StateScopeKey,
};
```

---

## 三、修改内容

| 文件                                        | 改动                                      |
| ------------------------------------------- | ----------------------------------------- |
| `crates/core/agent-platform/src/lib.rs`     | 新增 `pub mod session;` + 公共 API 重导出 |
| `crates/core/agent-platform/src/session/**` | **新增**（详见上节）                      |

> 未触碰 `agent-base`、`entity`、`migration`，未触碰其他既有 DAO，零破坏面。

---

## 四、设计要点

### 4.1 `SessionEventDao::append` 联动 `last_event_at`

事件 append 后调用 `SessionDao::touch_last_event_at`：

- **失败容忍**：刷新 `last_event_at` 失败仅 `tracing::warn!`，不影响事件本身落库（事件落库是关键路径，列表排序的精确性次之）。
- 这避免引入跨表事务；后续若需要严格一致，可在 Phase 4 上层 service 中包一层事务。

### 4.2 `StateDao::apply_delta` 不开启跨 key 事务

- 逐 key upsert，依赖各表自身唯一索引保证单行原子性。
- 行级锁的并发友好度优于表级事务，满足三层状态高频写入场景。
- 如调用方需要跨 key 强一致，应在外层用 `db.begin()` 自行包事务再调用。

### 4.3 软删除统一约定

- 所有读路径默认过滤 `deleted = false`。
- 所有「删除」操作仅置位 `deleted = true`，物理清理交给后台任务（Phase 5+）。
- `upsert` 命中已软删除记录时会自动复位 `deleted = false`，等价于「恢复并写新值」。

### 4.4 不解析 `payload_json`

`SessionEventDao` 把 `payload_json` 视作不透明字符串：

- DAO 层不感知 `AgentStreamEvent` 结构，避免循环依赖（agent-base ↔ entity ↔ agent-platform）。
- 解析职责将在 Phase 4 的 `DbSessionService` 中承担。

### 4.5 `StateScopeKey::Agent` 的语义

| `user_id` | 含义                                                                               |
| --------- | ---------------------------------------------------------------------------------- |
| `0`       | 真正的 agent 全局共享（运行计数器、热缓存、配额等）                                |
| `≠ 0`     | 「agent 视角下关于该用户的长期记忆」——与 `User` 作用域形成区分（后者**跨 agent**） |

---

## 五、注意事项

1. **agent-platform 现存编译错误**：工作区中 `agent-base` 已删除 `observability` / `publisher` / `storage` 等模块，但 `agent-platform/src/observability.rs` 仍引用 `agent_base::observability::*`，导致 `cargo check -p agent-platform` 失败。
   - 这是 **Phase 2 之前就存在** 的故障（属于另一条 agent-base 重构链条），与本次交付无关。
   - 验证 Phase 2 自身正确性的方法：临时注释 `lib.rs` 中 `pub mod observability;`，`cargo check` 与 `cargo clippy -- -D warnings` **均通过**。
   - 该存量问题留到 Phase 3 / 4 一并修复（届时 observability 会迁移为基于 `t_agent_session_event` 的事件衍生）。

2. **`run_id` 为 `Option<String>`**：与 entity 层对齐。系统级事件（session 创建、定时摘要、外部触发）不归属任何 run。

3. **DAO 不走 nject**：当前 agent-platform 没有引入 nject 体系，DAO 只通过 `Arc<DatabaseConnection>` 显式注入；待 Phase 4 引入 `DbSessionService` 时再视情况切换。

4. **未添加 DAO 单测**：根据用户决策跳过。质量保障转由 Phase 4 的 service 层集成测试覆盖。

---

## 六、使用示例

### 示例 1：创建会话并追加首条事件

```rust
use std::sync::Arc;
use sea_orm::DatabaseConnection;
use agent_platform::{
    NewSession, NewSessionEvent, SessionDao, SessionEventDao,
};

async fn bootstrap_session(db: Arc<DatabaseConnection>) -> anyhow::Result<()> {
    let session_dao = SessionDao::new(db.clone());
    let event_dao = SessionEventDao::new(db);

    let session = session_dao
        .create(NewSession {
            session_id: uuid::Uuid::new_v4().to_string(),
            agent_id: 1001,
            user_id: 42,
            title: Some("天气问答".into()),
            expires_at: None,
        })
        .await?;

    event_dao
        .append(NewSessionEvent {
            session_id: session.session_id.clone(),
            agent_id: 1001,
            user_id: 42,
            run_id: Some(uuid::Uuid::new_v4().to_string()),
            event_type: "user_message".into(),
            payload_json: r#"{"role":"user","content":"今天北京天气"}"#.into(),
            expires_at: None,
        })
        .await?;

    Ok(())
}
```

### 示例 2：三层状态读写

```rust
use std::sync::Arc;
use sea_orm::DatabaseConnection;
use agent_platform::{StateDao, StateScopeKey};

async fn write_states(db: Arc<DatabaseConnection>) -> anyhow::Result<()> {
    let state = StateDao::new(db);

    // 会话级：当前轮的 scratchpad
    state
        .upsert(
            StateScopeKey::Session { session_id: "sess-1", agent_id: 1001, user_id: 42 },
            "scratchpad",
            Some(r#"{"step":3}"#.into()),
            None,
        )
        .await?;

    // 用户级（跨 agent）：用户语言偏好
    state
        .upsert(
            StateScopeKey::User { user_id: 42 },
            "lang",
            Some(r#""zh-CN""#.into()),
            None,
        )
        .await?;

    // Agent 全局（user_id = 0）：运行总数
    state
        .upsert(
            StateScopeKey::Agent { agent_id: 1001, user_id: 0 },
            "run_count",
            Some("12345".into()),
            None,
        )
        .await?;

    Ok(())
}
```

### 示例 3：增量拉取事件流

```rust
use std::sync::Arc;
use sea_orm::DatabaseConnection;
use agent_platform::SessionEventDao;

async fn poll_events(db: Arc<DatabaseConnection>, session_id: &str) -> anyhow::Result<()> {
    let event_dao = SessionEventDao::new(db);
    let mut cursor: Option<i64> = None;

    loop {
        let batch = event_dao.list_by_session(session_id, cursor, Some(100)).await?;
        if batch.is_empty() {
            break;
        }
        cursor = batch.last().map(|m| m.id);
        // 业务消费 batch ...
    }
    Ok(())
}
```

---

## 七、兼容性说明

- **零破坏**：未删除/修改任何既有公共 API；旧的 `DbAgentMemoryWriter` / `DbAgentMemoryRetriever` / `DbAgentRunRecorder` 等保持不变。
- **新增 API 仅暴露 DAO 层**：未涉及 `agent-base` 的 trait，调用方无感知。
- **数据库依赖**：依赖 Phase 1 已注册的 5 张迁移；老环境需先跑 `cargo run -p migration` 升级。
- **后续 Phase 影响**：
  - **Phase 3**：处理 agent-platform 中残留的 observability 引用（迁移到事件流）。
  - **Phase 4**：基于本期 DAO 实现 `DbSessionService`，替换 `InMemorySessionService`。

---

## 八、验证记录

```bash
# 验证 Phase 2 自身（临时注释 observability 重导出后）
cargo check  -p agent-platform           # ✅ 通过，零警告
cargo clippy -p agent-platform --no-deps -- -D warnings   # ✅ 通过，零警告
```

存量 observability 引用问题不在 Phase 2 修复范围。
