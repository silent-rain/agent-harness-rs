# session-refactor-phase1

## 1. 功能说明

本期（Phase 1）为 **Session 级事件存储重构** 系列的第一阶段，仅交付 **数据层（Migration + Entity）**。

围绕「**以 session 为核心**」的架构目标，我们引入 5 张新表，逐步替代散落在 `t_agent_memory_stream`、`t_agent_tool_audit_log`、`t_agent_run_record` 等表中的会话级数据职责，并新增三层 KV 状态存储（App / User / Session）。

> 本期 **不修改任何业务代码**（agent-base / agent-platform 内的 trait、service、runtime 均保持不变），仅为后续 Phase 2 ~ Phase 6 提供存储底座。旧表暂时保留，按原有路径继续工作。

---

## 2. 新增内容

### 2.1 Migration 文件

| 文件                                               | 表名                    | 用途                      |
| -------------------------------------------------- | ----------------------- | ------------------------- |
| `crates/core/migration/src/agent/session.rs`       | `t_agent_session`       | 会话主表                  |
| `crates/core/migration/src/agent/session_event.rs` | `t_agent_session_event` | 会话事件流（append-only） |
| `crates/core/migration/src/agent/session_state.rs` | `t_agent_session_state` | 会话级 KV 状态            |
| `crates/core/migration/src/agent/user_state.rs`    | `t_agent_user_state`    | 用户级 KV 状态            |
| `crates/core/migration/src/agent/app_state.rs`     | `t_agent_app_state`     | 应用（智能体）级 KV 状态  |

### 2.2 Entity 文件

| 文件                                            | Entity         |
| ----------------------------------------------- | -------------- |
| `crates/core/entity/src/agent/session.rs`       | `Session`      |
| `crates/core/entity/src/agent/session_event.rs` | `SessionEvent` |
| `crates/core/entity/src/agent/session_state.rs` | `SessionState` |
| `crates/core/entity/src/agent/user_state.rs`    | `UserState`    |
| `crates/core/entity/src/agent/app_state.rs`     | `AppState`     |

### 2.3 关键字段约定

所有新表统一遵循以下约定：

| 决策点            | 取值                                                 | 理由                                                   |
| ----------------- | ---------------------------------------------------- | ------------------------------------------------------ |
| `agent_id` 类型   | `i64` (`big_integer`)                                | 与 `t_agent_config.id` 一致                            |
| `user_id` 类型    | `i32` (`integer`)                                    | 与现有 `memory_stream.user_id` 一致                    |
| `session_id` 类型 | `String(64)`                                         | UUID v4 文本，跨服务可读                               |
| 软删除            | `deleted: bool`                                      | 与 `t_agent_config` 对齐，避免 `deleted_at` 唯一性陷阱 |
| `value_json`      | `Text NULL`                                          | 兼容 SQLite/MySQL/PostgreSQL，由应用层负责 JSON 编解码 |
| 时间戳            | `created_at` / `updated_at`（事件流仅 `created_at`） | 与现有表对齐                                           |

### 2.4 表结构概览

#### t_agent_session（会话主表）

| 列                      | 类型               | 说明                           |
| ----------------------- | ------------------ | ------------------------------ |
| id                      | i64 PK             | 自增主键                       |
| session_id              | varchar(64) UNIQUE | 业务主键（UUID）               |
| agent_id                | i64                | 所属智能体                     |
| agent_name              | varchar(128)       | 冗余名称（免 join）            |
| user_id                 | i32                | 所属用户                       |
| title                   | varchar(255) NULL  | 会话标题                       |
| status                  | i16                | 0=archived, 1=active, 2=closed |
| last_event_at           | datetime NULL      | 最近事件时间（活跃度排序）     |
| deleted                 | bool               | 软删除                         |
| created_at / updated_at | datetime           | 时间戳                         |

索引：

- UNIQUE `(session_id)`
- IDX `(agent_id, user_id)`
- IDX `(user_id)` / `(status)` / `(last_event_at)`

#### t_agent_session_event（事件流，append-only）

| 列                 | 类型               | 说明                                                                                                 |
| ------------------ | ------------------ | ---------------------------------------------------------------------------------------------------- |
| id                 | i64 PK             | 自增主键                                                                                             |
| event_id           | varchar(64) UNIQUE | 全局唯一事件ID（UUID）                                                                               |
| session_id         | varchar(64)        | 所属会话                                                                                             |
| agent_id / user_id | i64 / i32          | 冗余字段，免 join                                                                                    |
| run_id             | varchar(64) NULL   | 关联运行ID                                                                                           |
| parent_event_id    | varchar(64) NULL   | 父事件（工具调用→工具结果回溯）                                                                      |
| seq                | i64                | **会话内单调递增**，与 `session_id` 联合唯一                                                         |
| event_type         | varchar(48)        | `user_message` / `assistant_message` / `tool_call` / `tool_result` / `planning` / `error` / `system` |
| author             | varchar(64) NULL   | `user` / `assistant` / `tool` / `system` / sub-agent 名                                              |
| payload_json       | TEXT               | 完整事件载荷（对应 `AgentStreamEvent` 序列化）                                                       |
| branch             | varchar(64) NULL   | 预留：分支编辑/回放                                                                                  |
| created_at         | datetime           | 事件发生时间                                                                                         |

索引：

- UNIQUE `(event_id)`
- UNIQUE `(session_id, seq)` ← **保证强排序**
- IDX `(session_id, created_at)` / `(run_id)` / `(event_type)`

#### 三层 KV 状态表

统一字段：`agent_id`(+`user_id` / `session_id`) + `key` + `value_json` + 时间戳。

| 表                      | 唯一约束                   | 作用域                    |
| ----------------------- | -------------------------- | ------------------------- |
| `t_agent_app_state`     | `(agent_id, key)`          | 智能体全局（跨用户/会话） |
| `t_agent_user_state`    | `(agent_id, user_id, key)` | 用户维度（跨会话）        |
| `t_agent_session_state` | `(session_id, key)`        | 会话维度（最高优先级）    |

> **三层叠加规则（与 trpc-agent-go 一致）**：读取时按 `App < User < Session` 顺序合并，相同 key 由 Session 覆盖；写入由 `apply_delta` 按 scope 行级 upsert，将在 Phase 2 实现。

---

## 3. 修改内容

仅修改注册入口，**未改动任何运行时代码**：

1. `crates/core/entity/src/agent/mod.rs`
   - 新增 5 个 `pub mod` 与 `pub use Entity as XxxEntity`
   - 在头部 doc 中标注旧表（`memory_stream` / `memory_distilled` / `tool_audit_log` / `run_record`）的 **Phase 6 下线计划**

2. `crates/core/migration/src/agent/mod.rs`
   - 同步新增 5 个迁移模块声明

3. `crates/core/migration/src/lib.rs`
   - 在 `Migrator::migrations()` 中注册 5 个新 Migration（顺序：`session` → `session_event` → `session_state` → `user_state` → `app_state`）

---

## 4. 注意事项

### 4.1 旧表保留期

为保证平滑迁移，本期 **保留所有旧表与对应 Entity / Service**：

| 旧表                       | 处置阶段     | 替代方案                                                          |
| -------------------------- | ------------ | ----------------------------------------------------------------- |
| `t_agent_memory_stream`    | Phase 6 下线 | 由 `t_agent_session_event` 接管                                   |
| `t_agent_memory_distilled` | Phase 6 下线 | 由后续 `t_agent_session_summary` 接管（待规划）                   |
| `t_agent_tool_audit_log`   | Phase 6 下线 | `t_agent_session_event` 中 `event_type='tool_call'/'tool_result'` |
| `t_agent_run_record`       | **建议保留** | 作为运行级别的汇总指标视图（与事件流互补）                        |

### 4.2 物理外键

延续项目惯例，**新增表均不创建物理外键**，仅通过应用层维护一致性，便于分库/归档/异步清理。

### 4.3 索引设计取舍

- `t_agent_session_event` 是写入热点，故意 **不为 `payload_json` 建索引**，所有查询需先按 `(session_id, seq)` 或 `(session_id, created_at)` 收敛后再做 JSON 解析
- `seq` 由应用层（Phase 2 `SessionEventDao`）通过 `MAX(seq)+1` 或乐观锁生成，依赖 `(session_id, seq)` 唯一约束兜底

### 4.4 命名约定

按你的反馈，**`app_name` 已替换为 `agent_id`**（i64），并在需要冗余可读性时附加 `agent_name`。这一约定与本项目「以 agent 为单位部署」的现状一致。

---

## 5. 使用示例

> ⚠️ 以下示例为 Phase 2 起的目标用法，仅用于说明数据模型，**Phase 1 暂未提供 DAO/Service**。

### 示例 1：场景 A —— 创建会话并写入首个用户消息事件

```rust,no_run
use std::sync::Arc;
use chrono::Local;
use sea_orm::{DatabaseConnection, EntityTrait, Set};
use entity::agent::{
    session::ActiveModel as SessionActive,
    session_event::ActiveModel as EventActive,
    session::Entity as SessionEntity,
    session_event::Entity as EventEntity,
};

async fn create_session_with_first_event(
    db: Arc<DatabaseConnection>,
    agent_id: i64,
    agent_name: String,
    user_id: i32,
    user_text: String,
) -> Result<String, sea_orm::DbErr> {
    let session_id = uuid::Uuid::new_v4().to_string();

    // 1) 创建会话
    let session = SessionActive {
        session_id: Set(session_id.clone()),
        agent_id: Set(agent_id),
        agent_name: Set(agent_name),
        user_id: Set(user_id),
        title: Set(None),
        status: Set(1),
        last_event_at: Set(Some(Local::now().naive_local())),
        deleted: Set(false),
        ..Default::default()
    };
    SessionEntity::insert(session).exec(db.as_ref()).await?;

    // 2) 写入首个用户消息事件（seq=1）
    let event = EventActive {
        event_id: Set(uuid::Uuid::new_v4().to_string()),
        session_id: Set(session_id.clone()),
        agent_id: Set(agent_id),
        user_id: Set(user_id),
        run_id: Set(None),
        parent_event_id: Set(None),
        seq: Set(1),
        event_type: Set("user_message".into()),
        author: Set(Some("user".into())),
        payload_json: Set(serde_json::json!({ "text": user_text }).to_string()),
        branch: Set(None),
        ..Default::default()
    };
    EventEntity::insert(event).exec(db.as_ref()).await?;

    Ok(session_id)
}
```

**使用场景**：用户首次发起对话，会话与首条事件需在同一逻辑动作内落库。Phase 2 的 `DbSessionService::create_session()` 会将这两步封装为事务。

### 示例 2：场景 B —— 按时间序读取整个会话事件流

```rust,no_run
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use entity::agent::session_event::{Column, Entity, Model};

async fn load_events(
    db: &sea_orm::DatabaseConnection,
    session_id: &str,
) -> Result<Vec<Model>, sea_orm::DbErr> {
    Entity::find()
        .filter(Column::SessionId.eq(session_id))
        .order_by_asc(Column::Seq) // 严格按 seq 排序（与 created_at 一致）
        .all(db)
        .await
}
```

**使用场景**：Agent 启动时回放历史 / 调试 UI 展示完整会话日志。

### 示例 3：场景 C —— Session 级状态 upsert（KV apply_delta 雏形）

```rust,no_run
use sea_orm::{
    sea_query::OnConflict, ColumnTrait, EntityTrait, Set,
};
use entity::agent::session_state::{ActiveModel, Column, Entity};

async fn upsert_session_state(
    db: &sea_orm::DatabaseConnection,
    session_id: &str,
    agent_id: i64,
    user_id: i32,
    key: &str,
    value_json: Option<String>,
) -> Result<(), sea_orm::DbErr> {
    let model = ActiveModel {
        session_id: Set(session_id.to_string()),
        agent_id: Set(agent_id),
        user_id: Set(user_id),
        key: Set(key.to_string()),
        value_json: Set(value_json),
        ..Default::default()
    };

    Entity::insert(model)
        .on_conflict(
            OnConflict::columns([Column::SessionId, Column::Key])
                .update_columns([Column::ValueJson, Column::UpdatedAt])
                .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}
```

**使用场景**：Agent 在工具调用后将进度/中间产物写回 Session 状态，下次 LLM 推理时合并到 prompt。Phase 2 将统一封装为 `StateDao::apply_delta(scope, ...)`。

---

## 6. 兼容性说明

### 6.1 已验证

- `cargo check -p entity -p migration` ✅ 通过
- 旧表与旧 Entity 全部保留，不破坏 `agent-base`、`agent-platform` 现有调用方
- 新表均使用 `if_not_exists`，重复执行迁移安全

### 6.2 升级路径（执行顺序）

```text
Phase 1（本次）：建表 ─────────────────────────┐
                                               ▼
Phase 2：DAO 层（agent-platform/src/session/）
        ├─ SessionDao
        ├─ SessionEventDao  (含 seq 生成)
        └─ StateDao         (App/User/Session 三层统一)
                                               ▼
Phase 3：DbSessionService (实现 agent_base::SessionService)
                                               ▼
Phase 4：agent-base 调整
        ├─ runtime/session.rs 拆分
        ├─ event.rs 增加 EventEnvelope
        └─ StateScope 行级 apply_delta
                                               ▼
Phase 5：RuntimeServices 注入 + builder.rs 接线
                                               ▼
Phase 6：旧表/旧 Service 标记 deprecated → 数据迁移脚本 → 物理下线
```

### 6.3 不兼容点

无。本期为纯增量改动。

---

## 7. 涉及组件

| 模块                                       | crate            | 角色            | 本期是否改动            |
| ------------------------------------------ | ---------------- | --------------- | ----------------------- |
| `entity::agent`                            | `entity`         | SeaORM 实体定义 | ✅ 新增 5 个模块        |
| `migration::agent`                         | `migration`      | 数据库迁移定义  | ✅ 新增 5 个模块 + 注册 |
| `agent_base::runtime::session`             | `agent-base`     | 会话 trait      | ⏸ Phase 4 改动          |
| `agent_platform::memory` / `observability` | `agent-platform` | DB 实现         | ⏸ Phase 6 改动          |

---

## 8. 后续行动项

- [ ] Phase 2：在 `agent-platform` 新增 `session/` 子模块，提供 `SessionDao` / `SessionEventDao` / `StateDao`
- [ ] Phase 2：补充 `t_agent_session_summary`（蒸馏摘要）的设计与建表（合并 `memory_distilled` 职责）
- [ ] Phase 4 前：在 `agent_base::event` 中统一 `EventEnvelope { event_id, parent, seq, event_type, payload }` 抽象，保证落库与流式分发用同一结构
- [ ] Phase 6：编写从 `memory_stream` + `tool_audit_log` 迁移到 `session_event` 的一次性脚本
