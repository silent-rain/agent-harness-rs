# tRPC-Agent-Go vs nucleus-rs 能力对齐矩阵

> 分析日期：2026-04-28
> 目标：系统性梳理 tRPC-Agent-Go 的完整能力清单，与 nucleus-rs 逐一比对，明确差距与演进方向。

---

## 1. 能力全景图

### 1.1 能力总览

```
tRPC-Agent-Go 能力全景                          nucleus-rs 能力全景
─────────────────────                          ─────────────────────
✅ Agent (LLM/Chain/Parallel/Cycle/Graph)      ✅ Agent (LLM + SubAgent Delegation)
✅ Graph (GraphAgent DAG 工作流)                ❌ Graph
✅ Multi-Agent (Parallel/Cycle/Chain)          🔶 Multi-Agent (仅 SubAgent 委派)
✅ Event (事件驱动 + callback)                  🔶 Event (单向流式推送，无订阅)
✅ Runner (独立执行器抽象)                       ❌ Runner (运行循环内嵌于 AgentRuntime)
✅ Model (多 LLM 适配)                          ✅ Model (ModelAdapter trait + blanket impl)
✅ Tools (Function/MCP/DuckDuckGo)             ✅ Tools (ToolRegistry + MCP + 内置工具)
✅ Session (结构化会话管理)                      🔶 Session (session_id 字符串透传)
✅ Memory (长期 + 个性化)                        🔶 Memory (流水+精炼+向量召回，向量待完善)
✅ Callbacks (生命周期回调)                      ❌ Callbacks (无拦截点)
✅ Planner (独立推理组件)                        ✅ Planner (PlannerDecision 结构化协议)
✅ Skill                                        ✅ Skill (SkillRegistry + UseSkillTool)
❌ Artifacts                                    ❌ Artifacts
✅ Debug (调试能力)                              🔶 Debug (tracing 日志，无交互式调试)
✅ A2A (Agent-to-Agent 协议)                    ❌ A2A
✅ Ecosystem (A2A + MCP + Agent 闭环)           🔶 Ecosystem (MCP + Agent，缺 A2A)
❌ AG-UI                                        ❌ AG-UI
❌ Evaluation                                   ❌ Evaluation
✅ HTTP Service (tRPC-Go RPC)                   ✅ HTTP Service (Axum HTTP/SSE)
✅ Observability (可观测性)                      ✅ Observability (RunRecorder + ToolAuditLogger)
✅ Knowledge (RAG 知识检索)                      ✅ Knowledge (AgentKnowledgeRetriever trait)
✅ Custom-Agent (自定义 Agent)                   ✅ Custom-Agent (AgentDefinition + .md 文件)
❌ Dify                                         ❌ Dify
❌ Plugin                                       🔶 Plugin (MCP 作为外部插件通道)
❌ Team                                         ❌ Team
✅ Trpc Agent CLI                               ❌ CLI (无独立 CLI 工具)
✅ CodeExecutor (Local/Container)               🔶 CodeExecutor (RunCommandTool 仅 Local)
```

---

## 2. 能力逐项对齐

### 2.1 Agent（智能体）

| 维度 | tRPC-Agent-Go | nucleus-rs | 差距 | 改进方向 |
|------|:---:|:---:|------|----------|
| LLMAgent | ✅ | ✅ | 无 | - |
| ChainAgent（链式执行） | ✅ | ❌ | 缺少链式编排 | 引入 `ExecutionStrategy` trait |
| ParallelAgent（并行执行） | ✅ | ❌ | 缺少并行编排 | `ParallelStrategy` |
| CycleAgent（循环迭代） | ✅ | ❌ | 缺少自优化循环 | `CycleStrategy` + 收敛检查 |
| GraphAgent（图工作流） | ✅ | ❌ | 缺少 DAG 编排 | 引入 DAG 执行引擎 |
| 自定义 Agent | ✅ | ✅ | 无 | - |

**tRPC-Agent-Go 详解**：

提供 5 种 Agent 模式，覆盖从简单到复杂的全场景编排需求：

- **LLMAgent**：基于大语言模型，支持工具调用和推理（ReAct 模式）
- **ChainAgent**：链式执行，多个步骤按序执行，每步的输出作为下一步的输入
- **ParallelAgent**：并行处理，多个专家同时协作处理不同子任务，结果汇总
- **CycleAgent**：循环迭代，自我优化直到满足终止条件（收敛检查 / 最大迭代）
- **GraphAgent**：图工作流，兼容现有编排系统，基于 DAG 执行，支持条件分支和并行节点

**nucleus-rs 现状**：

仅有 LLMAgent 模式，通过 `DelegateSubAgentTool` 实现子智能体委派，但缺少链式、并行、图工作流等编排模式。

---

### 2.2 Graph（图工作流）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| DAG 执行引擎 | ✅ | ❌ |
| 条件分支 | ✅ | ❌ |
| 并行节点 | ✅ | ❌ |
| 回溯/重规划 | ✅ | ❌ |
| 状态机模式 | ✅ | ❌ |

**tRPC-Agent-Go 详解**：

`GraphAgent` 基于 DAG（有向无环图）执行引擎：

- 节点表示 Agent 或处理步骤
- 边表示数据流和依赖关系
- 支持条件分支（根据上一步输出决定下一步）
- 支持并行执行（无依赖的节点并行运行）
- 兼容现有编排系统（可对接外部工作流引擎）

**改进建议**：在 `agent-runtime/src/runtime/strategies/` 中引入 `GraphStrategy`，基于 petgraph crate 构建 DAG 执行引擎。

---

### 2.3 Multi-Agent（多智能体协作）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 链式协作 | ✅ (ChainAgent) | ❌ |
| 并行协作 | ✅ (ParallelAgent) | ❌ |
| 循环协作 | ✅ (CycleAgent) | ❌ |
| 委派协作 | ✅ | ✅ (DelegateSubAgentTool) |
| 深度控制 | ✅ | ✅ (max_delegation_depth) |

**tRPC-Agent-Go 详解**：

Multi-Agent 不是单一功能，而是通过不同 Agent 模式组合实现：

- **ChainAgent**：多专家按序处理（如：分析→设计→编码→测试）
- **ParallelAgent**：多专家同时处理（如：安全审查 + 性能审查 + 代码审查）
- **CycleAgent**：循环迭代直到收敛（如：写代码→测试→修复→再测试）
- 所有 Agent 模式都可嵌套组合

**nucleus-rs 现状**：

仅通过 `DelegateSubAgentTool` 实现父→子的单向委派，LLM 决定何时委派，但无法表达链式/并行/循环等多智能体协作模式。

---

### 2.4 Event（事件系统）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 事件发布 | ✅ | ✅ (AgentEventPublisher) |
| 事件订阅 | ✅ | ❌ |
| 事件过滤 | ✅ | ❌ |
| Callback 机制 | ✅ | ❌ |
| 事件透传（子智能体） | ✅ | ✅ (SubAgentEvent) |
| 事件总线 | ✅ | ❌ |

**tRPC-Agent-Go 详解**：

事件驱动是 tRPC-Agent-Go 的核心解耦机制：

- 组件间通过事件总线通信，而非直接调用
- 支持 callback：在关键生命周期节点插入自定义逻辑
- 支持事件订阅：组件可订阅特定类型的事件并执行自定义处理
- 支持事件过滤：按类型、来源等条件过滤事件

**nucleus-rs 现状**：

`AgentStreamEvent` 枚举定义了 16 种事件类型，`AgentEventPublisher` trait 负责单向推送。支持 `ChannelEventPublisher`（内部测试）和 `ForwardingEventPublisher`（子智能体事件透传），但缺少事件订阅、过滤和 callback 机制。

**改进建议**：引入 `EventBus` trait，支持组件订阅特定事件类型。

---

### 2.5 Runner（执行器）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 独立 Runner 概念 | ✅ | ❌ |
| 执行流程管理 | ✅ | 内嵌于 AgentRuntime |
| 串联 Session/Memory/Service | ✅ | RuntimeServices 聚合 |
| 可替换执行策略 | ✅ | ❌ |

**tRPC-Agent-Go 详解**：

`Runner` 是 Agent 的执行器，职责清晰：

- 管理 Agent 的执行流程
- 串联 Session、Memory、Service 等能力
- 支持替换执行策略（链式/并行/循环/图）
- 将"执行什么"（Agent）和"怎么执行"（Runner）解耦

**nucleus-rs 现状**：

`AgentRuntime::run()` 内嵌了 while 循环，执行逻辑与 Agent 紧耦合。`RuntimeServices` 聚合了所有可选依赖，但缺少独立的执行策略抽象。

**改进建议**：抽象 `ExecutionStrategy` trait，将 while 循环提取为 `LlmLoopStrategy`。

---

### 2.6 Model（模型适配）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 多 LLM 适配 | ✅ | ✅ |
| OpenAI | ✅ | ✅ (rig-core) |
| DeepSeek | ✅ | ✅ (rig-core) |
| 流式推理 | ✅ | ✅ |
| 函数调用 | ✅ | ✅ |
| 接口抽象 | Go interface | Rust trait (ModelAdapter) |

**tRPC-Agent-Go 详解**：

通过 Go interface 定义 Model 契约，支持 OpenAI、DeepSeek 等多种 LLM，支持流式推理和函数调用。

**nucleus-rs 现状**：

`ModelAdapter` trait + blanket impl 方案更优雅：

```rust
#[async_trait]
pub trait ModelAdapter: Send + Sync + 'static {
    async fn completion(&self, request: CompletionRequest) -> AgentResult<PlannerDecision>;
}

// blanket impl: 任何 rig::CompletionModel 自动适配
impl<M> ModelAdapter for M where M: CompletionModel + Send + Sync + 'static { ... }
```

**nucleus-rs 优势**：blanket impl 零成本抽象，编译期保证类型安全，优于 Go 的运行时接口匹配。

---

### 2.7 Tools（工具系统）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| Function Calling | ✅ | ✅ |
| MCP 协议 | ✅ (tRPC-MCP-Go) | ✅ (rmcp crate) |
| DuckDuckGo 搜索 | ✅ | ❌ |
| 内置工具 | ✅ | ✅ (ReadFile/SearchFiles/ListDir/RunCommand) |
| 工具注册 | 接口注册 | ToolRegistry (候选池+白名单) |
| 白名单策略 | ❌ | ✅ (ToolWhitelistPolicy) |
| 双源加载 | ❌ | ✅ (本地扫描+动态注入) |
| 按需加载（Skill/Command） | ❌ | ✅ (UseSkillTool/UseCommandTool) |

**nucleus-rs 优势**：

1. **白名单策略**：`ToolWhitelistPolicy` 提供精细的访问控制
2. **双源加载**：本地文件扫描 + 动态注入
3. **按需加载**：Skill/Command 的两阶段加载（摘要注入 + 按需加载完整内容）

**改进方向**：可考虑增加 DuckDuckGo / WebSearch 等搜索工具。

---

### 2.8 Session（会话管理）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 结构化会话对象 | ✅ | ❌ |
| 会话状态管理 | ✅ | ❌ |
| 会话事件历史 | ✅ | ❌ |
| 会话生命周期 | ✅ | ❌ |
| session_id | ✅ | ✅ (字符串透传) |

**tRPC-Agent-Go 详解**：

`Session` 是独立组件，管理用户会话状态和事件：

- 结构化的会话对象，包含完整的上下文信息
- 管理会话生命周期（创建、激活、暂停、关闭）
- 存储会话事件历史，支持回放
- 与 Memory、Runner 紧密配合

**nucleus-rs 现状**：

`session_id` 仅为字符串透传，无结构化会话管理。对话历史存储在数据库中，但缺少运行时的会话状态管理。

**改进建议**：引入 `SessionManager` trait，管理会话生命周期和事件历史。

---

### 2.9 Memory（记忆系统）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 长期记忆 | ✅ | 🔶 (流水记忆) |
| 个性化记忆 | ✅ | 🔶 (精炼记忆) |
| 向量召回 | ✅ | 🔶 (trait 定义，实现待完善) |
| 记忆写入 | ✅ | ✅ (AgentMemoryWriter) |
| 记忆检索 | ✅ | ✅ (AgentMemoryRetriever) |
| Noop 默认实现 | - | ✅ |

**tRPC-Agent-Go 详解**：

`Memory` 组件管理用户的长期记忆和个性化信息：

- **长期记忆**：跨会话持久化的用户偏好和知识
- **个性化记忆**：用户特定的行为模式和习惯
- 支持语义检索（向量召回）

**nucleus-rs 现状**：

记忆系统通过 trait 抽象，三层架构：

- `AgentMemoryWriter`：写入流水记忆和精炼记忆
- `AgentMemoryRetriever`：检索精炼记忆
- `AgentKnowledgeRetriever`：知识库检索（RAG）
- 向量召回的 trait 已定义，但具体实现待完善

---

### 2.10 Callbacks（生命周期回调）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 运行开始回调 | ✅ | ❌ |
| 步骤开始回调 | ✅ | ❌ |
| 工具调用前回调 | ✅ | ❌ |
| 工具调用后回调 | ✅ | ❌ |
| 运行结束回调 | ✅ | ❌ |
| 自定义逻辑插入 | ✅ | ❌ |

**tRPC-Agent-Go 详解**：

Callback 是 tRPC-Agent-Go 的核心扩展机制，允许开发者在关键生命周期节点插入自定义逻辑：

- **on_run_started**：运行开始前执行（如：初始化、权限校验）
- **on_step_started**：每步推理前执行（如：日志、性能追踪）
- **on_tool_calling**：工具调用前执行（如：权限校验、参数修改、拦截）
- **on_tool_called**：工具调用后执行（如：审计、结果后处理）
- **on_run_finished**：运行结束后执行（如：资源清理、通知）

**改进建议**：引入 `AgentLifecycleHook` trait，在 `AgentRuntime` 中添加 hooks 字段。

---

### 2.11 Planner（规划器）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 独立组件 | ✅ | 🔶 (嵌入 AgentRuntime) |
| 推理能力 | ✅ | ✅ |
| 结构化决策 | ❌ (自由文本) | ✅ (PlannerDecision 枚举) |
| 决策类型 | 通用 | CallTool/QueryKnowledge/RecallMemory/UpdatePlan/Finalize |

**tRPC-Agent-Go 详解**：

`Planner` 作为独立组件，提供 Agent 的计划和推理能力。支持自定义 Planner 实现。

**nucleus-rs 优势**：

`PlannerDecision` 枚举实现结构化调用协议，**禁止解析自由文本**，运行时可靠性更高：

```rust
pub enum PlannerDecision {
    CallTool { name, arguments },
    QueryKnowledge { kb_ids, query },
    RecallMemory { query },
    UpdatePlan { plan_description },
    Finalize { answer },
}
```

这是 nucleus-rs 的核心设计优势，应继续保持。

---

### 2.12 Skill（技能系统）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 技能注册表 | ✅ | ✅ (SkillRegistry) |
| 技能加载 | ✅ | ✅ (双源加载) |
| 按需加载 | ❌ | ✅ (UseSkillTool 两阶段) |
| 白名单策略 | ❌ | ✅ (SkillWhitelistPolicy) |
| 本地文件扫描 | ❌ | ✅ (.codebuddy/skills/) |

**nucleus-rs 优势**：

Skill 系统比 tRPC-Agent-Go 更完善：

1. **两阶段加载**：启动时注入摘要，触发时按需加载完整内容，节省 token
2. **白名单策略**：精细控制每个 Agent 可用的技能
3. **双源加载**：本地文件扫描 + 动态注入

---

### 2.13 Artifacts（产物管理）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 产物生成 | ❌ | ❌ |
| 产物存储 | ❌ | ❌ |
| 产物版本管理 | ❌ | ❌ |

两个项目均未提供结构化的产物管理能力。

**改进建议**：可考虑引入 `ArtifactsManager` trait，管理 Agent 生成的文档、代码、图表等产物，支持版本管理和回溯。

---

### 2.14 Debug（调试能力）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 日志追踪 | ✅ | ✅ (tracing) |
| 交互式调试 | ✅ | ❌ |
| 断点/单步 | ✅ | ❌ |
| 执行过程回放 | ✅ | 🔶 (事件流可回放) |

**tRPC-Agent-Go 详解**：

提供 Agent 执行过程的调试能力：

- 交互式调试：在关键节点暂停，检查状态
- 断点/单步执行：逐步观察 Agent 的推理过程
- 执行过程回放：通过事件流回放 Agent 的完整执行过程

**nucleus-rs 现状**：

通过 `tracing` 提供日志追踪，`AgentStreamEvent` 提供完整的事件流，但缺少交互式调试和断点能力。

**改进建议**：可利用 `AgentStreamEvent` 事件流实现执行回放调试器。

---

### 2.15 A2A（Agent-to-Agent 协议）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| A2A 协议实现 | ✅ (tRPC-A2A-Go) | ❌ |
| Agent 发现 | ✅ | ❌ |
| Agent 通信 | ✅ | ❌ |
| 跨服务协作 | ✅ | ❌ |

**tRPC-Agent-Go 详解**：

tRPC-A2A-Go 是独立的开源组件，提供：

- **Agent 发现**：注册和发现远程 Agent 服务
- **Agent 通信**：标准化的 Agent-to-Agent 通信协议
- **跨服务协作**：不同服务中的 Agent 可以互相协作
- 与 tRPC-Agent-Go 无缝集成

**改进建议**：在 `agent-runtime` 中预留 `AgentCommunicationProtocol` trait，为后续 A2A 集成做准备。

---

### 2.16 Ecosystem（生态）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| A2A 框架 | ✅ (tRPC-A2A-Go) | ❌ |
| MCP 框架 | ✅ (tRPC-MCP-Go) | ✅ (rmcp) |
| Agent 框架 | ✅ (tRPC-Agent-Go) | ✅ (agent-runtime) |
| Python 版本 | ✅ (tRPC-Agent-Python) | ❌ |
| tRPC 生态闭环 | ✅ | ❌ (Rust crate 模块化) |

**tRPC-Agent-Go 详解**：

tRPC 提供完整的 AI 生态闭环：

- **tRPC-A2A-Go**：Agent-to-Agent 通信框架
- **tRPC-MCP-Go**：MCP 协议实现
- **tRPC-Agent-Go**：Agent 开发框架
- **tRPC-Agent-Python**：Python 版本 Agent 框架
- 所有组件基于 tRPC 微服务架构，天然支持 RPC 通信

**nucleus-rs 策略**：

采用 Rust crate 模块化策略，每个组件独立发布，使用者按需组合。这是 Rust 生态的惯用模式，也是优势。

---

### 2.17 AG-UI（Agent-UI 协议）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| AG-UI 协议 | ❌ | ❌ |

两个项目均未实现 AG-UI（Agent-User Interaction Protocol）。

**改进建议**：可考虑在 `AgentStreamEvent` 基础上扩展 AG-UI 兼容层，为前端交互提供标准化协议。

---

### 2.18 Evaluation（评估）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| Agent 评估框架 | ❌ | ❌ |
| 质量指标 | ❌ | ❌ |
| 自动化测试 | ❌ | ❌ |

两个项目均未提供 Agent 评估能力。

**改进建议**：引入 `AgentEvaluator` trait，评估 Agent 的回答质量、工具调用准确率、任务完成率等指标。

---

### 2.19 HTTP Service（HTTP 服务）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| RPC 框架 | ✅ (tRPC-Go) | ❌ |
| HTTP 服务 | ✅ | ✅ (Axum) |
| SSE 流式推送 | ✅ | ✅ |
| gRPC 支持 | ✅ | ❌ |

**tRPC-Agent-Go 详解**：

基于 tRPC-Go 框架，天然支持高性能 RPC 通信，同时支持 HTTP 和 gRPC 协议。

**nucleus-rs 现状**：

基于 Axum 提供 HTTP 服务，通过 SSE 实现流式推送。无 gRPC 支持。

---

### 2.20 Observability（可观测性）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 运行记录 | ✅ | ✅ (AgentRunRecorder) |
| 工具审计 | ✅ | ✅ (AgentToolAuditLogger) |
| Noop 默认实现 | - | ✅ |
| 追踪/指标 | ✅ (tRPC filter) | 🔶 (tracing) |

**nucleus-rs 优势**：

可观测性通过 trait 抽象，与业务逻辑解耦：

- `AgentRunRecorder`：记录运行开始/结束/步骤
- `AgentToolAuditLogger`：记录工具调用的详细审计日志
- `NoopRunRecorder` / `NoopToolAuditLogger`：默认空实现，测试友好

---

### 2.21 Knowledge（知识检索）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| RAG 检索 | ✅ | ✅ |
| 知识库管理 | ✅ | ✅ (AgentKnowledgeRetriever) |
| 多知识库 | ✅ | ✅ (kb_ids) |
| Noop 默认实现 | - | ✅ |

两个项目的知识检索能力基本对齐，nucleus-rs 通过 trait 抽象更灵活。

---

### 2.22 Custom-Agent（自定义智能体）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 自定义 Agent | ✅ | ✅ |
| 配置化定义 | ✅ | ✅ (AgentDefinition + .md) |
| 本地文件扫描 | ❌ | ✅ (.codebuddy/agents/) |
| 动态注入 | ✅ | ✅ (数据库/HTTP) |

**nucleus-rs 优势**：

自定义 Agent 支持更丰富：

1. **.md 文件定义**：通过 frontmatter 声明能力，body 作为 system_prompt
2. **双源加载**：本地文件扫描 + 动态注入
3. **独立能力池**：每个子智能体有独立的工具/规则/技能白名单

---

### 2.23 Dify（第三方集成）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| Dify 集成 | ❌ | ❌ |

两个项目均未集成 Dify。

---

### 2.24 Plugin（插件系统）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| 插件系统 | ❌ | 🔶 (MCP 作为外部插件通道) |

**nucleus-rs 现状**：

虽然无独立的插件系统，但 MCP 协议天然充当了外部插件通道：

- MCP 服务可以动态连接/断开
- 每个 Agent 有独立的 MCP 白名单
- MCP 内省工具自动注册到 ToolSet

**改进建议**：可考虑在 MCP 基础上抽象更通用的 `PluginManager` trait。

---

### 2.25 Team（团队协作）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| Team 概念 | ❌ | ❌ |

两个项目均未提供 Team 概念（多个 Agent 组成固定团队，角色分工协作）。

**改进建议**：可在 Multi-Agent 编排的基础上引入 Team 抽象，定义团队成员角色和协作规则。

---

### 2.26 Trpc Agent CLI（命令行工具）

| 维度 | tRPC-Agent-Go | nucleus-rs |
|------|:---:|:---:|
| CLI 工具 | ✅ | ❌ |
| 脚手架生成 | ✅ | ❌ |
| Agent 调试 | ✅ | ❌ |

**tRPC-Agent-Go 详解**：

提供独立的 CLI 工具：

- 脚手架生成：快速创建 Agent 项目
- Agent 调试：交互式调试 Agent 行为
- 配置管理：管理 Agent 配置文件

**改进建议**：可考虑提供 nucleus-cli 工具，支持 Agent 项目脚手架生成和本地调试。

---

## 3. 能力矩阵汇总

### 3.1 状态标记说明

| 标记 | 含义 |
|:---:|------|
| ✅ | 已实现且功能完整 |
| 🔶 | 部分实现或有基础框架但需增强 |
| ❌ | 未实现 |

### 3.2 完整能力矩阵

| # | 能力 | tRPC-Agent-Go | nucleus-rs | 差距等级 | 建议优先级 |
|---|------|:---:|:---:|:---:|:---:|
| 1 | Agent (LLM) | ✅ | ✅ | - | - |
| 2 | Agent (Chain) | ✅ | ❌ | 高 | P1 |
| 3 | Agent (Parallel) | ✅ | ❌ | 高 | P1 |
| 4 | Agent (Cycle) | ✅ | ❌ | 中 | P2 |
| 5 | Agent (Graph) | ✅ | ❌ | 高 | P1 |
| 6 | Multi-Agent | ✅ | 🔶 | 中 | P1 |
| 7 | Event | ✅ | 🔶 | 中 | P2 |
| 8 | Runner | ✅ | ❌ | 高 | P0 |
| 9 | Model | ✅ | ✅ | - | - |
| 10 | Tools | ✅ | ✅ | - | - |
| 11 | Session | ✅ | 🔶 | 中 | P1 |
| 12 | Memory | ✅ | 🔶 | 低 | P2 |
| 13 | Callbacks | ✅ | ❌ | 高 | P0 |
| 14 | Planner | ✅ | ✅ | - | - |
| 15 | Skill | 🔶 | ✅ | nucleus 优 | - |
| 16 | Artifacts | ❌ | ❌ | - | P3 |
| 17 | Debug | ✅ | 🔶 | 低 | P3 |
| 18 | A2A | ✅ | ❌ | 高 | P2 |
| 19 | Ecosystem | ✅ | 🔶 | 中 | P2 |
| 20 | AG-UI | ❌ | ❌ | - | P3 |
| 21 | Evaluation | ❌ | ❌ | - | P3 |
| 22 | HTTP Service | ✅ | ✅ | - | - |
| 23 | Observability | ✅ | ✅ | - | - |
| 24 | Knowledge | ✅ | ✅ | - | - |
| 25 | Custom-Agent | ✅ | ✅ | - | - |
| 26 | Dify | ❌ | ❌ | - | P3 |
| 27 | Plugin | ❌ | 🔶 | - | P2 |
| 28 | Team | ❌ | ❌ | - | P3 |
| 29 | CLI | ✅ | ❌ | 中 | P2 |
| 30 | CodeExecutor | ✅ | 🔶 | 中 | P1 |

### 3.3 差距统计

```
nucleus-rs 已对齐（✅ vs ✅）        ：8 项  →  Agent(LLM), Model, Tools, Planner, Skill, HTTP, Observability, Knowledge, Custom-Agent
nucleus-rs 部分对齐（✅ vs 🔶）      ：7 项  →  Multi-Agent, Event, Session, Memory, Debug, Ecosystem, Plugin
nucleus-rs 未对齐（✅ vs ❌）         ：9 项  →  Agent(Chain/Parallel/Cycle/Graph), Runner, Callbacks, A2A, CLI, CodeExecutor(容器)
双方均未实现                          ：5 项  →  Artifacts, AG-UI, Evaluation, Dify, Team
nucleus-rs 独有优势                  ：3 项  →  Skill(两阶段+白名单), Tools(白名单+双源), Planner(结构化协议)
```

---

## 4. 优先级排序改进路线

### P0 - 核心差距（影响架构演进方向）

| # | 能力 | 改进内容 | 预计工时 |
|---|------|----------|----------|
| 1 | Runner | 抽象 `ExecutionStrategy` trait，将 while 循环提取为 `LlmLoopStrategy` | 3-5d |
| 2 | Callbacks | 引入 `AgentLifecycleHook` trait，在关键生命周期节点添加钩子 | 3-5d |

### P1 - 重要差距（影响核心功能完整性）

| # | 能力 | 改进内容 | 预计工时 |
|---|------|----------|----------|
| 3 | Agent (Chain) | 实现 `ChainStrategy`，支持多步骤按序执行 | 3-5d |
| 4 | Agent (Parallel) | 实现 `ParallelStrategy`，支持多专家并行协作 | 3-5d |
| 5 | Agent (Graph) | 引入 DAG 执行引擎，支持条件分支和并行节点 | 5-8d |
| 6 | Session | 引入 `SessionManager` trait，管理会话生命周期 | 3-5d |
| 7 | CodeExecutor | `RunCommandTool` 增加容器化执行模式 | 3-5d |
| 8 | Multi-Agent | 基于新 Agent 模式实现多种多智能体协作 | 随 Agent 模式实现 |

### P2 - 增强差距（提升竞争力）

| # | 能力 | 改进内容 | 预计工时 |
|---|------|----------|----------|
| 9 | Event | 引入 `EventBus` trait，支持事件订阅/过滤 | 3-5d |
| 10 | Memory | 完善向量召回实现 | 5-8d |
| 11 | A2A | 预留 `AgentCommunicationProtocol` trait | 2-3d |
| 12 | Ecosystem | 对接 A2A/MCP 生态组件 | 随 A2A 实现 |
| 13 | Plugin | 抽象 `PluginManager` trait | 3-5d |
| 14 | CLI | 提供 nucleus-cli 脚手架工具 | 5-8d |

### P3 - 远期规划

| # | 能力 | 改进内容 |
|---|------|----------|
| 15 | Artifacts | 引入 `ArtifactsManager` trait |
| 16 | Debug | 基于 AgentStreamEvent 实现交互式调试器 |
| 17 | AG-UI | 在 AgentStreamEvent 基础上扩展 AG-UI 兼容层 |
| 18 | Evaluation | 引入 `AgentEvaluator` trait |
| 19 | Dify | 集成 Dify 平台 API |
| 20 | Team | 引入 Team 抽象，定义团队成员角色 |

---

## 5. nucleus-rs 独有优势（tRPC-Agent-Go 不具备）

| 优势 | 说明 |
|------|------|
| **结构化调用协议** | `PlannerDecision` 枚举禁止解析自由文本，运行时可靠性更高 |
| **Skill 两阶段加载** | 启动时注入摘要，触发时按需加载，节省 token |
| **白名单策略** | ToolWhitelistPolicy / SkillWhitelistPolicy 提供精细访问控制 |
| **双源加载** | 本地文件扫描 + 动态注入，灵活适配多种配置来源 |
| **Command 系统** | 用户/宿主显式触发的指令式交互能力，tRPC 无此概念 |
| **Noop 默认实现** | 所有可选依赖均有 Noop 实现，测试友好 |
| **Blanket impl** | ModelAdapter 的 blanket impl 零成本抽象，优于 Go 接口匹配 |
| **Builder 双层 API** | 底层注入 + 便捷配置，灵活性与便捷性兼顾 |

这些优势应继续保持和强化。
