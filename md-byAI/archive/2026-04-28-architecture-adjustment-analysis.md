# nucleus-rs 架构调整分析

## 一、当前架构诊断

### 1.1 现有架构概览

```
nucleus-rs 当前架构：
┌──────────────────────────────────────┐
│          agent-platform              │  ← 配置装配层（DB 依赖）
│  ├── provider (DB 加载配置)           │
│  ├── memory   (DB 记忆存取)           │
│  ├── knowledge (DB 知识检索)          │
│  └── observability (DB 运行记录)      │
├──────────────────────────────────────┤
│          agent-base                  │  ← 运行时核心
│  ├── runtime/AgentRuntime            │  ← 执行循环 + 工厂（耦合）
│  ├── planner                         │  ← 结构化决策协议
│  ├── skill / rule / command          │  ← 规则引擎
│  ├── tool / mcp / sub_agent          │  ← 工具 & 子智能体
│  └── memory / observability          │  ← trait 定义 + noop 实现
└──────────────────────────────────────┘
```

### 1.2 核心问题识别

| # | 问题 | 严重程度 | 影响 |
|---|------|---------|------|
| 1 | **Runner 与 AgentRuntime 耦合** | 🔴 P0 | 无法替换执行策略，无法运行中操控 |
| 2 | **无 Callbacks/Lifecycle Hooks** | 🔴 P0 | 无扩展点，横切关注点需改核心代码 |
| 3 | **框架与存储层耦合** | 🔴 P0 | agent-platform 紧耦合 SeaORM/MySQL |
| 4 | **仅有 LLM 单循环执行模式** | 🟠 P1 | 无法表达 Chain/Parallel/Cycle/Graph |
| 5 | **Session 管理简单** | 🟠 P1 | 无分层状态、无事件分页、无摘要 |
| 6 | **串行工具调用** | 🟡 P2 | LLM 返回多 tool_call 时效率低 |

---

## 二、是否需要调整架构？

### 结论：**是的，需要调整。** 但调整策略应是 **渐进式重构**，而非推倒重来

#### 理由

1. **Runner 抽象是所有后续改进的前置依赖**
   - 没有 Runner，就无法实现 Callbacks、编排模式、运行中操控
   - 当前 `AgentRuntime::run()` 的 while 循环需要被提取出来

2. **Callbacks 机制是框架扩展性的基础**
   - tRPC-Agent-Go 的 Plugin/Callbacks 体系让它无需修改核心代码即可扩展
   - nucleus-rs 当前任何扩展都需要修改 `agent_runtime.rs`（1394 行的大文件）

3. **存储层解耦决定了框架的通用性**
   - 当前 agent-platform 强绑定 MySQL，限制了适用场景
   - 很多场景（CLI、桌面应用、边缘部署）需要轻量存储

4. **但 nucleus-rs 有自身独有优势，调整时必须保留**

---

## 三、nucleus-rs 的独有优势（必须保留）

| # | 优势 | 详情 |
|---|------|------|
| 1 | **PlannerDecision 结构化调用协议** | 所有动作走结构化枚举，禁止解析自由文本；tRPC-Agent-Go 的 LLM 输出是自由文本解析 |
| 2 | **ModelAdapter 类型安全** | rig `CompletionModel` trait 封装，编译期保证类型安全 |
| 3 | **Builder 双层 API** | 编程便捷性（链式 builder）+ 框架可扩展性（RuntimeServices 注入） |
| 4 | **Skill/Rule/Command 三层规则引擎** | 细粒度行为控制：Skill=能力包、Rule=约束规则、Command=用户自定义 |
| 5 | **Rust 所有权与内存安全** | 编译期防内存泄漏、数据竞争；Go 的 GC 无法提供这种保证 |
| 6 | **事件模型完整** | `AgentStreamEvent` 覆盖完整生命周期，含子智能体透传 |

---

## 四、推荐调整方案

### 4.1 提取 Runner 抽象（P0 — 最高优先级）

**现状**：`AgentRuntime::run()` 内嵌了 1394 行的执行循环，无法替换。

**目标架构**：

```rust
/// Runner 是 Agent 运行的调度容器，将执行逻辑与 Agent 逻辑解耦
#[async_trait]
pub trait Runner: Send + Sync + 'static {
    /// 执行 Agent，返回事件流
    fn run(
        &self,
        session_id: &str,
        user_input: &str,
        user_id: i32,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentStreamEvent> + Send>>>;

    /// 关闭 Runner
    fn close(&self) -> AgentResult<()>;
}

/// ManagedRunner 扩展 Runner，支持运行控制
#[async_trait]
pub trait ManagedRunner: Runner + Send + Sync + 'static {
    /// 取消指定请求的运行
    fn cancel(&self, request_id: &str) -> bool;

    /// 查询运行状态
    fn run_status(&self, request_id: &str) -> Option<RunStatus>;
}

/// SteerableRunner 扩展 ManagedRunner，支持安全边界内注入消息
#[async_trait]
pub trait SteerableRunner: ManagedRunner + Send + Sync + 'static {
    /// 向活跃运行注入用户消息
    fn enqueue_user_message(
        &self,
        request_id: &str,
        message: &str,
    ) -> AgentResult<()>;
}
```

**实现路径**：

1. 从 `AgentRuntime::run()` 中提取 while 循环到 `DefaultRunner`
2. `AgentRuntime` 退化为配置 + 工厂角色，通过 `build()` 生成 Runner
3. `DefaultRunner` 持有 `AgentRuntime` 的配置和 `RuntimeServices`
4. 保持 `AgentRuntime::run()` 的签名兼容，内部委托给 Runner

**关键文件变更**：

- 新增 `crates/core/agent-base/src/runner/mod.rs`
- 新增 `crates/core/agent-base/src/runner/default_runner.rs`
- 修改 `crates/core/agent-base/src/runtime/agent_runtime.rs`（简化）
- 修改 `crates/core/agent-base/src/runtime/builder.rs`（增加 Runner 构建）

### 4.2 添加 Callbacks / Lifecycle Hooks（P0）

**现状**：无任何运行时拦截点，扩展需改核心代码。

**目标架构**：

```rust
/// 回调参数与结果定义
// --- Agent 生命周期回调 ---
pub struct BeforeAgentArgs {
    pub invocation: Invocation,
    pub context: Context,
}

pub struct BeforeAgentResult {
    pub context: Option<Context>,
    /// 非空则跳过 Agent 执行，直接作为响应返回
    pub custom_response: Option<String>,
}

pub struct AfterAgentArgs {
    pub invocation: Invocation,
    pub context: Context,
    pub response: Option<AgentStreamEvent>,
    pub error: Option<AgentError>,
}

pub struct AfterAgentResult {
    pub context: Option<Context>,
    /// 非空则替换 Agent 的输出
    pub custom_response: Option<String>,
}

// --- Model 生命周期回调 ---
pub struct BeforeModelArgs {
    pub messages: Vec<ConversationMessage>,
    pub tools: Vec<ToolDefinition>,
}

pub struct BeforeModelResult {
    pub messages: Option<Vec<ConversationMessage>>,
    /// 非空则跳过 Model 调用，直接作为响应返回
    pub custom_response: Option<PlannerDecision>,
}

pub struct AfterModelArgs {
    pub decision: PlannerDecision,
}

pub struct AfterModelResult {
    pub decision: Option<PlannerDecision>,
}

// --- Tool 生命周期回调 ---
pub struct BeforeToolArgs {
    pub tool_code: String,
    pub call_id: String,
    pub arguments_json: String,
}

pub struct BeforeToolResult {
    /// 非空则跳过工具执行，直接使用此结果
    pub custom_result: Option<ToolCallResult>,
}

pub struct AfterToolArgs {
    pub tool_code: String,
    pub call_id: String,
    pub success: bool,
    pub result: Option<ToolCallResult>,
}

pub struct AfterToolResult {
    pub result: Option<ToolCallResult>,
}

/// Callbacks 聚合配置
pub struct Callbacks {
    pub before_agent: Vec<Arc<dyn BeforeAgentCallback>>,
    pub after_agent: Vec<Arc<dyn AfterAgentCallback>>,
    pub before_model: Vec<Arc<dyn BeforeModelCallback>>,
    pub after_model: Vec<Arc<dyn AfterModelCallback>>,
    pub before_tool: Vec<Arc<dyn BeforeToolCallback>>,
    pub after_tool: Vec<Arc<dyn AfterToolCallback>>,
    /// Agent 执行出错时是否继续执行后续回调
    pub continue_on_error: bool,
    /// Before 回调返回自定义响应时是否继续执行后续回调
    pub continue_on_response: bool,
}
```

**实现路径**：

1. 在 `agent-base` 中定义 Callbacks 相关 trait 和结构体
2. 在 `DefaultRunner` 的执行循环中插入回调调用点
3. 通过 `AgentRuntimeBuilder` 注册回调
4. 提供 `CallbackRegistry` 管理回调的注册和执行顺序

**回调在 Runner 中的调用位置**：

```text
Runner.run()
  ├── before_agent 回调 ← 可以跳过 Agent 或修改上下文
  ├── Agent 执行（LLM 推理）
  │     ├── before_model 回调 ← 可以跳过 Model 或修改消息
  │     ├── ModelAdapter::completion()
  │     └── after_model 回调 ← 可以替换决策
  ├── 处理 PlannerDecision
  │     ├── CallTool → before_tool / after_tool 回调
  │     ├── QueryKnowledge / RecallMemory / UpdatePlan / Finalize
  └── after_agent 回调 ← 可以替换输出
  └── 最终返回
```

### 4.3 存储层抽象（P0）

**现状**：`agent-platform` 紧耦合 SeaORM + MySQL。

**目标架构**：

```rust
/// 会话存储后端 trait — 解耦框架与具体数据库
#[async_trait]
pub trait SessionStorage: Send + Sync + 'static {
    /// 创建或更新会话
    async fn upsert_session(
        &self,
        key: &SessionKey,
        state: &SessionState,
    ) -> AgentResult<()>;

    /// 获取会话（含事件分页）
    async fn get_session(
        &self,
        key: &SessionKey,
        options: &SessionGetOptions,
    ) -> AgentResult<Option<Session>>;

    /// 追加事件
    async fn append_event(
        &self,
        key: &SessionKey,
        event: &AgentStreamEvent,
    ) -> AgentResult<()>;

    /// 删除会话
    async fn delete_session(&self, key: &SessionKey) -> AgentResult<()>;

    /// 关闭存储后端
    async fn close(&self) -> AgentResult<()>;
}

/// 会话获取选项
pub struct SessionGetOptions {
    /// 事件分页：页码
    pub event_page: Option<u32>,
    /// 事件分页：每页数量
    pub event_page_size: Option<u32>,
}

/// 会话状态（三级分层）
pub struct SessionState {
    /// 应用级状态（跨用户共享）
    pub app_state: HashMap<String, serde_json::Value>,
    /// 用户级状态（跨会话共享）
    pub user_state: HashMap<String, serde_json::Value>,
    /// 会话级状态（当前会话独有）
    pub session_state: HashMap<String, serde_json::Value>,
}
```

**内置实现**：

| 实现 | 适用场景 | 依赖 |
|------|---------|------|
| `SqliteSessionStorage` | CLI / 桌面 / 边缘 | 仅 rusqlite |
| `PostgresSessionStorage` | 生产级部署 | tokio-postgres |
| `MysqlSessionStorage` | 兼容现有部署 | sqlx |
| `InMemorySessionStorage` | 测试 | 无 |
| `SeaOrmSessionStorage` | 迁移过渡 | sea-orm |

**实现路径**：

1. 在 `agent-base` 中定义 `SessionStorage` trait
2. 在 `agent-platform` 中将现有的 SeaORM 操作重构为 `SeaOrmSessionStorage`
3. 将 `RuntimeServices` 中的存储相关字段改为 `Arc<dyn SessionStorage>`
4. 通过 `AgentRuntimeBuilder` 注入具体实现
5. 新增 `SqliteSessionStorage`（零外部依赖版本）

### 4.4 Agent 编排模式（P1）

**现状**：仅有 LLM 单循环 + 子智能体委派。

**目标架构**：

```rust
/// Agent 执行策略 trait
#[async_trait]
pub trait ExecutionStrategy: Send + Sync + 'static {
    /// 执行策略
    fn execute(
        &self,
        runner: &dyn Runner,
        ctx: &mut Context,
        agents: &dyn SubAgentRegistry,
        invocation: Invocation,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentStreamEvent> + Send>>>;
}

// 具体策略实现：

/// 1. LLM 单循环（当前实现）
pub struct LlmLoopStrategy;

/// 2. 链式执行：A → B → C
pub struct ChainStrategy {
    agents: Vec<Arc<dyn Agent>>,
}

/// 3. 并行执行：A | B | C，结果合并
pub struct ParallelStrategy {
    agents: Vec<Arc<dyn Agent>>,
}

/// 4. 循环执行：A ↔ B ↔ ... ↔ A（直到收敛）
pub struct CycleStrategy {
    agents: Vec<Arc<dyn Agent>>,
    max_cycles: u32,
}

/// 5. DAG 工作流：条件分支 + 并行 + 回溯
pub struct GraphStrategy {
    graph: Graph,
}
```

**实现路径**：

1. 定义 `ExecutionStrategy` trait
2. 将 `AgentRuntime::run()` 中的循环逻辑提取为 `LlmLoopStrategy`
3. 逐步实现 `ChainStrategy` 和 `ParallelStrategy`
4. 后续实现 `CycleStrategy` 和 `GraphStrategy`

---

## 五、演进路线

### 阶段一：解耦与抽象（3-4 周）

| # | 任务 | 产出 | 风险 |
|---|------|------|------|
| 1 | 提取 Runner trait | `runner/mod.rs`, `runner/default_runner.rs` | 中 — 需拆分 1394 行大文件 |
| 2 | 添加 Callbacks 机制 | `callbacks/mod.rs`, 回调 trait 定义 | 低 — 纯新增 |
| 3 | 定义 SessionStorage trait | `storage/mod.rs`, `storage/session_storage.rs` | 中 — 需重构 agent-platform |
| 4 | 实现内置 SqliteSessionStorage | `storage/sqlite_session_storage.rs` | 低 — 新增文件 |

### 阶段二：核心能力增强（2-3 周）

| # | 任务 | 产出 |
|---|------|------|
| 5 | 实现 Chain / Parallel 执行策略 | `strategy/chain.rs`, `strategy/parallel.rs` |
| 6 | 完善 Session 三级状态管理 | 状态分层 + 分页 + 摘要生成 |
| 7 | 实现并行工具调用 | 多 tool_call 并行执行 |

### 阶段三：高级编排（2-3 周）

| # | 任务 | 产出 |
|---|------|------|
| 8 | Graph DAG 执行引擎 | `graph/` 模块 |
| 9 | Checkpoint / 恢复机制 | 断点续跑 |
| 10 | Plugin 插件系统（非 MCP） | 6 个拦截点 |

---

## 六、调整原则

1. **渐进式重构**：每一步都可编译、可测试、可发布
2. **向后兼容**：`AgentRuntime::run()` 签名保持兼容，内部委托给 Runner
3. **保持独有优势**：PlannerDecision 结构化协议、ModelAdapter 类型安全等
4. **默认实现优先**：Callback / Storage 均提供 Noop 默认实现
5. **测试覆盖**：每个新 trait 都有对应的单元测试

## 七、总结

| 改进项 | 优先级 | 预估工作量 | 前置依赖 |
|--------|--------|-----------|---------|
| Runner 抽象 | P0 | 1-1.5 周 | 无 |
| Callbacks 机制 | P0 | 1 周 | Runner |
| 存储层抽象 | P0 | 1-1.5 周 | 无 |
| Agent 编排模式 | P1 | 2-3 周 | Runner + Callbacks |
| Session 结构化管理 | P1 | 1 周 | 存储层抽象 |
| 并行工具调用 | P1 | 0.5 周 | 无 |
| Graph DAG 引擎 | P2 | 2 周 | Agent 编排模式 |
| Plugin 插件系统 | P2 | 1.5 周 | Callbacks |

**核心路径**：Runner → Callbacks → 存储层抽象 → 编排模式 → Graph/Plugin

**总预估**：8-10 周（含测试和文档）
