# Phase 7 — 治理与生产能力（Run Record + Tool Audit Log + FanOut 并发限制）

## 功能说明

本次实现了 Phase 7 的核心治理能力和 Phase 6 遗留的并发安全保障：

1. **Run Record 运行记录持久化**：每次 `AgentRuntime::run()` 的完整执行记录写入 `t_agent_run_record` 表
2. **工具调用审计日志**：每次工具调用的入参/出参/耗时写入 `t_agent_tool_audit_log` 表
3. **FanOut 并发数限制**：`delegate_fan_out` 通过信号量限制最大并发数（默认 8），防止资源耗尽

---

## 新增内容

### 数据库迁移

| 文件 | 表名 | 说明 |
|------|------|------|
| `migration/src/agent/run_record.rs` | `t_agent_run_record` | 运行记录表 |
| `migration/src/agent/tool_audit_log.rs` | `t_agent_tool_audit_log` | 工具调用审计日志表 |

#### `t_agent_run_record` 表结构

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 记录ID |
| `run_id` | VARCHAR(64) UNIQUE | 运行ID（UUID） |
| `agent_id` | BIGINT | 所属智能体ID |
| `user_id` | BIGINT | 所属用户ID |
| `session_id` | VARCHAR(64) | 会话ID |
| `parent_run_id` | VARCHAR(64) NULL | 父运行ID（子智能体委派时设置） |
| `status` | VARCHAR(32) | 运行状态: running/success/failed/cancelled |
| `user_input` | TEXT | 用户输入文本 |
| `final_answer` | TEXT NULL | 最终回答（成功时） |
| `error_message` | TEXT NULL | 错误信息（失败时） |
| `total_steps` | INT | 总推理步数 |
| `finish_reason` | VARCHAR(32) NULL | 结束原因 |
| `duration_ms` | BIGINT NULL | 运行耗时（毫秒） |
| `snapshot_json` | TEXT NULL | 配置快照（JSON） |
| `created_at` | DATETIME | 创建时间（运行开始） |
| `updated_at` | DATETIME | 更新时间（运行结束） |

#### `t_agent_tool_audit_log` 表结构

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 日志ID |
| `run_id` | VARCHAR(64) | 所属运行ID |
| `agent_id` | BIGINT | 所属智能体ID |
| `user_id` | BIGINT | 所属用户ID |
| `session_id` | VARCHAR(64) | 会话ID |
| `step` | INT | 所在推理步骤 |
| `tool_code` | VARCHAR(128) | 工具代码/名称 |
| `call_id` | VARCHAR(64) | 工具调用ID |
| `arguments_json` | TEXT NULL | 调用入参（JSON） |
| `result_json` | TEXT NULL | 调用出参（JSON） |
| `result_summary` | TEXT NULL | 结果摘要 |
| `success` | BOOLEAN | 是否成功 |
| `error_message` | TEXT NULL | 错误信息（失败时） |
| `duration_ms` | BIGINT NULL | 调用耗时（毫秒） |
| `created_at` | DATETIME | 创建时间 |

### SeaORM 实体

| 文件 | 说明 |
|------|------|
| `entity/src/agent/run_record.rs` | `t_agent_run_record` 实体 |
| `entity/src/agent/tool_audit_log.rs` | `t_agent_tool_audit_log` 实体 |

### agent-base 新增模块

**`agent-base/src/observability.rs`** — 可观测性抽象层

```rust
// 运行记录写入器 trait
pub trait AgentRunRecorder: Send + Sync + 'static {
    async fn create(&self, entry: RunRecordEntry) -> AgentResult<()>;
    async fn finish(&self, run_id: &str, status: &str, ...) -> AgentResult<()>;
}

// 工具调用审计日志写入器 trait
pub trait AgentToolAuditLogger: Send + Sync + 'static {
    async fn log(&self, entry: ToolAuditEntry) -> AgentResult<()>;
}

// Noop 实现（测试/不需要持久化的场景）
pub struct NoopRunRecorder;
pub struct NoopToolAuditLogger;
```

### agent-platform 新增模块

**`agent-platform/src/observability.rs`** — 数据库实现

```rust
// 基于数据库的运行记录写入器
pub struct DbAgentRunRecorder { db: Arc<DatabaseConnection> }

// 基于数据库的工具调用审计日志写入器
pub struct DbAgentToolAuditLogger { db: Arc<DatabaseConnection> }
```

---

## 修改内容

### `AgentRuntime` 结构体新增字段

```rust
pub struct AgentRuntime {
    // ... 原有字段 ...
    /// 运行记录写入器（可选，不配置时使用 Noop 实现）
    run_recorder: Arc<dyn AgentRunRecorder>,
    /// 工具调用审计日志写入器（可选，不配置时使用 Noop 实现）
    tool_audit_logger: Arc<dyn AgentToolAuditLogger>,
}
```

新增 builder 方法：

- `with_run_recorder(recorder)` — 设置运行记录写入器
- `with_tool_audit_logger(logger)` — 设置工具调用审计日志写入器

### `AgentRuntime::run()` 集成

**运行开始时**：调用 `run_recorder.create()` 创建 status=running 的记录

**工具调用时**：

- 记录工具调用开始时间
- 调用完成后调用 `tool_audit_logger.log()` 写入审计日志（含耗时）

**运行结束时**：调用 `run_recorder.finish()` 更新记录状态（success/failed）

> 所有写入失败均以 `warn!` 记录，**不阻断主流程**。

### `SubAgentScheduler::delegate_fan_out` 并发限制

```rust
// 最大并发数限制，防止大量子智能体时资源耗尽
const DEFAULT_FAN_OUT_CONCURRENCY: usize = 8;
let semaphore = Arc::new(Semaphore::new(DEFAULT_FAN_OUT_CONCURRENCY));
```

通过 `tokio::sync::Semaphore` 限制同时执行的子智能体数量，超出限制的请求将等待。

---

## 注意事项

1. **两阶段写入**：运行记录采用两阶段写入（create + finish），即使运行中途崩溃，数据库中也会保留 status=running 的记录，便于排查问题
2. **非阻断设计**：所有可观测性写入失败均不阻断主流程，仅记录警告日志
3. **user_id 占位**：当前 `user_id` 使用占位值 0，需要在 service 层通过 `with_run_recorder` 注入携带用户 ID 的自定义实现
4. **并发数可配置**：`DEFAULT_FAN_OUT_CONCURRENCY = 8` 为默认值，后续可通过 `DelegationPolicy` 对象进行配置化

---

## 使用示例

### 场景一：在 agent-service 中集成运行记录

```rust
use agent_platform::{DbAgentRunRecorder, DbAgentToolAuditLogger};
use std::sync::Arc;

// 构建运行时时注入数据库实现
let runtime = resolver
    .resolve(snapshot)?
    .with_run_recorder(Arc::new(DbAgentRunRecorder::new(db.clone())))
    .with_tool_audit_logger(Arc::new(DbAgentToolAuditLogger::new(db.clone())));

let answer = runtime.run("session-1", "你好").await?;
```

### 场景二：测试时使用 Noop 实现

```rust
use agent_base::{NoopRunRecorder, NoopToolAuditLogger};

let runtime = AgentRuntime::without_memory(snapshot, planner, toolset, publisher)
    .with_run_recorder(Arc::new(NoopRunRecorder))
    .with_tool_audit_logger(Arc::new(NoopToolAuditLogger));
```

### 场景三：查询运行记录

```rust
use entity::agent::run_record::{Column, Entity};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

// 查询某智能体的最近运行记录
let records = Entity::find()
    .filter(Column::AgentId.eq(agent_id))
    .filter(Column::Status.eq("success"))
    .all(&db)
    .await?;
```

### 场景四：查询工具调用审计日志

```rust
use entity::agent::tool_audit_log::{Column, Entity};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

// 查询某次运行的所有工具调用
let logs = Entity::find()
    .filter(Column::RunId.eq(&run_id))
    .all(&db)
    .await?;

// 统计工具调用成功率
let failed = logs.iter().filter(|l| !l.success).count();
println!("失败率: {}/{}", failed, logs.len());
```

---

## 兼容性说明

- **向后兼容**：`AgentRuntime::new()` 和 `AgentRuntime::without_memory()` 默认使用 Noop 实现，无需修改现有调用代码
- **渐进式接入**：通过 builder 方法按需注入，不影响不需要持久化的场景（如测试）
- **迁移顺序**：`t_agent_run_record` 依赖 `t_agent_config`，`t_agent_tool_audit_log` 依赖 `t_agent_run_record`，已在 `migration/src/lib.rs` 中按正确顺序注册
