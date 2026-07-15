# tRPC-Agent-Go vs nucleus-rs 整体框架差异分析

## 一、架构设计理念对比

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|--------------|-----------|
| **设计哲学** | 模块化 + 事件驱动，以 Runner 为调度核心 | 分层架构 + 结构化协议，以 AgentRuntime 为核心 |
| **核心抽象** | `Runner` 接口 → `runner` 结构体 | `AgentRuntime` 结构体 + `Agent` trait |
| **执行模型** | Runner 持有 Agent，Runner 调度事件循环 | AgentRuntime 内嵌 while 循环，直接驱动 LLM |
| **配置方式** | `Option` 函数式选项 + `AgentFactory` | `AgentRuntimeBuilder` 构建 + `RuntimeServices` 聚合 |

### 核心差异解读

**tRPC-Agent-Go** 的设计核心是 **Runner 抽象**。Runner 作为 Agent 的运行容器和调度器，将执行逻辑与 Agent 逻辑解耦。Agent 只负责单次调用的逻辑（给定 invocation，返回事件流），而 Runner 负责会话管理、请求生命周期、回调调度、插件管理等运行时职责。

**nucleus-rs** 的设计核心是 **AgentRuntime**。AgentRuntime 直接内嵌了执行循环（while loop），将运行逻辑与 Agent 逻辑耦合在一起。没有独立的 Runner 层，所有运行时职责（会话、记忆、回调等）都通过 `RuntimeServices` 直接注入到 AgentRuntime 中。

---

## 二、核心组件架构对比

### 2.1 Runner 层（tRPC-Agent-Go 独有）

```go
// tRPC-Agent-Go 的 Runner 接口层级
type Runner interface {
    Run(ctx, userID, sessionID, message, ...RunOption) (<-chan *event.Event, error)
    Close() error
}

type ManagedRunner interface {
    Runner
    Cancel(requestID string) bool
    RunStatus(requestID string) (RunStatus, bool)
}

type SteerableRunner interface {
    ManagedRunner
    EnqueueUserMessage(requestID string, message model.Message) error
}
```

**nucleus-rs 无 Runner 抽象**。`AgentRuntime::run()` 方法承担了 Runner 的全部职责，但没有将其抽象为独立接口：

```rust
// nucleus-rs 的执行入口
pub async fn run(&self, session_id: &str, user_input: &str, user_id: i32) -> AgentResult<String>
```

**关键差异**：

- tRPC-Agent-Go 支持通过 `SteerableRunner` 在安全边界内向运行中的 Agent 注入用户消息
- tRPC-Agent-Go 支持通过 `ManagedRunner` 查询运行状态和取消运行
- nucleus-rs 的 `run()` 是一次性调用，不支持运行中操控

### 2.2 Agent 接口

```go
// tRPC-Agent-Go
type Agent interface {
    Run(ctx context.Context, invocation *Invocation) (<-chan *event.Event, error)
    Tools() []tool.Tool
    Info() Info
    SubAgents() []Agent
    FindSubAgent(name string) Agent
}
```

```rust
// nucleus-rs — 无独立 Agent trait
// AgentRuntime 内嵌了 Agent 的全部逻辑
```

**关键差异**：

- tRPC-Agent-Go 的 `Agent.Run()` 返回事件 channel，是真正的流式响应
- nucleus-rs 的 `AgentRuntime::run()` 返回最终字符串，非流式
- tRPC-Agent-Go 的 Agent 可以暴露子智能体列表，支持动态发现
- nucleus-rs 通过 `DelegateSubAgentTool` 实现子智能体委派，但子智能体是预配置的

### 2.3 Callbacks / Lifecycle Hooks

```go
// tRPC-Agent-Go 的结构化回调体系
type BeforeAgentCallbackStructured = func(ctx, *BeforeAgentArgs) (*BeforeAgentResult, error)
type AfterAgentCallbackStructured  = func(ctx, *AfterAgentArgs) (*AfterAgentResult, error)

type Callbacks struct {
    BeforeAgent []BeforeAgentCallbackStructured
    AfterAgent  []AfterAgentCallbackStructured
    continueOnError    bool  // 出错时是否继续
    continueOnResponse bool  // 收到自定义响应时是否继续
}
```

**nucleus-rs 无 Callback 机制**。没有任何运行时拦截点。

**关键差异**：

- tRPC-Agent-Go 支持在 Agent 执行前后插入自定义逻辑（鉴权、参数变换、审计等）
- Before 回调可以返回 CustomResponse 跳过 Agent 执行
- After 回调可以替换 Agent 的输出
- 支持错误继续和响应继续的控制策略
- nucleus-rs 完全没有这种扩展能力

### 2.4 插件系统

```go
// tRPC-Agent-Go Plugin 接口
type Plugin interface {
    Name() string
    Register(r *Registry)
}

// Plugin Registry 提供的注册点
type Registry struct {
    name string
    mgr  *Manager
}

func (r *Registry) BeforeAgent(cb)
func (r *Registry) AfterAgent(cb)
func (r *Registry) BeforeModel(cb)
func (r *Registry) AfterModel(cb)
func (r *Registry) BeforeTool(cb)
func (r *Registry) AfterTool(cb)
func (r *Registry) OnEvent(hook EventHook)
```

**nucleus-rs 的 MCP 方式**：通过 `McpRegistry` 连接外部 MCP 服务器，但不支持插件级别的回调注入。

**关键差异**：

- tRPC-Agent-Go 的 Plugin 可以拦截 6 个生命周期节点（Agent 前/后、Model 前/后、Tool 前/后、事件流）
- nucleus-rs 仅通过 MCP 外部调用接入工具，无内部插件机制
- Plugin 还实现了 `Closer` 接口，支持优雅关闭

### 2.5 事件系统

```go
// tRPC-Agent-Go 的丰富事件类型
// Agent 生命周期事件
RunStarted / RunFinished
StepStarted
// 工具调用事件
ToolCallStarted / ToolCallFinished
// RAG 事件
RagStarted / RagFinished
// LLM 交互事件
ModelCallStarted / ModelCallFinished
// 流式输出事件
AnswerDelta
// 状态事件
Warning / Error
```

```rust
// nucleus-rs 的事件枚举
pub enum AgentStreamEvent {
    RunStarted { run_id, agent_id, agent_name }
    StepStarted { run_id, step }
    ToolCallStarted { run_id, step, tool_code, call_id, arguments_json }
    ToolCallFinished { run_id, step, tool_code, call_id, success, result_summary }
    RagStarted { run_id, step, kb_ids, query }
    RagFinished { run_id, step, hit_count }
    AnswerDelta { run_id, delta }
    RunFinished { run_id, success, total_steps, finish_reason }
    Warning { run_id, message }
    Error { run_id, code, message }
}
```

**对比结论**：事件类型基本一致，nucleus-rs 缺少 `ModelCall` 相关事件和 `StepFinished` 事件。

### 2.6 会话管理

```go
// tRPC-Agent-Go Session Service
type Service interface {
    CreateSession(ctx, key, state, ...Option) (*Session, error)
    GetSession(ctx, key, ...Option) (*Session, error)
    ListSessions(ctx, userKey, ...Option) ([]*Session, error)
    DeleteSession(ctx, key, ...Option) error
    // 状态管理（分层）
    UpdateAppState(ctx, appName, state) error
    ListAppStates(ctx, appName) (StateMap, error)
    DeleteAppState(ctx, appName, key) error
    UpdateUserState(ctx, userKey, state) error
    ListUserStates(ctx, userKey) (StateMap, error)
    DeleteUserState(ctx, userKey, key) error
    UpdateSessionState(ctx, key, state) error
    // 事件追加
    AppendEvent(ctx, session, event, ...Option) error
    // 摘要
    CreateSessionSummary(ctx, sess, filterKey, force) error
    EnqueueSummaryJob(ctx, sess, filterKey, force) error
    GetSessionSummaryText(ctx, sess, ...SummaryOption) (string, bool)
    Close() error
}
```

**nucleus-rs** 通过 `agent-platform` 的 `session_service` 提供类似功能，但：

- 没有分层的 `StateMap` 管理（App/User/Session 三级）
- 没有事件分页（`GetSession` 的 `WithEventPage` 选项）
- 没有异步摘要生成（`EnqueueSummaryJob`）
- Session 数据结构简单（`session_id` 字符串透传），无生命周期管理

### 2.7 Graph / DAG 执行引擎

```go
// tRPC-Agent-Go Graph 模块
// 支持两种调度模式：
// 1. BSP（广度优先）— 适合并行节点
// 2. DAG（有向无环图）— 适合复杂依赖关系
// 核心文件：
// - graph.go: Graph 构建与验证
// - executor.go: 执行引擎
// - executor_dag.go: DAG 执行器
// - state_graph.go: 状态图构建器
// - checkpoint.go: 执行状态持久化与恢复
```

**nucleus-rs 无 Graph/DAG 支持**。当前只有 LLM 单循环 + 子智能体委派模式。

---

## 三、需要补充的关键能力

### P0 — 架构级缺失（影响框架可演进性）

| # | 能力 | 描述 | 参考来源 | 建议优先级 |
|---|------|------|----------|-----------|
| 1 | **Runner 抽象** | 将 `AgentRuntime::run()` 中的执行循环抽象为 `Runner` trait，支持替换执行策略 | `trpc-agent-go/runner/runner.go` | **最高** |
| 2 | **Callbacks / Lifecycle Hooks** | 增加 Agent/Model/Tool 前后的拦截点 | `trpc-agent-go/agent/callbacks.go` | **最高** |
| 3 | **框架与存储层分离** | 定义 `StorageBackend` trait，将 SeaORM/MySQL 硬编码替换为可插拔后端 | `trpc-agent-go/session` 多后端实现 | **最高** |

### P1 — 核心功能缺失

| # | 能力 | 描述 | 参考来源 | 建议优先级 |
|---|------|------|----------|-----------|
| 4 | **Agent 编排模式** | 支持 Chain / Parallel / Cycle / Graph 等多种协作模式 | `trpc-agent-go/agent/chainagent/`, `parallelagent/`, `cycleagent/`, `graphagent/` | **高** |
| 5 | **Session 结构化管理** | 三级状态管理（App/User/Session）、事件分页、生命周期事件 | `trpc-agent-go/session/session.go` | **高** |
| 6 | **并行工具调用** | LLM 返回多个 `tool_call` 时并行执行 | `trpc-agent-go/runner/runner.go` | **高** |
| 7 | **CodeExecutor 安全沙箱** | 容器隔离的代码执行环境 | `trpc-agent-go/codeexecutor/container/` | **高** |
| 8 | **事件订阅/过滤** | 支持订阅特定事件类型或匹配条件的事件 | `trpc-agent-go/agent/stream_hub.go` | 中 |

### P2 — 增强能力缺失

| # | 能力 | 描述 | 参考来源 | 建议优先级 |
|---|------|------|----------|-----------|
| 9 | **Graph DAG 执行引擎** | 条件分支、并行节点、回溯等复杂工作流 | `trpc-agent-go/graph/` | 中 |
| 10 | **Checkpoint / 恢复** | 执行状态持久化和断点续跑 | `trpc-agent-go/graph/checkpoint/` | 中 |
| 11 | **向量召回实现** | 完善 `AgentMemoryRetriever` 的向量检索 | nucleus-rs 已有 trait，缺实现 | 中 |
| 12 | **A2A 协议** | Agent-to-Agent 通信协议 | `trpc-agent-go/agent/a2aagent/` | 中 |
| 13 | **Plugin 插件系统** | 通用插件管理器（非 MCP 方式） | `trpc-agent-go/plugin/` | 中 |
| 14 | **MCP 连接池/复用** | 缓存 MCP 连接，避免重复建立 | 当前 `build()` 每次重连 | 低 |
| 15 | **重试/断路器** | MCP/LLM 外部依赖的重试和断路器 | — | 低 |

---

## 四、具体实现建议

### 4.1 Runner 抽象（P0）

```rust
/// Runner 是 Agent 运行的调度容器
#[async_trait]
pub trait Runner: Send + Sync + 'static {
    /// 执行 Agent 运行，返回事件流
    fn run(
        &self,
        ctx: Context,
        user_id: i32,
        session_id: &str,
        message: &str,
        options: Vec<RunOption>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentStreamEvent> + Send>>>;

    /// 关闭 Runner，释放资源
    fn close(&self) -> AgentResult<()>;
}

/// ManagedRunner 扩展 Runner，支持运行控制
#[async_trait]
pub trait ManagedRunner: Runner {
    /// 取消指定请求 ID 的运行
    fn cancel(&self, request_id: &str) -> bool;

    /// 查询运行状态
    fn run_status(&self, request_id: &str) -> Option<RunStatus>;
}

/// SteerableRunner 扩展 ManagedRunner，支持安全边界内注入用户消息
#[async_trait]
pub trait SteerableRunner: ManagedRunner {
    /// 向活跃运行注入用户消息
    fn enqueue_user_message(
        &self,
        request_id: &str,
        message: &str,
    ) -> AgentResult<()>;
}
```

**实现路径**：将 `AgentRuntime::run()` 中的 while 循环提取到 `DefaultRunner` 中，`AgentRuntime` 退化为配置和工厂角色。

### 4.2 Callbacks 机制（P0）

```rust
/// Agent 执行前回调参数
pub struct BeforeAgentArgs {
    pub invocation: Invocation,
}

/// Agent 执行前回调结果
pub struct BeforeAgentResult {
    /// 自定义上下文（传递给后续操作）
    pub context: Option<Context>,
    /// 自定义响应（非空则跳过 Agent 执行）
    pub custom_response: Option<String>,
}

/// Agent 执行后回调参数
pub struct AfterAgentArgs {
    pub invocation: Invocation,
    pub full_response_event: Option<AgentStreamEvent>,
    pub error: Option<AgentError>,
}

/// Agent 执行后回调结果
pub struct AfterAgentResult {
    pub context: Option<Context>,
    pub custom_response: Option<String>,
}

/// Callbacks 配置
pub struct Callbacks {
    pub before_agent: Vec<Box<dyn BeforeAgentCallback>>,
    pub after_agent: Vec<Box<dyn AfterAgentCallback>>,
    pub continue_on_error: bool,
    pub continue_on_response: bool,
}
```

### 4.3 存储层抽象（P0）

```rust
/// 会话存储后端 trait
#[async_trait]
pub trait SessionStorage: Send + Sync + 'static {
    /// 创建会话
    async fn create_session(
        &self,
        key: &SessionKey,
        state: &SessionState,
    ) -> AgentResult<()>;

    /// 获取会话
    async fn get_session(
        &self,
        key: &SessionKey,
        options: &SessionGetOptions,
    ) -> AgentResult<Option<Session>>;

    /// 更新会话状态
    async fn update_session_state(
        &self,
        key: &SessionKey,
        state: &SessionState,
    ) -> AgentResult<()>;

    /// 追加事件
    async fn append_event(
        &self,
        key: &SessionKey,
        event: &AgentStreamEvent,
    ) -> AgentResult<()>;

    /// 删除会话
    async fn delete_session(&self, key: &SessionKey) -> AgentResult<()>;
}

// 实现示例：
// - SqliteSessionStorage（内置 SQLite，零依赖）
// - PostgresSessionStorage（生产级）
// - InMemorySessionStorage（测试用）
```

### 4.4 Agent 编排模式（P1）

```rust
/// Agent 执行策略
#[async_trait]
pub trait ExecutionStrategy: Send + Sync + 'static {
    /// 执行策略，返回事件流
    fn execute(
        &self,
        ctx: Context,
        agents: &dyn AgentRegistry,
        invocation: Invocation,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentStreamEvent> + Send>>>;
}

/// 单 Agent LLM 循环（当前实现）
pub struct LlmLoopStrategy;

/// 链式执行
pub struct ChainStrategy;

/// 并行执行
pub struct ParallelStrategy;

/// 循环执行
pub struct CycleStrategy;

/// DAG 工作流执行
pub struct GraphStrategy;
```

---

## 五、架构演进路线建议

### 阶段一：解耦与抽象（3-4 周）

1. **提取 Runner trait** — 从 `AgentRuntime::run()` 中分离执行逻辑
2. **添加 Callbacks trait** — 在 Runner 中加入拦截点
3. **定义 StorageBackend trait** — 解耦 `agent-platform` 与 SeaORM

### 阶段二：核心能力增强（2-3 周）

1. **实现多种执行策略** — Chain / Parallel / Cycle
2. **完善 Session 管理** — 三级状态、事件分页
3. **实现并行工具调用** — 多 `tool_call` 并行执行

### 阶段三：高级编排（2-3 周）

1. **Graph DAG 执行引擎** — 条件分支、并行节点
2. **Checkpoint / 恢复机制** — 断点续跑
3. **Plugin 插件系统** — 替代纯 MCP 方式

---

## 六、总结

| 方面 | tRPC-Agent-Go | nucleus-rs | 差距评估 |
|------|--------------|-----------|---------|
| 架构核心 | Runner + Agent 双层 | AgentRuntime 单层 | **大** — 缺 Runner 抽象 |
| 执行模式 | 5 种 Agent 模式 | 1 种 LLM 循环 | **大** — 缺编排模式 |
| 扩展性 | Callbacks + Plugin | 仅 MCP 外部接入 | **大** — 缺内部扩展机制 |
| 会话管理 | 三级状态 + 分页 + 摘要 | 简单透传 | **中** — 缺分层状态管理 |
| 存储灵活性 | MySQL/PG/Redis/SQLite/ClickHouse | SeaORM/MySQL 固定 | **中** — 缺存储抽象 |
| Graph 支持 | BSP + DAG 双引擎 | 无 | **中** — 缺工作流引擎 |
| 事件系统 | 完整（含 Model 事件） | 基本完整 | **小** — 缺 Model 事件 |

**核心结论**：nucleus-rs 的最大差距在于缺少 **Runner 抽象**和 **Callbacks 机制**，这两个是所有其他功能（编排模式、插件系统、Graph 引擎）的前置依赖。建议优先实现 Runner + Callbacks，然后逐步添加执行策略和存储抽象。
