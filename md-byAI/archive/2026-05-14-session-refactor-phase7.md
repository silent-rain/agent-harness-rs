## 1. 功能说明

本期（**Phase 7**）为 **Session 级事件存储重构** 系列的尾声补全工作，属于"锦上添花"类的功能完善与代码质量提升，**不引入新表、不修改 trait 接口**，仅在 `agent-base` 内部增强：

| 主题 | 范围 | 价值 |
|---|---|---|
| **P0 多记忆类型写入** | `default_runner` 在 `ToolCallFinished` / `RagFinished` 事件点同步写入对应 `MemoryType` 的记忆 | 让 [`MemoryType`] 6 个变体真正各司其职，不再只有 `Message` 起作用 |
| **P1 重要性评分动态化** | `MemoryEntry` 评分从硬编码 `0.5/0.7` 改为基于内容长度、命中数、成功标志的动态计算 | 为后续向量召回 + 重要性加权提供更合理的基线 |
| **P1 AgentRef 值对象** | 新增 `AgentRef` / `SessionRef` 强类型值对象，封装 `(agent_id, user_id, session_id)` 标识 | 提供统一日志格式，简化跨函数传参，预留扩展点 |

> 主线重构（Phase 1 ~ Phase 6）已完整收口；Phase 7 完成后，session 级事件存储管理整体改造**全部达成**。

---

## 2. 新增内容

### 2.1 [`memory.rs`] — 语义化构造器与评分模块

新增 5 个 `MemoryEntry` 关联函数（语义化构造器）：

| 构造器 | 对应 `MemoryType` | 评分依据 |
|---|---|---|
| `MemoryEntry::user_message(content)` | `Message` (User) | 文本长度 |
| `MemoryEntry::assistant_answer(content)` | `Message` (Assistant) | 文本长度 |
| `MemoryEntry::tool_result(name, call_id, success, summary, json)` | `ToolResult` | 成功标志 + 摘要长度 |
| `MemoryEntry::rag_result(query, context, hit_count)` | `RagResult` | 命中数（0~4） + 上下文长度 |
| `MemoryEntry::sub_agent_result(code, summary, success)` | `SubAgentResult` | 成功标志 + 摘要长度 |

新增 `pub mod importance` 评分助手：

```rust
pub mod importance {
    pub const MIN: f32 = 0.0;
    pub const MAX: f32 = 1.0;

    pub fn for_user_message(content: &str) -> f32;
    pub fn for_assistant_answer(content: &str) -> f32;
    pub fn for_tool_result(success: bool, summary: &str) -> f32;
    pub fn for_rag_result(hit_count: usize, context_text: &str) -> f32;
    pub fn for_sub_agent_result(success: bool, summary: &str) -> f32;
}
```

**评分曲线**：

| 类型 | 基线 | 长度奖励曲线 | 命中奖励 |
|---|---|---|---|
| user_message | 0.5 | 200~800 字符 +0~0.2 线性 | — |
| assistant_answer | 0.7 | 同上 | — |
| tool_result (成功) | 0.6 | 同上 | — |
| tool_result (失败) | 0.3 | 同上 | — |
| rag_result (命中) | 0.55 | 同上 | 每条 +0.05，最多 +0.2 |
| rag_result (无命中) | 0.2 | — | — |
| sub_agent_result (成功) | 0.65 | 同上 | — |
| sub_agent_result (失败) | 0.35 | 同上 | — |

所有结果在返回前裁剪到 `[0.0, 1.0]`。

### 2.2 [`session/agent_ref.rs`] — 强类型值对象

新增模块 `crates/core/agent-base/src/session/agent_ref.rs`：

```rust
/// 智能体执行上下文引用（agent_id + user_id）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentRef {
    pub agent_id: i64,
    pub user_id: i32,
}

impl AgentRef {
    pub const fn new(agent_id: i64, user_id: i32) -> Self;
    pub fn with_session(self, session_id: impl Into<String>) -> SessionRef;
}

impl fmt::Display for AgentRef { /* "agent#7:user#42" */ }
impl From<(i64, i32)> for AgentRef { /* tuple 互转 */ }

/// 会话级三元组（agent + session_id）
pub struct SessionRef {
    pub agent: AgentRef,
    pub session_id: String,
}

impl SessionRef {
    pub fn new(agent_id: i64, user_id: i32, session_id: impl Into<String>) -> Self;
    pub const fn agent_id(&self) -> i64;
    pub const fn user_id(&self) -> i32;
}

impl fmt::Display for SessionRef { /* "agent#7:user#42:session#sess-1" */ }
```

**设计取舍**：

- 选择"**值对象 + 不强制改 trait**"的折中方案：让 `SessionService` 现有签名（`agent_id: i64, user_id: i32, session_id: &str` 三参数）继续保持自解释，避免调用方因引入值对象而增加样板代码
- `AgentRef` 主要用于：跨函数传参、日志统一、`HashMap` 键、跨进程透传（`Serialize`）
- 需要时可通过 `AgentRef::new(a, u)` 一行就近构造，无侵入

---

## 3. 修改内容

### 3.1 [`default_runner.rs`]

| 位置 | 改动 |
|---|---|
| L743 `ToolCallFinished` 成功分支 | 在 `messages.push_tool_result` 之前追加 `record_memory(MemoryEntry::tool_result(...))` |
| L800 `ToolCallFinished` 失败分支 | 同上，写入失败摘要 |
| L990 `RagFinished` 之后 | 追加 `record_memory(MemoryEntry::rag_result(query, context, hit_count))` |
| L1054 `Finalize` 助手回答 | 把硬编码 `MemoryEntry { ..., importance_score: 0.7, ... }` 替换为 `MemoryEntry::assistant_answer(...)` |
| L1146 `run_inner` 用户输入 | 把硬编码 `MemoryEntry { ..., importance_score: 0.5, ... }` 替换为 `MemoryEntry::user_message(...)` |
| 顶部 use | 移除 `MemoryType` / `MessageRole`（构造器内部已封装） |

### 3.2 [`session/mod.rs`]

新增模块声明与 re-export：

```rust
pub mod agent_ref;
pub use agent_ref::{AgentRef, SessionRef};
```

### 3.3 [`lib.rs`]

顶层 re-export 增加 `AgentRef`、`SessionRef`，方便外层 crate 直接 `use agent_base::AgentRef`。

### 3.4 顺手清理（非本期目标）

- [`session/mod.rs`] / [`runtime/services.rs`]：把 `[`xxx`]:` 文档链接改为 `[`xxx`][]:`，消除 `clippy::doc_nested_refdefs` warning

---

## 4. 注意事项

### 4.1 不破坏向后兼容

- `MemoryEntry` 公开字段未变，旧的 `MemoryEntry { memory_type, role, ... }` 直接构造方式仍可用（仅在内部场景改用语义化构造器）
- `SessionService` trait 签名 0 改动
- `RuntimeServices` 字段 0 改动

### 4.2 评分改动的兼容性

旧硬编码值（用户 0.5 / 助手 0.7）保持为新评分函数的"基线值"，因此对于**短文本**（< 200 字符）输出与之前完全一致；只有长文本会获得 +0~0.2 的奖励——属于增量优化。

### 4.3 AgentRef 不强制使用

只是公开提供，**未替换任何现有接口**。后续若 `agent-platform` / `agent-service` 调用方愿意采纳，可逐步替换 `(i64, i32)` 二元组为 `AgentRef`。

### 4.4 仍未触发的记忆类型

`MemoryType` 中以下变体当前还**无写入触发点**：

- `SubAgentResult` — 子智能体由 `ForwardingEventPublisher` 转发为 `SubAgentEvent`，未走 `default_runner` 内的直接事件发布点；构造器已就绪，等待后续 sub-agent 调用收口时接入
- `PlanStep` / `Reflection` — 当前 planner 协议未定义对应事件类型；未来扩展时可直接复用现有抽象

---

## 5. 使用示例

### 示例 1：使用 `MemoryEntry` 语义化构造器

```rust,no_run
use agent_base::memory::{MemoryEntry, MemoryType};

// 用户消息
let user = MemoryEntry::user_message("帮我查一下今天的天气");
assert_eq!(user.memory_type, MemoryType::Message);
assert!(user.importance_score >= 0.5);

// 工具调用成功结果
let tool_ok = MemoryEntry::tool_result(
    "weather_query",
    "call-001",
    true,
    "晴，25°C",
    Some(r#"{"temp":25,"cond":"sunny"}"#.to_string()),
);
assert_eq!(tool_ok.memory_type, MemoryType::ToolResult);

// RAG 检索结果（命中 3 条）
let rag = MemoryEntry::rag_result(
    "Rust 错误处理",
    "Result 类型用于...\n? 操作符...",
    3,
);
assert_eq!(rag.memory_type, MemoryType::RagResult);
assert!(rag.content.starts_with("[RAG/"));
```

**使用场景**：业务代码需要写入会话事件流前组装记忆条目。直接调用构造器可省去手动填字段，并自动获得合理的重要性评分。

### 示例 2：动态评分函数直接调用

```rust,no_run
use agent_base::memory::importance;

// 评分始终落在 [0.0, 1.0]
assert!((0.0..=1.0).contains(&importance::for_user_message("hi")));

// 长文本评分高于短文本
let short = importance::for_user_message("hi");
let long = importance::for_user_message(&"x".repeat(500));
assert!(long > short);

// RAG 命中数越多评分越高
let none = importance::for_rag_result(0, "");
let many = importance::for_rag_result(4, "long context...");
assert!(none < many);

// 工具失败基线低于成功
let ok = importance::for_tool_result(true, "done");
let err = importance::for_tool_result(false, "timeout");
assert!(ok > err);
```

**使用场景**：在不需要构造完整 `MemoryEntry` 的地方（如旧记忆条目重新评分、批量评分计算），直接复用评分逻辑。

### 示例 3：`AgentRef` 与 `SessionRef` 在日志/上下文传参中使用

```rust,no_run
use agent_base::{AgentRef, SessionRef};
use std::collections::HashMap;

// 1) 构造与 Display
let a = AgentRef::new(7, 42);
println!("{}", a);                       // agent#7:user#42

// 2) 元组互转
let a2: AgentRef = (7_i64, 42_i32).into();
assert_eq!(a, a2);

// 3) 与 session_id 组合
let s = a.with_session("sess-abc");
println!("{}", s);                       // agent#7:user#42:session#sess-abc
assert_eq!(s.agent_id(), 7);
assert_eq!(s.user_id(), 42);

// 4) 作为 HashMap 键聚合事件流
let mut buffer: HashMap<AgentRef, Vec<String>> = HashMap::new();
buffer.entry(a).or_default().push("event-1".into());

// 5) 在 tracing 日志中统一格式
tracing::info!(agent = %a, "agent run started");
// 输出: agent=agent#7:user#42 agent run started
```

**使用场景**：跨服务/跨任务携带智能体身份；统一日志输出格式；作为缓存 / 事件聚合的强类型键。

---

## 6. 兼容性说明

### 6.1 已验证

- ✅ `cargo check --workspace` 通过
- ✅ `cargo test -p agent-base --lib` 全部通过（**238 个测试**）
- ✅ `cargo clippy -p agent-base --lib --tests` 无新增 warning（仅余 1 条预存在的 `module_inception`）

### 6.2 升级路径

无破坏性变更，**直接升级即可**。已存在的 `MemoryEntry { ... }` 直接构造方式不受影响；旧硬编码评分语义保留为新函数的基线值。

### 6.3 不兼容点

无。

---

## 7. 涉及组件

| 组件 | crate | 角色 | 改动幅度 |
|---|---|---|---|
| `memory` | `agent-base` | `MemoryEntry` + `importance` 模块 | ➕ 新增构造器与评分函数 + 单测 |
| `session::agent_ref` | `agent-base` | `AgentRef` / `SessionRef` 值对象 | ➕ 新文件 |
| `session::mod` / `lib` | `agent-base` | re-export | ✏️ 注册 + 导出 |
| `runtime::runner::default_runner` | `agent-base` | 记忆写入路径 | ✏️ 4 处替换 + 2 处新增 |

---

## 8. 整体改造收尾

至此 **Session 级事件存储重构** 系列全部 7 个阶段完成：

| Phase | 主题 | 状态 |
|---|---|---|
| 1 | 数据层（5 张新表 Migration + Entity） | ✅ |
| 2 | DAO 层（`SessionDao` / `SessionEventDao` / `StateDao`） | ✅ |
| 3 | 废弃表清理（memory_stream / memory_distilled / run_record / tool_audit_log） | ✅ |
| 4 | `DbSessionService` 落地 | ✅ |
| 5 | 强类型接口 + `AgentMemoryWriter` 下线 + clippy 清零 | ✅ |
| 6 | 测试模块下沉 + `default_runner` 补齐 memory 写入路径 | ✅ |
| 7 | 多记忆类型 / 动态评分 / `AgentRef` 值对象 | ✅ |

后续可按需推进的延伸（依赖外部条件）：

- 接入 `rag-core` 实现 `DbAgentMemoryRetriever` 的向量召回
- 引入 `t_agent_session_summary` 蒸馏摘要表（需先确定触发条件）
- 编写 `memory_stream` → `session_event` 一次性数据迁移脚本（仅生产已有数据时需要）

至此架构层面"以 session 为核心"的目标完整达成，剩余项均与产品需求/外部依赖耦合。
