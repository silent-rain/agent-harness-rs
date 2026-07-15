# SteerableAgent 注入队列重构

## 功能说明

将 `SteerableAgent::enqueue_user_message` 的消息注入机制从"直接写入 `t_agent_session_state` + 基于 id 增量读取"重构为"独立队列表 `t_agent_session_pending_input` + drain 消费即固化"模式。

### 重构动机

原设计存在以下问题：

1. **语义混淆**：`t_agent_session_state` 同时承担"已固化对话历史"和"待消费注入队列"两个角色
2. **Race Condition**：`id > baseline` 的增量读取在 baseline 取值窗口存在竞态
3. **注入位置不当**：注入消息直接追加到 messages 末尾，可能紧跟 tool 结果，语义诡异
4. **无"已消费"标记**：进程重启后可能重复加载或丢失注入消息

### 新设计

独立的 `t_agent_session_pending_input` 队列表 + "消费即固化"事务语义：

```text
用户/API 调用 enqueue_user_message
    │
    ▼
t_agent_session_pending_input (consumed_at = NULL)
    │
    ▼ drive_react_loop 每轮开头 drain
    │
    ├─► 标记 consumed_at = NOW()
    ├─► 固化到 t_agent_session_state（容错）
    └─► push 到本轮 messages
```

## 新增内容

### 数据库表

| 表名                            | 说明                                |
| ------------------------------- | ----------------------------------- |
| `t_agent_session_pending_input` | 会话注入消息队列（FIFO + 消费标记） |

字段：`id`, `session_id`, `agent_id`, `user_id`, `content_json`, `consumed_at`, `created_at`

### 新增文件

| 文件                                                       | 说明                   |
| ---------------------------------------------------------- | ---------------------- |
| `agent-migration/src/agent/agent_session_pending_input.rs` | Migration 定义         |
| `agent-entity/src/agent/session_pending_input.rs`          | SeaORM Entity          |
| `agent-assembly/src/session/pending_input_dao.rs`          | DAO（enqueue + drain） |

### 新增 Trait 方法

`SessionService` trait 新增：

- `enqueue_pending_input(session_id, agent_id, user_id, message)` — 写入队列
- `drain_pending_input(session_id)` — 消费全部未处理消息

`RuntimeServices` 新增容错代理：

- `try_enqueue_pending_input(...)` — 容错写入
- `drain_pending_input(session_id)` — 容错消费

## 修改内容

### `SessionService` trait (`agent-runtime/src/session/traits.rs`)

- **移除** `list_context_after` 方法（默认实现）
- **移除** `get_max_context_id` 方法（默认实现）
- **新增** `enqueue_pending_input` 方法（默认空实现）
- **新增** `drain_pending_input` 方法（默认空实现）

### `DbSessionService` (`agent-assembly/src/session/db_session_service.rs`)

- 新增 `pending_input_dao` 字段
- 实现 `enqueue_pending_input`：序列化后委托 DAO
- 实现 `drain_pending_input`：委托 DAO drain + 反序列化
- **移除** `list_context_after` 实现
- **移除** `get_max_context_id` 实现

### `InMemorySessionService` (`agent-runtime/src/session/in_memory.rs`)

- 新增 `pending_inputs` 内存池字段
- 实现 `enqueue_pending_input`：push 到内存队列
- 实现 `drain_pending_input`：remove 并返回
- **移除** `list_context_after` 实现

### `RuntimeServices` (`agent-runtime/src/runtime/services.rs`)

- **移除** `list_context_after` 容错代理
- **移除** `get_max_context_id` 容错代理
- **新增** `try_enqueue_pending_input` 容错代理
- **新增** `drain_pending_input` 容错代理

### `LlmAgent::enqueue_user_message` (`agent-runtime/src/runtime/agent/llm_agent.rs`)

- 改为调用 `services.try_enqueue_pending_input(...)` 写入队列表
- 不再直接写 `t_agent_session_state`

### `LlmAgent::drive_react_loop` (`agent-runtime/src/runtime/agent/llm_agent.rs`)

- **移除** `context_baseline_id` 参数
- **移除** `last_context_id` 内部状态变量
- 每轮开头调用 `services.drain_pending_input(session_id)`
- 消费后同步固化到 `t_agent_session_state`（"消费即固化"）
- 然后 push 到 messages

### `LlmAgent::run_stream_inner` (`agent-runtime/src/runtime/agent/llm_agent.rs`)

- **移除** `get_max_context_id` 调用
- **移除** `context_baseline_id` 变量
- 简化 `drive_react_loop` 调用参数

### `AgentSessionDao` (`agent-service/src/dao/agent_session.rs`)

- `append_session_state` → `append_pending_input`（写入 pending_input 表）

### `AgentSessionService` (`agent-service/src/service/agent_session.rs`)

- `inject_message` 改为调用 `append_pending_input`

## 注意事项

1. **SessionStateDao 的 `list_after` / `get_max_id` 方法保留**：虽然不再有调用方，但作为 DAO 层的通用能力保留，未来可能有其他用途。如确认无用可后续清理。

2. **数据迁移**：如果生产环境已有通过旧方式注入但尚未被消费的消息（存在于 `t_agent_session_state` 中），需要手动迁移到新表。但由于旧设计中没有"已消费"标记，实际上这些消息已经在上次 run 中被全量加载过了，无需迁移。

3. **并发安全**：`PendingInputDao::drain` 的"读取 + 标记"操作在高并发下理论上存在 TOCTOU 问题（两个 drain 同时读到同一批记录）。但由于同一 session 同一时刻只有一个 `drive_react_loop` 在运行（由 `SessionStatus::Running` 互斥保证），实际不会发生。

4. **队列清理**：已消费的记录（`consumed_at IS NOT NULL`）可通过定时任务清理，或设置 TTL。当前未实现自动清理。

## 使用示例

### 场景 1：HTTP API 注入消息

```rust
// controller 层
pub async fn inject_message(...) -> Responder<InjectMessageResp> {
    let service: AgentSessionService = provider.provide();
    let resp = service.inject_message(&user_id, req).await?;
    Ok(Response::data(resp).to_json()?)
}
// 内部写入 t_agent_session_pending_input，下一轮 ReAct 循环自动消费
```

### 场景 2：通过 SteerableAgent trait 注入

```rust
let agent: Arc<dyn SteerableAgent> = ...;
agent.enqueue_user_message(session_id, "请只修改 TS 文件").await?;
// 消息写入队列，drive_react_loop 下一轮自动 drain
```

### 场景 3：drive_react_loop 消费流程

```rust
// 每轮开头自动执行：
let injected = self.services.drain_pending_input(session_id).await;
if !injected.is_empty() {
    for msg in &injected {
        // 固化到 session_state
        self.services.try_append_context(scope, msg).await;
    }
    for msg in injected {
        messages.push(msg);
    }
}
```

## 兼容性说明

- **API 兼容**：HTTP 接口 `POST /session/inject_message` 行为不变，仅底层存储表变更
- **Trait 兼容**：`SteerableAgent::enqueue_user_message` 签名不变
- **SessionService trait**：移除了 `list_context_after` / `get_max_context_id`（有默认实现，不影响已有实现者编译）；新增方法也有默认空实现
- **数据库**：需要执行 migration 创建新表 `t_agent_session_pending_input`
