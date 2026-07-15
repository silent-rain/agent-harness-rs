# Phase 4：知识与记忆接入实现记录

## 功能说明

Phase 4 为智能体运行时接入了**知识检索**和**记忆持久化**能力，使 Agent 能够：

- 在运行时通过 `QueryKnowledge` 动作检索知识库内容（接入 `rag-core`）
- 将每次运行产生的流水记忆写入数据库（`t_agent_memory_stream`）
- 在规划前从数据库召回相关历史记忆
- 支持精炼记忆的存储（`t_agent_memory_distilled`，蒸馏任务在 Phase 7 实现）

---

## 新增内容

### 1. 数据库表（Migration）

#### `t_agent_memory_stream` — 流水记忆表（短期记忆）

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 记忆ID |
| `agent_id` | BIGINT | 所属智能体ID |
| `user_id` | BIGINT | 所属用户ID |
| `session_id` | VARCHAR(64) | 会话ID |
| `run_id` | VARCHAR(64) | 运行ID |
| `memory_type` | VARCHAR(32) | 记忆类型（message/tool_result/rag_result/...） |
| `role` | VARCHAR(32) | 角色（user/assistant/tool） |
| `content` | TEXT | 内容文本 |
| `content_json` | TEXT | 结构化内容（JSON，可选） |
| `importance_score` | FLOAT | 重要性评分（0.0 ~ 1.0） |
| `tool_name` | VARCHAR(128) | 关联工具名称（可选） |
| `tool_call_id` | VARCHAR(64) | 关联工具调用ID（可选） |
| `distilled` | BOOLEAN | 是否已被蒸馏 |
| `created_at` | DATETIME | 创建时间 |

#### `t_agent_memory_distilled` — 精炼记忆表（长期记忆）

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 精炼记忆ID |
| `agent_id` | BIGINT | 所属智能体ID |
| `user_id` | BIGINT | 所属用户ID |
| `session_id` | VARCHAR(64) | 来源会话ID（可跨会话蒸馏） |
| `title` | VARCHAR(256) | 记忆标题（LLM 生成） |
| `content` | TEXT | 精炼后的记忆内容 |
| `tags` | TEXT | 标签（JSON 数组） |
| `importance_score` | FLOAT | 重要性评分 |
| `embedding_id` | BIGINT | 关联向量 embedding ID |
| `source_stream_ids` | TEXT | 来源流水记忆 ID 列表（JSON 数组） |
| `created_at` | DATETIME | 创建时间 |
| `updated_at` | DATETIME | 更新时间 |

### 2. Entity（SeaORM）

- `entity::agent::memory_stream::Model` — 流水记忆实体
- `entity::agent::memory_distilled::Model` — 精炼记忆实体

### 3. agent-base：知识检索 trait

新增 `AgentKnowledgeRetriever` trait 和相关类型（位于 `agent_base::memory`）：

```rust
/// 知识检索结果
pub struct KnowledgeQueryResult {
    pub context_text: String,
    pub hit_count: usize,
}

/// 知识检索器 trait
#[async_trait]
pub trait AgentKnowledgeRetriever: Send + Sync + 'static {
    async fn query(
        &self,
        kb_ids: &[i64],
        query: &str,
        user_id: i32,
        top_k: usize,
    ) -> AgentResult<KnowledgeQueryResult>;
}

/// 空实现（测试用）
pub struct NoopKnowledgeRetriever;
```

### 4. agent-base：AgentRuntime 扩展

- 新增 `knowledge_retriever` 字段（默认 `NoopKnowledgeRetriever`）
- 新增 `with_knowledge_retriever()` builder 方法
- `QueryKnowledge` 动作现在调用真实的知识检索器（替换占位符）

```rust
let runtime = AgentRuntime::new(snapshot, planner, registry, publisher, writer, retriever)
    .with_knowledge_retriever(Arc::new(knowledge_adapter));
```

### 5. agent-platform：记忆实现

**`DbAgentMemoryWriter`**（`agent_platform::memory`）：

- 将流水记忆写入 `t_agent_memory_stream`
- 依赖 `Arc<DatabaseConnection>`

**`DbAgentMemoryRetriever`**（`agent_platform::memory`）：

- 从 `t_agent_memory_stream` 按重要性评分降序 + 时间降序召回
- 当前为数据库查询实现，后续可升级为向量语义召回

### 6. agent-platform：知识检索适配器

**`AgentKnowledgeAdapter`**（`agent_platform::knowledge`）：

- 封装 `rag-core` 的向量检索流程
- 实现 `AgentKnowledgeRetriever` trait
- 支持多知识库并发检索 + 结果合并

---

## 修改内容

| 文件 | 修改说明 |
|------|---------|
| `agent-base/src/memory/mod.rs` | 新增 `AgentKnowledgeRetriever` trait、`KnowledgeQueryResult`、`NoopKnowledgeRetriever` |
| `agent-base/src/runtime/agent_runtime.rs` | 添加 `knowledge_retriever` 字段，替换 `QueryKnowledge` 占位符 |
| `agent-base/src/lib.rs` | 导出新增的知识检索类型 |
| `agent-platform/src/lib.rs` | 导出 `memory` 和 `knowledge` 模块 |
| `agent-platform/Cargo.toml` | 添加 `rag-core` 依赖 |
| `entity/src/agent/mod.rs` | 注册 `memory_stream`、`memory_distilled` 模块 |
| `migration/src/agent/mod.rs` | 注册记忆表迁移模块 |
| `migration/src/lib.rs` | 注册记忆表迁移到 Migrator |

---

## 注意事项

1. **`DbAgentMemoryWriter` 的 `user_id` 字段**：当前写入时 `user_id` 使用占位值 `0`，需要在 `AgentRunService` 层将 `user_id` 传入运行时（Phase 7 完善）。

2. **记忆召回当前为简单数据库查询**：按重要性 + 时间排序，不支持语义相似度。后续可通过 `rag-core` 的向量检索升级为语义召回。

3. **精炼记忆蒸馏任务**：`t_agent_memory_distilled` 表已就绪，蒸馏逻辑（LLM 总结 + 向量化）将在 Phase 7 通过 `job-engine` 异步执行。

4. **知识检索适配器需要外部注入**：`AgentKnowledgeAdapter` 需要 `Embedder` 和 `Retriever` 实例，这些由 `agent-service` 层从配置中构建并注入。

---

## 使用示例

### 示例 1：使用数据库记忆层

```rust
use std::sync::Arc;
use agent_platform::memory::{DbAgentMemoryWriter, DbAgentMemoryRetriever};
use agent_base::AgentRuntime;

// 构建带数据库记忆的运行时
let memory_writer = Arc::new(DbAgentMemoryWriter::new(Arc::new(db.clone())));
let memory_retriever = Arc::new(DbAgentMemoryRetriever::new(Arc::new(db.clone())));

let runtime = AgentRuntime::new(
    snapshot,
    planner,
    registry,
    publisher,
    memory_writer,
    memory_retriever,
);

let answer = runtime.run("session-1", "你好").await?;
```

### 示例 2：接入知识检索

```rust
use std::sync::Arc;
use agent_platform::knowledge::AgentKnowledgeAdapter;

// 构建知识检索适配器（embedder 和 retriever 由 rag-core 提供）
let knowledge_adapter = Arc::new(AgentKnowledgeAdapter::new(embedder, retriever));

let runtime = AgentRuntime::new(snapshot, planner, registry, publisher, writer, retriever)
    .with_knowledge_retriever(knowledge_adapter);

// Agent 现在可以通过 QueryKnowledge 动作检索知识库
let answer = runtime.run("session-1", "请查询知识库中关于 Rust 的内容").await?;
```

### 示例 3：Noop 实现（测试场景）

```rust
use agent_base::{NoopKnowledgeRetriever, NoopMemoryWriter, NoopMemoryRetriever};

// 测试时使用 Noop 实现，不依赖数据库
let runtime = AgentRuntime::new(
    snapshot, planner, registry, publisher,
    Arc::new(NoopMemoryWriter),
    Arc::new(NoopMemoryRetriever),
)
.with_knowledge_retriever(Arc::new(NoopKnowledgeRetriever));
```

---

## 兼容性说明

- `AgentRuntime::new()` 签名不变，`knowledge_retriever` 通过 `with_knowledge_retriever()` 可选注入
- 不注入时默认使用 `NoopKnowledgeRetriever`，行为与 Phase 3 一致
- 所有现有测试继续通过，无破坏性变更
