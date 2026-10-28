# 以 rig-agent 为核心的可观测智能体改造方案

> **文档版本**：v2（2026-10-28 修订：编排能力 `ChainAgent` / `ParallelAgent` 由「死代码」改判为「未完成能力，保留补完」；新增 §2.5、§3.3–§3.5、§5.4，新增里程碑 M5）
> **创建日期**：2026-10-28
> **适用仓库**：`agent-harness-rs`
> **目标读者**：运行时（`agent-runtime`）、装配（`agent-assembly`）、服务（`services/agent-service`）维护者
> **涉及外部依赖**：`rig-agent 0.45` / `rig-core 0.45` / `rig-reqwest 0.45`

---

## 一、目标与范围

### 1.1 目标定稿

> 以 **rig-agent** 承担"决策 + 执行 + 会话 transcript"，harness 只保留三件事：
> **装配**（DB → `AgentBundle` → rig `Agent`）、**观测**（`stream()` + `AgentHook` 双源 → `AgentStreamEvent` → 落库/出口）、**准入**（工具白名单）。
> skill / mcp / sub_agent / a2a / builtin 全部坍缩为同一种东西 —— `DynamicTool`。

一句话：**框架只管跑，harness 只管看和放行。**

### 1.2 成功判据

系统应能对以下三个问题给出唯一答案，且无需翻 transcript：

1. 某次运行第 N 步调了哪个工具、参数、耗时、结果 —— 一条 SQL 从 `t_agent_session_event` 查出；
2. 某 skill / 某 MCP 工具被哪个 agent 调用过多少次、成功率 —— 从指标查出；
3. 某次运行为何停在 max_turns —— `RunFinished.error_message` + `turn_count` 指标 + trace span 三处可对齐。

### 1.3 非目标

- 不改动入站 OpenAI 兼容协议（`openai/*_stream_adapter.rs`）；
- 不改动 `agent-assembly` 的 DAO / Loader / `DbSessionService`；
- 不改动前端可见的 SSE 事件名与字段（M4 之前）；
- 不引入新的数据库表（除 M4 若需事件历史兼容读的迁移）。

---

## 二、现状问题清单

### 2.1 P0：死代码（零调用方 / 零读取），可直接删

| 编号 | 目标 | 证据 | 动作 |
| --- | --- | --- | --- |
| D-1 | 原「`ChainAgent` / `ParallelAgent` 为死代码」判断**已撤销** | 二者是未完成的编排能力，非废弃 | **保留并补完**，见 §2.5 / §3.3 |
| D-2 | `model/mock.rs`（219 行，`MockModel` / `MockBehavior`） | 仅 `model/mod.rs:20` 导出 + README 提及 | 删除；需要 mock 用 `rig-agent/test-utils` |
| D-3 | `dto::agent_run::AgentRunResp` | 仅 `services/agent-service/src/dto/agent_run.rs:61` 定义，从未构造 | 删除 |
| D-4 | `AgentRunReq.debug` 字段 | controller / service 全仓无读取 | 删除字段 |
| D-5 | `ConversationMessage.metadata` | 除 `message.rs:83-85` 与单测外零使用 | 删除字段与 `with_metadata` |
| D-6 | `openai::AdapterOutput::Extension` 通道 | 消费侧 `runtime/agent/traits.rs:232` 是 `_ => {}`，分支被静默丢弃 | 删除 Extension 与 `NUCLEUS_EVENT_PREFIX` |

### 2.2 P1：双真源（语义重复，导致 bug / 写放大）

| 编号 | 问题 | 位置 | 处理 |
| --- | --- | --- | --- |
| R-1 | 事件类型映射实现 3 份 | `session/event.rs:195` / `dto/agent_run.rs:87` / `model_caller.rs:372` | 统一走 `AgentStreamEvent::event_type()`，删另两处 |
| R-2 | 最终答案双份投递（`TextDelta` 仅末轮 + `Answer` 重发整段） | `session/event.rs:146` 注释、`llm_agent.rs:725` | 迁移后 M4 定稿：`TextDelta` 全轮透出，`Answer` 降级为终止标记 |
| R-3 | `Reasoning`（整段）/ `ReasoningDelta` 双形态 + `reasoning_emitted_full` 去重集合 | `model_caller.rs:616-634` | M4 删除 `Reasoning`，需整段由消费方聚合 |
| R-4 | 单次工具调用落 3 行事件（`ToolCallsPlanned` / `ToolCallStarted` / `ToolCallFinished`） | `event.rs:85`、`model_caller.rs:651`、`tool_executor.rs:339` | `ToolCallsPlanned` 降级为仅推流（或删），批信息由 `step` 表达 |

### 2.3 P2：设计缺陷（行为不符合语义）

| 编号 | 问题 | 证据 | 处理 |
| --- | --- | --- | --- |
| F-1 | `AgentBundle.tool_configs` 在顶层 `build()` 被完全忽略，DB 配的工具白名单对主智能体无效 | `builder.rs:454-480` 无 `load_tools`；仅 `delegate_sub_agent.rs` / `a2a/card.rs` 读取 | `build()` 末尾补 `allow_tools(bundle.tool_names())` |
| F-2 | `all_builtin_tools()` 无条件注册 `run_command` + `duckduckgo_search`，`agent_run` 不设白名单 → 所有 agent 天生具备 shell 与出网 | `tool/simple_tools.rs:26-34`、`service/agent_run.rs:250,275` | 默认关闭，按 `tool_configs` 启用；`run_command` 标记高危 |
| F-3 | `knowledge_query` 对每个 agent 注册，但 `knowledge` 默认 `NoopKnowledgeRetriever` → 暴露永远为空的工具 | `builder.rs:513-517`、`runtime/services.rs:67` | 仅当 retriever 非 Noop 时注册；或删除 knowledge 子系统 |
| F-4 | `RuntimeServices::noop()` 静默降级：忘配 session 服务时事件全部进内存、无告警 | `runtime/services.rs:64-69` | 首次写入 `warn_once`；`build()` 时若为默认实现给提示 |
| F-5 | 错误类型不统一：同 crate 内 `anyhow::Result`（118 处）与 `err_code::Error` 并存 | `service/agent_run.rs` vs `service/tool_registry.rs` | 收敛到 `err_code::Error`，为 `agent_runtime::Error` 实现 `From` |
| F-6 | `AgentRunService` 6 个方法 ≈ 同 2 段代码复制 6 次，`build_agent` / `build_agent_from_config` 重复构造服务 | `service/agent_run.rs:50-280` | 收敛为 `RunSource { Db | Inline }` + `Surface { OpenAI | Debug }` + 单一 `execute` |
| F-7 | SSE 组装样板重复 4 次 | `service/agent_run.rs` | 抽 `sse_chunks()` / `sse_events()` |
| F-8 | 每次请求都全量重装配：8 条装配查询 ×（1 + 子 agent 数）+ 每请求 N 次 MCP 握手 | `agent_run.rs:237-255`、`source.rs:188-197`、`builder.rs:607-625` | 三级缓存 L0/L1/L2，详见 §12.5「P2-6 详细设计」 |
| F-9 | `RunOutcome` 缺 `run_id` / `session_id`，前端只能自行从流里捞 | `session/outcome.rs:39-46` | `from_stream` 从 `RunStarted` 补齐 |

### 2.4 待评估（先加指标再决定）

- `json_repair.rs`（287 行手写修复）：迁移后以 `DynamicTool` 装饰器形态保留，先统计命中率；若 < 1%，退化为 30 行的常见修复。
- `MessageContext`（`context/message_context.rs`，278 行）：迁移后 `to_rig_messages` 职责上移，仅剩 push/extend；建议降级为 `Vec<ConversationMessage>`，等真正做上下文压缩再抽。

### 2.5 未完成能力：智能体编排（Chain / Parallel）

**定性：不是死代码，而是只完成了最底层语义、尚未接入运行时的雏形。**

已具备的能力：

- `ChainAgent`（`chain_agent.rs`）：串行串联，上游 `final_answer` 作为下游 `user_input`，任一失败即中断，`total_steps` 累加；
- `ParallelAgent`（`parallel_agent.rs`）：`join_all` 并发 + `Semaphore` 限流（`with_max_concurrency`），结果按定义顺序以 `\n---\n` 拼接，支持部分失败（`RunOutcome::failure`）。

缺口（按严重度排序）：

| 编号 | 缺口 | 证据 | 后果 |
| --- | --- | --- | --- |
| O-1 | 未实现 `run_stream` / `infer_stream`，落到 trait 默认实现直接返回 `Err` | `traits.rs:123-128`；两个 agent 仅 `impl Agent::run` | `AgentOpenAI::run_stream` 与 debug 流式端点对编排体**直接失败** |
| O-2 | 文档承诺的兜底类型 `AgentExecutor` 从未存在 | `traits.rs:112,125,155` 三处引用，全仓无定义 | 报错文案把用户指向不存在的类型，流式无法补救 |
| O-3 | 子 run 事件被 `RunOutcome::from_stream` 吞掉，编排层完全不可观测 | `session/outcome.rs:95` | 与"可观测智能体"目标直接冲突：编排时看不到子步骤/工具事件 |
| O-4 | 子 run 共用父 `session_id` 且都按 primary 处理，`ensure_session_running` 遇 `Running` 直接报错 | `llm_agent.rs:480-513`、`parallel_agent.rs:140` | **并行编排会互相踩**：首个分支把 session 置 `Running`，其余并发分支大概率失败 |
| O-5 | 并发上限语义危险：`max_concurrency = 0` 表示不限 | `parallel_agent.rs:57,77-81` | 分支数 × 工具并发 × token 无上界，易打爆上游限流 |
| O-6 | 无编排级事件，只有工具委派用的 `SubAgentEvent`（`DelegateType` 仅 `Agent`/`SubAgent`/`A2aAgent`） | `session/event.rs:126-139` | 前端无法区分"委派"与"链/并行"，看不到分支进度 |
| O-7 | `RunOutcome` 无 `run_id` / `session_id`（F-9） | `session/outcome.rs:39-46` | 编排链路无法回溯每步归属 |

**结论：编排能力保留并补完**（见 §3.3 定位、§九 M5 里程碑）。它与工具委派共用同一套"子 run 不写 session 状态"机制，不引入第二套执行模型。

---

## 三、目标架构

### 3.1 三层 + 一条观测总线

```text
┌───────────────────────────────────────────────────────────────┐
│ 装配层  agent-assembly                                        │
│   DB → AgentBundle → rig_agent::AgentBuilder                  │
│   职责：加载 rule/skill/command/mcp/sub_agent/a2a、去重、      │
│         委派深度截断、工具准入                                  │
│   产出：rig_agent::Agent（不可变快照，可缓存）                  │
├───────────────────────────────────────────────────────────────┤
│ 运行层  rig-agent 0.45                                        │
│   AgentRunner：max_turns / tool_concurrency / history /        │
│                preamble / temperature / tool_context           │
│   工具：dynamic_tools(Vec<DynamicTool>)  ← ToolPool 原样喂入    │
│   钩子：AgentHook（9 个观测/干预点）                            │
│   产出：(MultiTurnStreamItem 流, AgentHook 回调)               │
├───────────────────────────────────────────────────────────────┤
│ 观测层  agent-observer（新，替代 llm_agent.rs 的壳）            │
│   双源合并：stream（结果）+ hook（过程）→ AgentStreamEvent      │
│   → SessionService 落库（低频双写 / 高频只推流）                │
│   → SSE 出口 / OpenAI 兼容出口 / trace 关联                    │
└───────────────────────────────────────────────────────────────┘
```

**关键点**：`rig_agent::Agent` 是不可变快照，适合按 `agent_id` 缓存；装配成本（尤其 MCP 连接）必须从请求路径上移走。

### 3.2 改造后目录形态（`crates/agent-runtime/src`）

```text
runtime/
  agent/
    mod.rs            # 导出 Agent / AgentOpenAI / AgentSource / HarnessAgent
    traits.rs         # 保留（Agent、AgentOpenAI、AgentSource）
    builder.rs        # 保留装配逻辑，build() 产出 HarnessAgent
    local_source.rs   # 保留
    harness_agent.rs  # 新增：run 生命周期 + session 状态 + 出口适配
  observer/
    mod.rs            # 新增：观测层入口
    stream_map.rs     # 新增：MultiTurnStreamItem → AgentStreamEvent
    hooks.rs          # 新增：AgentHook → AgentStreamEvent（工具/模型钩子）
    persist.rs        # 新增：双写策略与 transcript 落库
  orchestration/
    mod.rs            # 新增：编排抽象出口（Chain / Parallel / Executor）
    chain.rs          # 改写：补 run_stream + 步骤契约（由 agent/chain_agent.rs 迁入）
    parallel.rs       # 改写：补 run_stream + 失败策略 + 显式并发上限（由 agent/parallel_agent.rs 迁入）
    executor.rs       # 新增：补齐 traits.rs 引用的 AgentExecutor（给组合体提供流式出口）
  llm/
    json_repair.rs    # 保留（下移为工具装饰器使用）
tool/
  pool.rs             # 保留 + 新增 dynamic_tools()
  retry.rs            # 新增：DynamicTool 重试/修复装饰器
```

删除：`runtime/agent/llm_agent.rs`、`runtime/llm/{model_caller,tool_executor,types}.rs`、`model/{adapter,mock}.rs`、`model/streaming/*`。

迁移（不删除）：`runtime/agent/chain_agent.rs` → `runtime/orchestration/chain.rs`，`runtime/agent/parallel_agent.rs` → `runtime/orchestration/parallel.rs`。

### 3.3 编排层的定位：位于观测层之上、rig-agent 叶子之下

```text
              ┌────────────────────────────────────┐
              │ 编排层  orchestration                │
              │  ChainAgent（串行） / ParallelAgent  │
              │  （并发） / AgentExecutor（流式出口）  │
              │  职责：调度、失败策略、token 预算、     │
              │        编排级事件、session 归属         │
              └──────────────┬─────────────────────┘
                             │ 每个叶子 = 一个 rig-agent run
              ┌──────────────▼─────────────────────┐
              │ 观测层（§四）→ HarnessAgent 单 run    │
              └────────────────────────────────────┘
```

三条设计原则：

1. **叶子唯一**：`rig_agent::Agent` 只承担单 run；任何"多 run"语义（串行/并发/委派）一律在编排层表达，禁止在 rig-agent 内模拟。
2. **session 归属编排层**：编排体是 session 的唯一 owner；所有子 run 一律 `is_primary = false`（不创建/不更新 session 状态），这同时修掉 O-4 的并行冲突。
3. **事件不吞**：每个子 run 的观测层输出必须被编排层"包一层后透传"，不得降级为 `RunOutcome`。包法见 §3.4。

### 3.4 编排级事件（必须在 M5 前定稿）

复用 `SubAgentEvent` 的包装思路，但扩展 `DelegateType`：

| 变体 | 语义 | 关键字段 |
| --- | --- | --- |
| `ChainStep` | 串行链中的第 N 步 | `step_index`、`step_name`、`run_id`（子 run）、`status` |
| `ParallelBranch` | 并发分支 | `branch_index`、`branch_name`、`run_id`、`status` |
| `Delegate`（原 `SubAgent`） | 工具委派 | 保持不变 |

要求：`parent_run_id` 由编排层生成（一次编排一个），子 run 的 `run_id` 作为 `child_run_id` 下挂，保证"1 次编排 → N 次运行"可回溯；`total_steps` 口径改为"所有子 run 的 `CompletionCall` 次数之和"。

### 3.5 编排层的失败策略与预算

```rust
enum StepFailure { FailFast, ContinueOnError, Fallback(Arc<dyn Agent>) }
enum ConcurrencyMode { Unlimited, Limited(usize) }   // 取消 0 = 不限 的隐式语义
struct OrchestrationBudget { max_total_turns: u32, max_total_tokens: Option<u64> }
```

- `ChainAgent`：默认 `FailFast`；`ContinueOnError` 时把失败步的 `error_message` 注入下一步输入前缀。
- `ParallelAgent`：默认 `Limited(4)`；`Unlimited` 必须显式选择并在指标中标记。
- 预算超限的终止原因统一映射为 `FinishReason::MaxTurns` 的编排级变体或新增 `FinishReason::BudgetExceeded`（M5 定稿）。

---

## 四、可观测性契约

### 4.1 事件权威源（唯一真源表）

| 事件 | 唯一权威源 | 落库 | 说明 |
| --- | --- | --- | --- |
| `RunStarted` / `UserMessage` / `RunFinished` | 观测层（外层编排） | 双写 | rig-agent 不管会话生命周期 |
| `TextDelta` / `ReasoningDelta` | `stream()` 的 `StreamAssistantItem` | 只推流 | 全轮透出（不再仅末轮） |
| `Answer` | `stream()` 的 `FinalResponse` | 双写 | 幂等镜像，语义为终止标记 |
| `StepStarted` | `CompletionCall` 计数或 `HookContext::turn()` | 双写 | 与 `max_turns` 口径对齐 |
| `ToolCallStarted`（args / source / 起点时刻） | `AgentHook::on_dispatch` | 双写 | 流中没有实时起点 |
| `ToolCallFinished`（result / error / duration） | `AgentHook::on_outcome` | 双写 | 流中 `ToolResult` 是批量落地后投影，abort 会整批消失 |
| `ToolCallsPlanned` | `on_dispatch` 按 `turn` 聚合 | 只推流 | 批信息由 `step` 表达 |
| `Reasoning`（整段） | 无（M4 删除） | — | 需要整段由消费方聚合 |
| `SubAgentEvent` | 子 run 的观测层输出，包一层 | 双写 | 子 run 不写 session 状态 |

### 4.2 三条硬约束（必须写入代码注释）

1. **工具事件不允许从流里取。** rig-agent 的 `ToolResult` 明确"仅在整个批次成功落地后按调用顺序透出；批次中途终止不产生成功结果"，实时观测必须走 tool hooks。
2. **token 级事件不落库**（`TextDelta` / `ReasoningDelta`），否则事件表写放大不可控。
3. **不自建 trace。** rig-agent 内置 `invoke_agent` / `execute_tool` / `gen_ai.*` span，直接接 OTel exporter；事件表只加 `run_id + turn + call_id` 三元关联，供前端从 span 跳回事件行。

### 4.3 指标清单

`turn_count`、`max_turns_hit_rate`、`tool_success_rate{tool_name,source}`、`tool_duration_ms`、`invalid_tool_call_rate`、`hook_deny_rate`、`tokens{input,output,cached}`、`run_duration_ms{finish_reason}`。

落点：观测层单开 `metrics` 门面，不塞进 `RuntimeServices`。

---

## 五、工具能力建设

### 5.1 一条注册路径，一个准入门

所有工具来源统一为 `DynamicTool` + `ToolSource` 标签。`ToolSource` 仅服务 UI 卡片样式，**禁止参与任何逻辑分支**。

| 来源 | 装配入口 | 现有资产 | 迁移动作 |
| --- | --- | --- | --- |
| builtin | `all_builtin_tools()` | read/list/search/run_command/duckduckgo | 默认关闭；按配置启用；`run_command` 高危分级 |
| skill | `SkillLoadTool` / `SkillListDocsTool` / `SkillSelectDocsTool` / `SkillExecTool` | local + DB store | 保留；超时、输出截断、沙箱收敛到 `SkillExecTool` 单点 |
| mcp | `McpRegistry::connect_all()` → `Vec<DynamicTool>` | rmcp（SSE / HTTP / stdio） | registry 常驻 + 健康检查，`build()` 不再每次连接 |
| sub_agent | `FlatSubAgentTool` / `DelegateSubAgentTool` | bundle 递归 | 子 run 走同一观测层，`is_primary=false` |
| a2a | `A2aRegistry` → `DynamicTool` | a2a-rs | 远端事件包 `SubAgentEvent` |

### 5.2 准入规则（唯一权威 = `tool_configs`）

```text
未配置      → 仅安全内建（read_file / list_dir / search_files）+ 已装配的 skill / mcp
已配置      → 严格白名单
高危工具    → run_command 等需显式出现在白名单
子 Agent    → 父白名单 ∩ 自身配置
```

### 5.3 skill / mcp 建设要点

- **skill**：manifest + store（local / DB）+ 4 个内省工具。执行侧必须单点收敛超时、输出截断（建议 64KB）、工作目录隔离；否则模型一次 `skill_exec` 就能把上下文打满。
- **mcp**：`McpRegistry` 常驻，按 `server_name` 维护连接与健康状态；工具列表在 agent 装配时读取一次快照，服务端工具变更通过运维接口触发 registry 刷新 + `AgentCache` 失效。
- **工具描述质量**：`DynamicTool` 的 `description()` 直接进入 LLM 上下文，skill / mcp 的描述需在装配层做长度上限与去噪（当前没有约束）。

---

### 5.4 工具并发：现状已支持，缺口在护栏

**已具备**（`ModelCaller::execute_tool_calls`，`model_caller.rs:692-756`）：

- `Semaphore::new(max_parallel_tools)`（默认 8，`constant.rs:47`）+ `join_all` 并发执行同一批工具；
- 单工具失败不冒泡：以 `ToolCallFinished { success: false }` + 错误文本回灌，模型下一轮自我修正；
- 结果按**调用顺序**（非完成顺序）合并回上下文，符合 OpenAI 消息规范。

**缺口**：

| 编号 | 缺口 | 证据 | 建议 |
| --- | --- | --- | --- |
| T-1 | 无 per-tool 超时（仅 `run_command` 自带 30s），`join_all` 必须等全部完成 | `tool/tools/run_command.rs:82`，其余工具无 timeout | 新增 `tool/guard.rs`（超时 + 输出截断），与 `tool/retry.rs` 同层，作为 `DynamicTool` 装饰器 |
| T-2 | 并发工具事件被批次序列化：`duration_ms` 真实，但 `ToolCallFinished` 全部在批次结束后按调用序发出 | `model_caller.rs:736-753` | rig-agent 的 `on_outcome` 是完成即回调，天然真实；定稿为"完成即发 + `step` 归属"，回放按 `step` 重排 |
| T-3 | 并发预算仅在单 run 单批次内生效；编排并行 × 工具并发无全局上界 | `run_config.max_parallel_tools`；`parallel_agent.rs:57`（0 = 不限） | per-root-run `ConcurrencyBudget`：一个 `Semaphore` 由编排层与所有子 run 共享 |
| T-4 | 无同批依赖（DAG）表达 | 仅全并行或全串行 | 暂不做；依赖关系用"模型多轮"表达，并在文档标注避免误用 |

**迁移映射**：`RunConfig.max_parallel_tools` → `AgentRunner::tool_concurrency`；`execute_tool_calls` 整体删除，信号量语义上移为 `ConcurrencyBudget`。

## 六、rig-agent 能力映射与 API 速查

### 6.1 能力映射

| 现有能力 | 现有实现 | rig-agent 对应 | 结论 |
| --- | --- | --- | --- |
| ReAct 多轮循环 | `llm_agent.rs:1153` `drive_react_loop` | `agent.prompt(msg).history(h).max_turns(n).stream()` | 直接替换 |
| 最大轮次 | `RunConfig.max_turns` | `AgentRunner::max_turns` / `default_max_turns` | 直接替换（口径见风险 R-2） |
| 工具并发 | `max_parallel_tools` + 信号量 | `AgentRunner::tool_concurrency` | 直接替换 |
| 工具注册 | `ToolPool`（`HashMap<String, Arc<DynamicTool>>`） | `AgentBuilder::dynamic_tools(Vec<DynamicTool>)` | 1:1 复用，`ToolPool` 加访问器 |
| 工具上下文 | `tool/rig_bridge.rs:160` `call_context` | `AgentRunner::tool_context(ctx)`，每次派发 clone | 同一 `rig_core::tool::ToolContext`，scope 为 `Arc`，可携带事件通道 |
| 文本 / 推理增量 | `StreamChunk::{TextDelta,ReasoningDelta}` | `StreamAssistantItem(Item::Event(StreamEvent::{Text,Reasoning}))` | 直接映射 |
| 单次调用 usage | `StreamChunk::Done{usage}` | `CompletionCall` + `PromptResponse.usage` | 直接替换 |
| 最终回答 | `finalize_answer`（`llm_agent.rs:725`） | `FinalResponse(PromptResponse)` | 直接替换 |
| 会话 transcript | 手工维护 assistant+tool_calls / tool 结果并 push 上下文 | `PromptResponse.messages` | 大幅简化，run 结束一次性落库 |
| 长记忆 / 历史注入 | `build_initial_messages`（`llm_agent.rs:359`） | `history(...)`（外部拼好传入） | 职责外移 |
| 6 个观测钩子 | `HookRegistry` | `AgentHook`（可观测 + 可干预） | 重新映射，见 6.2 |
| 取消 | `consume_stream` 内 2s 轮询 DB 状态 | 无内建；drop stream | 外层 `select!` 包装 |
| 工具重试 / 参数修复 | `ToolCallRetryConfig` + `json_repair` | 无重试；有 `UnhandledInvalidToolCall` + `on_invalid_tool_call` | 下移为工具装饰器 + 钩子 |

### 6.2 钩子映射

| 现有钩子 | rig-agent | 用途 |
| --- | --- | --- |
| `before_agent` | `on_run_start` | 观测 / 可改写 prompt（如需） |
| `after_agent` | `on_run_settled` | 观测终态与耗时 |
| `before_model` | `on_completion_call` | 观测；可返回 `RequestPatch`（M4 前不做注入） |
| `after_model` | `on_model_turn_finished` | 观测；可 `retry_with_feedback` |
| `before_tool` | `on_dispatch` | **发 `ToolCallStarted`**，记录 `Instant` 到 `Scratchpad` |
| `after_tool` | `on_outcome` | **发 `ToolCallFinished`**，计算 `duration_ms` |

### 6.3 关键类型速查（rig-agent 0.45）

- 流：`AgentRunner::stream() -> StreamingResult = BoxStream<Result<MultiTurnStreamItem, PromptError>>`
- 流条目：`StreamAssistantItem(Item<StreamEvent>)` / `ToolCall` / `ToolExecutionCommitted` / `ToolResult` / `CompletionCall` / `ModelTurnRetried` / `FinalResponse`
  - 注意 `MultiTurnStreamItem` 是 `#[non_exhaustive]`，match 必须带通配分支
- 终态：`PromptResponse { usage, completion_calls, messages, content }`
- 错误：`PromptError::{Provider, Report, Memory, MaxTurns, ...}`
- 钩子事件：`DispatchEvent { id, kind, turn, call_id, context }`、`OutcomeEvent { tool_result, tool_context, ... }`
- 请求补丁：`RequestPatch { preamble, temperature, max_tokens, tool_choice, active_tools, extra_context, history, additional_params }`
- 会话记忆（可选）：`AgentBuilder::memory(store)` + `conversation(log)`，与 `history()` 互斥

---

## 七、明确放弃项

| 放弃 | 原因 |
| --- | --- |
| `ModelAdapter` / `StreamChunk` / `streaming::conv` | rig-agent 直接消费 `DynModel<Completion>` 与 `Item<StreamEvent>` |
| `ToolCallRetryConfig` / `arguments_json_repair` 主路径 | 下移为 `DynamicTool` 装饰器与 `on_invalid_tool_call`，禁止在 `RunConfig` 与框架各配一份 |
| 运行中注入消息（`drain_pending_input`） | rig-agent 无对应；若要保留只能靠 `CompletionCallAction::Patch(RequestPatch.history)`，语义已变，做成默认关闭的可选项 |
| `ChainAgent` / `ParallelAgent` / `MockModel` | 死代码；与 rig-agent 的单 run 模型也不匹配 |
| `Answer` 内容双份 / `Reasoning` 整段 / `ToolCallsPlanned` 落库 | 第二真源，M4 收敛 |
| 自研会话状态机 | 收敛为 `SessionService` 的 `Running / Completed / Cancelled` 三态，观测层在 run 前后各写一次 |

---

## 八、逐文件改造清单

### 8.1 删除（约 3,300 行）

| 文件 | 行数 | 说明 |
| --- | --- | --- |
| `runtime/agent/llm_agent.rs` | 1533 | 由 `runtime/agent/harness_agent.rs` + `observer/*` 取代 |
| `runtime/llm/model_caller.rs` | 777 | 事件聚合交 `observer/stream_map.rs` |
| `runtime/llm/tool_executor.rs` | 411 | 重试/审计下沉到 `tool/retry.rs` 与 `observer/hooks.rs` |
| `runtime/llm/types.rs` | 41 | `ModelTurn` 不再需要 |

> `chain_agent.rs`（172）与 `parallel_agent.rs`（254）**不删**，迁至 `runtime/orchestration/` 并补完（§2.5 / §3.3）。
| `model/adapter.rs` | 115 | 直接用 `DynModel<Completion>` |
| `model/streaming/{mod,chunk,conv}.rs` | 212 | rig-agent 内部已聚合 |
| `model/mock.rs` | 219 | D-2 |
| `runtime/llm/types.rs` | 41 | `ModelTurn` 不再需要 |

### 8.2 新增

| 文件 | 预估行数 | 内容 |
| --- | --- | --- |
| `runtime/agent/harness_agent.rs` | 350-450 | 实现 `Agent` / `AgentOpenAI`；run 生命周期（ensure_session_running、RunStarted/UserMessage/RunFinished、终态写回）；取消 `select!` 包装 |
| `runtime/observer/stream_map.rs` | 200-300 | `MultiTurnStreamItem` → `AgentStreamEvent`；`UsageAccumulator`、`StepTracker`（可从 `model_caller.rs:388-442` 迁移） |
| `runtime/observer/hooks.rs` | 200-280 | 工具/模型钩子 → 事件；`Scratchpad` 记 `Instant`；`source_of` 回填 |
| `runtime/observer/persist.rs` | 150-220 | 双写策略、transcript（`PromptResponse.messages`）→ 上下文落库 |
| `runtime/observer/metrics.rs` | 100-150 | 指标门面 |
| `runtime/orchestration/{mod,chain,parallel}.rs` | 350-500 | 编排能力补完：`run_stream` 事件透传、失败策略、显式并发上限、session 归属 |
| `runtime/orchestration/executor.rs` | 120-180 | 补齐 `traits.rs` 引用的 `AgentExecutor`（组合体流式出口） |
| `tool/retry.rs` | 100-150 | `DynamicTool` 重试 / 参数修复装饰器 |
| `tool/guard.rs` | 100-150 | `DynamicTool` 超时 / 输出截断装饰器（§5.4 T-1） |

### 8.3 改写

| 文件 | 动作 |
| --- | --- |
| `runtime/agent/builder.rs`（760） | 保留 `AgentBundle` / `AgentSource` / 去重 / 截断；`build()` 末尾：`allow_tools(bundle.tool_names())` + `dynamic_tools(pool.dynamic_tools())` → 返回 `HarnessAgent`；`load_builtin_tools` 按白名单注册 |
| `runtime/agent/traits.rs`（458） | 保留 `Agent` / `AgentOpenAI` / `AgentSource`；`run_stream` 的 `_ => {}` 改为显式列举 `Chunk` / `Done` / `Skip`（随 D-6） |
| `openai/stream_adapter.rs`（487）/ `openai/mod.rs` | 删除 `AdapterOutput::Extension` 变体与 `NUCLEUS_EVENT_PREFIX`；`AdapterOutput` 保留 `Chunk` / `Done` / `Skip` 三变体，`openai/mod.rs:47` 导出不变 |
| `runtime/llm/mod.rs` | 只留 `json_repair` |
| `model/mod.rs` / `model/openai.rs`（34） | `create_model` 返回 `DynModel<Completion>` |
| `tool/pool.rs`（319） | 新增 `pub fn dynamic_tools(&self) -> Vec<DynamicTool>`；`call` 保留供内部/测试使用 |
| `sub_agent/tools/{delegate,flat}_sub_agent.rs` | 子 Agent 用 rig `AgentBuilder` 构建；事件回放并包 `SubAgentEvent`；`is_primary` 改为 per-run 标志 |
| `runtime/agent/{chain,parallel}_agent.rs` | 迁至 `runtime/orchestration/`；补 `run_stream`（子 run 事件包 `ChainStep` / `ParallelBranch` 透传）、子 run 强制 `is_primary=false`、并发上限改为显式 `ConcurrencyMode` |
| `runtime/agent/traits.rs`（补） | `run_stream` / `infer_stream` 的默认实现错误文案不再引用未定义的 `AgentExecutor`（先落地 §3.2 的 `orchestration/executor.rs`，再更新文案） |
| `services/agent-service/service/agent_run.rs`（281） | 收敛为 `RunSource` + `Surface` + 单一 `execute`；接入 `AgentCache`（L0/L1，L2 待 P3-5）；错误类型改 `err_code::Error` |
| `services/agent-service/dto/agent_run.rs` | 删 `AgentRunResp`（D-3）、`AgentRunReq.debug`（D-4）；`SseEventData` 改用 `event_type()`（R-1） |
| `crates/agent-runtime/Cargo.toml` | 新增 `rig-agent = { workspace = true }`；移除随删除文件失效的依赖 |
| 根 `Cargo.toml` | `[workspace.dependencies]` 新增 `rig-agent = "0.45"` |

### 8.4 不动

`session/event.rs`（M4 前）、`session/{traits,in_memory}.rs`、`session/outcome.rs`（仅补字段）、`openai/*`、`prompt/*`、`message.rs`、`rule/*`、`command/*`、`skill/*`、`mcp/*`、`a2a/*`、`agent-assembly` 全部、`services/agent-service` 除 `agent_run` 外的所有模块。

---

## 九、里程碑与验收

| 里程碑 | 内容 | 验收判据 | 破坏性 |
| --- | --- | --- | --- |
| **M0 清理** | 死代码（D-2..D-6；D-1 保留）、`event_type()` 统一（R-1）、`RunOutcome` 补 `run_id/session_id`（F-9） | `cargo check --workspace` + clippy 无新增告警；HTTP 契约不变 | 无 |
| **M1 准入** | F-1/F-2/F-3/F-4/F-8：白名单生效、builtin 默认关闭、`knowledge_query` 按需、noop 告警、`AgentCache` + MCP 常驻、per-tool 超时/截断装饰器（§5.4 T-1） | ①`tool_definitions()` 与后台配置逐项一致；②单 agent 连续 100 次运行 MCP 仅建连 1 次；③未配置 agent 的工具集不含 `run_command`；④卡死工具在超时后返回错误而非阻塞整批 | 有（工具可用性变化，需先刷配置数据或加"未配置=全开"兼容开关） |
| **M2 迁移** | 观测层 + `HarnessAgent`，与旧 `LlmAgent` 并存（配置开关切换） | **同一 session 分别走新旧路径，`t_agent_session_event` 逐行 `event_type` / 顺序 / `tool_call_finished.result` 对齐**；批量工具中途 abort 不丢行 | 无（双路径） |
| **M3 换心** | 删旧 loop；子 Agent / skill / a2a 切新实现；`tool/retry.rs` 落地 | skill / mcp / sub_agent / a2a 四条链路 e2e 冒烟；前端零改动可渲染 | 无（事件协议未变） |
| **M4 协议定稿** | R-2 / R-3 / R-4：`Answer` 语义、`Reasoning` 合并、`ToolCallsPlanned` 不落库；F-5/F-6/F-7 服务层收敛 | DB 迁移脚本 + 历史事件兼容读；前端一版适配完成 | 有（需前端与回放页联调） |
| **M5 编排** | §2.5 O-1..O-7 + §3.3/3.4/3.5：`ChainAgent` / `ParallelAgent` 补 `run_stream` 与事件透传、`AgentExecutor` 落地、失败策略 + `ConcurrencyBudget`、`DelegateType` 扩展 | ①链/并行体可通过 OpenAI 流式端点与 debug 端点运行；②并行 3 分支时 session 状态不被覆盖、不出现"已有活跃运行"报错；③编排事件含 `parent_run_id` + 子 `run_id`，前端可渲染分支进度 | 有（新增事件变体，需前端配合） |

### 9.1 执行顺序约束

- **M1 必须先于 M2**：迁移的对齐测试需要语义正确的基线；若工具白名单本身失效、`knowledge_query` 一直骗模型，将无法区分"新框架问题"与"旧缺陷"。
- **M2 是胜负手**：对齐测试不通过不得进入 M3。
- **M4 单独一版**：不得与 M2/M3 同批提交，否则协议变更与框架迁移的问题无法二分定位。

### 9.2 回滚策略

- M0/M1：常规 revert，无数据结构变化。
- M2：双路径开关（如 `run_stream_with_config` 优先走新实现），出问题切回旧路径。
- M3：一次性删除，靠 M2 对齐测试背书；建议在 M3 前打 tag。
- M4：DB 迁移必须可回滚（新增列/新事件名而非覆写旧值）。

---

## 十、风险与取舍

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| **R-1 工具事件无法从流取** | 若照搬"消费流即入库"，批量工具 abort 会丢行、前端卡片延迟 | 强制走 `on_dispatch` / `on_outcome`；M2 对齐测试必须覆盖 abort 场景 |
| **R-2 `max_turns` 口径变化** | rig-agent 是"模型调用总预算（含重试）"，现为"步骤数"，极端重试会提前耗尽 | 观测层把 `MaxTurns` 单独计数；文档标注差异 |
| **R-3 子 Agent 会话语义外移** | 现由 `meta.is_primary()` 在 loop 内跳过状态管理；迁移后归编排层，写错会让父会话被子 Agent 覆盖成 `Completed` | 子 run 显式传 `is_primary=false`；单测覆盖"子 run 不改 session 状态" |
| **R-4 工具实时性与 transcript 可能不同步** | `DispatchAction::Patch` 改写参数后，hook 看到的是改写后的值 | 定稿：事件表以 hook 为准，transcript 以 `PromptResponse.messages` 为准，用 `call_id` 关联；异常路径差异写入文档 |
| **R-5 MCP 连接生命周期** | 现状每请求重连；常驻后需处理断连与工具列表变更 | registry 健康检查 + 运维刷新接口 + `AgentCache` 失效 |
| **R-6 新增依赖体积** | `rig-agent` 引入 `rig-memory` / `schemars` / `rig-derive`；要求 rust 1.97.1+ | 当前 nightly 1.101 满足；依赖已在本机 registry 缓存 |
| **R-7 编排与委派共用 session** | 并行分支共用一个 `session_id`，若子 run 仍按 primary 处理会互相踩（即当前 O-4） | M5 前先落地"子 run 一律 `is_primary=false`"；单测覆盖"3 个并行分支不改写 session 状态" |
| **R-8 并发预算缺失** | 编排并发 × 工具并发 × 分支数无上界，易打爆上游限流与连接池 | `ConcurrencyBudget` 由 root run 持有，子 run 只能借用；`Unlimited` 必须显式声明并打指标 |

---

## 十一、待确认问题

1. **`tool_configs` 白名单切换策略**：现有线上 agent 是否都已配置工具清单？若无，M1 需要"未配置 = 全开（含 run_command）"的过渡开关，还是一个强制刷数据的窗口？
2. **`run_command` 是否保留**：若 agent 需要执行本地命令，需明确沙箱/超时/白名单；否则建议 M1 直接下架。
3. **`knowledge` 子系统去留**：当前只有 `NoopKnowledgeRetriever`；若无近期接入计划，M1 直接删除（约 300 行）。
4. **`pending_input` 是否保留**：若前端有"运行中插话"需求，需要单独设计（`RequestPatch.history` 方案），否则 M0 一并删除。
5. **M4 前端适配窗口**：`Answer` 语义变更与 `Reasoning` 删除需要前端版本配合，需提前排期。
6. **编排事件形态**：扩展 `DelegateType`（新增 `ChainStep` / `ParallelBranch`，复用 `SubAgentEvent` 信封）还是另起独立事件族 `OrchestrationEvent`？前者复用度高、事件表结构不变；后者语义更清晰、但需前端新增渲染分支。
7. **默认并发上限**：`ParallelAgent` 默认 `Limited(4)`、工具并发默认 8，是否符合线上上游限流与 MCP 连接池容量？需要按实际供应商配额校准。

---

## 十二、执行任务清单（按提交顺序）

### 12.1 排序五原则

1. **先固化基线，再迁移**：没有基线，无法证明"换心未换行为"。
2. **先修准入，再做迁移**：拿一条错误基线（工具白名单失效、`knowledge_query` 空转）去对齐，等于白做。
3. **先新增后删除**：每一步都能独立 revert。
4. **协议变更最后**：避免"框架迁移"与"事件协议"两个变量同时动，出问题无法二分。
5. **编排最后**：依赖 M2 落地的"子 run 不写 session 状态"机制。

### 12.2 依赖图

```text
P0 准备 ──┬─► P1 清理(M0) ──┐
          └─► P2 准入(M1) ──┴─► P3 迁移(M2) ══► P4 换心(M3) ──┬─► P5 协议(M4)
                    │                                          └─► P6 编排(M5)
                    └─（P1 与 P2 可并行：文件不重叠）

══ 表示闸门：P3 的对齐测试不通过，不得进入 P4
```

### 12.3 P0 准备（约 0.5 人日）

- [ ] **P0-1 接入依赖**：根 `Cargo.toml` 增加 `rig-agent = "0.45"`；`agent-runtime` 增加依赖；`cargo check -p agent-runtime` 通过（验证 rust 1.97.1 要求与依赖解析）。
- [ ] **P0-2 固化事件基线**：新增 `crates/agent-runtime/tests/event_baseline.rs`，用脚本化 model（固定文本 / 固定 tool_calls 序列）跑一次完整 ReAct，把 `AgentStreamEvent` 序列化后与 `tests/fixtures/event_baseline.json` 比对。**这是 P3 的验收基准，必须在改动任何 loop 之前落地。**
- [ ] **P0-3 决策三条**：确认 §十一 的 1（白名单切换策略）、2（`run_command` 去留）、3（`knowledge` 去留）。**阻塞 P2。**

### 12.4 P1 清理（M0，约 1 人日，5 个独立 commit）

- [ ] **P1-1** `refactor(runtime): remove unused mock model` —— 删 `model/mock.rs` 与 `model/mod.rs` 导出（D-2）。
- [ ] **P1-2** `refactor(service): drop dead run dto` —— 删 `AgentRunResp`、`AgentRunReq.debug`（D-3/D-4）。
- [ ] **P1-3** `refactor(runtime): drop unused message metadata` —— 删 `ConversationMessage.metadata` 与 `with_metadata`（D-5）。
- [ ] **P1-4** `refactor(openai): remove unused extension channel` —— 删 `AdapterOutput::Extension` 与 `NUCLEUS_EVENT_PREFIX`，`traits.rs` 的 `_ => {}` 改显式列举（D-6）。
- [ ] **P1-5** `refactor(runtime): unify event type mapping` —— `SseEventData::from_agent_event` 改用 `event.event_type()`，`ModelCaller::log_item` 删除；顺带给 `RunOutcome` 补 `run_id` / `session_id`（R-1 / F-9）。

验收：`cargo check --workspace` + `cargo clippy` + 单测全绿；`P0-2` 基线 fixture 不变（证明零行为变化）。

### 12.5 P2 准入（M1，约 3 人日）

> 顺序刻意如此：先加观测（P2-1）、再加护栏（P2-2），最后才动准入与生命周期（P2-3~P2-6）。

- [ ] **P2-1** `feat(runtime): warn on noop session service` —— `RuntimeServices::noop()` 首次写入 `warn_once`；`build()` 检测默认 session 实现并提示（F-4）。
- [ ] **P2-2** `feat(tool): add timeout and truncation guards` —— 新增 `tool/guard.rs` 装饰器（per-tool 超时 + 输出截断），`ToolPool` 注册时可选叠加（T-1）。
- [ ] **P2-3** `feat(runtime): enforce tool whitelist from bundle` —— `build()` 末尾按 `bundle.tool_names()` 调 `allow_tools`；**同时加"未配置 = 全开"过渡开关**（F-1）。
- [ ] **P2-4** `feat(tool): gate builtin tools behind whitelist` —— `run_command` / `duckduckgo_search` 默认不注册，需出现在白名单（F-2）。依赖 P2-3。
- [ ] **P2-5** `refactor(runtime): register knowledge tool on demand` —— 仅当 retriever 非 Noop 时注册，或整体删除 knowledge 子系统（F-3，按 P0-3 决策）。
- [ ] **P2-6** `feat(service): cache bundle and keep mcp registry resident` —— 见下方「P2-6 详细设计」。**注意：只做 L0 + L1，L2（缓存整个 Agent）推迟到 P3-5 之后**（`HarnessAgent` 定型后才有稳定值类型）。

验收：①`tool_definitions()` 与后台配置逐项一致；②单 agent 连续 100 次运行 MCP 仅建连 1 次；③未配置 agent 的工具集不含 `run_command`；④卡死工具在超时后返回错误而非阻塞整批；⑤配置修改后 ≤ 1 个请求内生效（不得出现长期旧配置）。

#### P2-6 详细设计：装配成本与三级缓存

**现状证据（每个请求都在重复做）**：

- `DbAgentSource::load_bundle` 一次并发 8 个查询（`source.rs:188-197`：agent 配置 / 模型绑定 / rule / command / skill / mcp / tool / sub_agent / a2a）；
- 子智能体由 `DbSubAgentLoader` 递归装配，每个子 agent 再跑一遍这 8 个查询 —— 成本随委派树线性放大；
- `McpRegistry` 是 `AgentBuilder` 的字段（`mcp/registry.rs:170-177` 为普通 struct，`connections: HashMap<String, McpConnection>`），每次 `build()` 新建并 `connect_all()`（`builder.rs:607-625`），即**每请求 N 次 MCP 握手**（stdio 还要 spawn 子进程）。

**三级方案（推荐先做 L1，L2 延后）**：

| 级别 | 内容 | 失效依据 | 落地时机 |
| --- | --- | --- | --- |
| **L0** | 缓存 `AgentBundle`（`agent_id → Arc<AgentBundle>`） | 配置版本号 | P2-6 |
| **L1** | `McpRegistry` 提升为进程级单例（nject provider 或 `OnceLock<Arc<Mutex<_>>>`），`connect_all()` 仅首次；`build_tools()` 每次从常驻连接派生 `DynamicTool`（轻量，闭包 Arc 指向同一连接） | 手工刷新 + 健康检查 | P2-6 |
| **L2** | 缓存装配产物 `agent_id → Arc<rig_agent::Agent>`（rig `Agent` 是不可变快照，可安全跨请求共享；per-run 状态全在 `tool_context` 里） | 配置版本号 | **P3-5 之后** |

**L2 的三个前置条件（缺一不可）**：

1. 缓存的值类型必须是 `HarnessAgent`（P3-5 定稿），不能是当前 `LlmAgent`，否则 P3 换心时缓存层要重写；
2. 每次 run 必须完整注入 `tool_context`（`run_id` / `session_id` / `step` / 事件通道），否则并发请求会互相串事件；
3. **归属校验必须在查缓存之前**：`DbAgentSource` 当前用 `.filter(agent_config::Column::UserId.eq(user_id))` 做私有性校验（`source.rs:225`），命中缓存会绕过它。规则固定为「先按 `(agent_id, caller_user_id)` 校验归属 → 再查缓存 → 缓存键用 `agent_id`」。

**配置版本号怎么取**：`t_agent_config.updated_at` 与所有关联绑定表（rule / skill / command / mcp / tool / sub_agent / a2a / 模型绑定）的 `max(updated_at)` 取最大值，写入缓存条目；请求时比对一次（一条聚合 SQL，成本远低于 8 条装配查询）。MCP 侧另有 registry 自身的 `tools/list` 快照版本。

**MCP 常驻的三条运维要求**：

1. 显式 `close_all()`：Registry 常驻后连接不再随 `Drop` 释放，优雅关机 / 配置移除时必须主动关闭（stdio 需确保子进程退出，避免僵尸进程）；
2. 健康检查 + 重连：服务端会话过期（HTTP/SSE 长连接）要能自动重建，并计入 `mcp_reconnect_total`；
3. 工具快照失效：server 侧工具变更不触发 DB `updated_at`，需要运维接口或定时刷新 registry 工具列表。

**必须观测的指标**：`agent_cache_hit_rate{level}`、`assembly_duration_ms`、`mcp_connect_total{server}`、`mcp_reconnect_total{server}`、`agent_cache_entries`、`config_version_stale_total`。

**风险（已有对应缓解）**：

| 风险 | 缓解 |
| --- | --- |
| 配置热更新延迟 | 版本号比对；单测覆盖"改配置后 ≤ 1 请求生效" |
| 连接 / 子进程泄漏 | `close_all()` + 关机钩子；`mcp_connect_total` 与 `agent_cache_entries` 双指标告警 |
| 跨用户语义混淆 | 明确：`AgentDefinition.user_id` 是**配置所有者**（不是调用者），事件与 session 的 `user_id` 落的是 owner；调用者身份只在准入与审计链路使用 |
| 缓存穿透（频繁换 agent_id） | 缓存上限 LRU + `agent_cache_evicted_total` 指标 |
| `DynamicTool` 与连接生命周期耦合 | registry 常驻期间不重建连接，只重建闭包；`close_all()` 后必须让上层缓存一起失效 |

### 12.6 P3 迁移（M2，约 6 人日，闸门阶段）

- [ ] **P3-1** `feat(tool): expose dynamic tools from pool` —— `ToolPool::dynamic_tools()`（约 10 行）。
- [ ] **P3-2** `feat(runtime): add stream to event mapper` —— `observer/stream_map.rs`：`MultiTurnStreamItem` → `AgentStreamEvent`；迁移 `UsageAccumulator` / `StepTracker`；**单测直接复用 P0-2 的 fixture 形状**。
- [ ] **P3-3** `feat(runtime): add agent hooks observer` —— `observer/hooks.rs`：`on_dispatch` → `ToolCallStarted`、`on_outcome` → `ToolCallFinished`（`Scratchpad` 记 `Instant` 算 `duration_ms`）、`on_run_settled` → after-agent。
- [ ] **P3-4** `feat(runtime): add event persistence writer` —— `observer/persist.rs`：低频双写 / 高频只推流；run 结束后用 `PromptResponse.messages` 批量写 session 上下文。
- [ ] **P3-5** `feat(runtime): add rig-backed agent` —— `HarnessAgent`（实现 `Agent` + `AgentOpenAI`）：装配 rig `Agent`、`tool_context` 注入 `AgentRunScope`、run 生命周期（ensure_session_running / RunStarted / UserMessage / RunFinished / 终态写回）。
- [ ] **P3-6** `feat(service): add dual-path run switch` —— 新旧实现并存开关（如 `run_stream_with_config` 优先走新实现），便于灰度与回滚。
- [ ] **P3-7** `feat(runtime): cancel run by dropping stream` —— 外层 `select!` 轮询 session 状态 → drop stream；子 run `is_primary=false` 机制（编排 M5 复用）。
- [ ] **P3-8 闸门**：对齐测试通过 —— 同一 fixture、同一配置下，新旧两条路径产出的 `AgentStreamEvent` 序列逐行一致；真实 DB 再跑一次含工具与取消的会话做二次确认。

**P3-8 不通过不得进入 P4。**

### 12.7 P4 换心（M3，约 2.5 人日）

- [ ] **P4-1** `refactor(sub_agent): rebuild child agent on rig` —— `delegate_sub_agent` / `flat_sub_agent` 改用 rig `AgentBuilder`，事件回放包 `SubAgentEvent`。
- [ ] **P4-2** `refactor(skill,mcp,a2a): verify tool chains on new runtime` —— 三条链路 e2e 冒烟，修工具描述长度/超时接入 P2-2 护栏。
- [ ] **P4-3** `feat(tool): add retry decorator` —— `tool/retry.rs` 落地（原 `ToolCallRetryConfig` 语义下移）。
- [ ] **P4-4** `refactor(runtime): remove legacy react loop` —— 删 `llm_agent.rs` / `model_caller.rs` / `tool_executor.rs` / `model/adapter.rs` / `model/streaming/*` / `runtime/llm/types.rs`；`AgentBuilder::build` 只返回 `HarnessAgent`。
- [ ] **P4-5** `chore: clippy and fmt` —— `cargo clippy --workspace`、`cargo fmt`，迁移前的临时开关（P3-6）可保留一个版本后移除。

验收：`P0-2` 基线 fixture 仍绿；skill / mcp / sub_agent / a2a 四条链路冒烟通过；前端零改动可渲染。

### 12.8 P5 协议定稿（M4，约 4 人日，需前端配合）

> 可与 P6 并行启动，但建议 P5 先行：编排事件需要纳入同一次协议定稿。

- [ ] **P5-1** `refactor(service): collapse run entrypoints` —— `RunSource` + `Surface` + 单一 `execute`；抽 `sse_chunks()` / `sse_events()`；错误类型收敛到 `err_code::Error`（F-5/F-6/F-7，**无破坏，可提前到 P4 前做**）。
- [ ] **P5-2** `feat(runtime): make answer a terminal marker` —— `TextDelta` 全轮透出，`Answer` 降级为终止标记（R-2）。
- [ ] **P5-3** `refactor(runtime): merge reasoning events` —— 删 `Reasoning` 整段变体，只留 `ReasoningDelta`（R-3）。
- [ ] **P5-4** `refactor(runtime): stop persisting planned calls` —— `ToolCallsPlanned` 仅推流不落库（R-4）。
- [ ] **P5-5** `chore(migration): event compatibility` —— DB 迁移（新增列/新事件名，不覆写旧值）+ 历史事件兼容读。
- [ ] **P5-6 前端联调**：回放页与聊天页适配一版。

### 12.9 P6 编排补完（M5，约 6 人日）

- [ ] **P6-1** 定稿编排事件形态（§十一 第 6 条决策）+ `FinishReason` 是否新增 `BudgetExceeded`。
- [ ] **P6-2** `refactor(runtime): move orchestration into own module` —— `chain_agent.rs` / `parallel_agent.rs` 迁 `runtime/orchestration/`。
- [ ] **P6-3** `feat(orchestration): stream child run events` —— 补 `run_stream`：子 run 事件包 `ChainStep` / `ParallelBranch` 后透传，不再降级为 `RunOutcome`（O-1/O-3/O-6）。
- [ ] **P6-4** `fix(orchestration): child runs never own session` —— 子 run 强制 `is_primary=false`，修并行编排 session 冲突（O-4）。
- [ ] **P6-5** `feat(orchestration): add executor` —— 落地 `orchestration/executor.rs`（`AgentExecutor`），并修正 `traits.rs` 三处失效文案（O-2）。
- [ ] **P6-6** `feat(orchestration): failure policy and budget` —— `StepFailure` / `ConcurrencyMode::Limited(n)` / `OrchestrationBudget`（O-5，§3.5）。
- [ ] **P6-7 测试**：3 分支并行时不出现"已有活跃运行"、session 状态不被覆盖、事件含 `parent_run_id` + 子 `run_id`。

### 12.10 明确不要做的顺序（踩坑清单）

| 不要 | 原因 |
| --- | --- |
| 在 P3 期间顺手改事件协议 | 迁移与协议两个变量同时动，对齐测试失去意义 |
| 先删 `llm_agent.rs` 再做 P3 | 没有旧路径就无法做双路径对齐，等于放弃验收 |
| P4 与 P5 同一批提交 | 出问题无法区分钟是"换心"还是"改协议" |
| P2-3 / P2-4 分开发布到生产 | 中间态是"白名单生效但 builtin 全开"，比现状更混乱 |
| 跳过 P0-2 直接开工 | 没有基线，M2 的验收无从谈起 |

### 12.11 工期与并行度

| 阶段 | 人日 | 可并行 |
| --- | --- | --- |
| P0 准备 | 0.5 | — |
| P1 清理（M0） | 1 | 与 P2 并行（不同文件） |
| P2 准入（M1） | 3 | 与 P1 并行 |
| P3 迁移（M2） | 6 | 内部 P3-2~P3-5 可由两人分担 |
| P4 换心（M3） | 2.5 | — |
| P5 协议（M4） | 4（含前端） | P5-1 可提前到 P4 前 |
| P6 编排（M5） | 6 | 与 P5 部分并行 |
| **合计** | **约 23 人日** | 最紧路径：P0 → P2 → P3 → P4 |

## 附录 A：事件字典（M4 定稿后的目标形态）

| 事件 | 关键字段 | 落库 | 出口 |
| --- | --- | --- | --- |
| `run_started` | `run_id`、`agent_id`、`agent_name`、`session_id`、`user_id`、`meta` | 是 | SSE / Debug |
| `user_message` | `run_id`、`content` | 是 | SSE / Debug |
| `step_started` | `run_id`、`step` | 是 | SSE / Debug |
| `text_delta` | `run_id`、`delta` | 否 | SSE / OpenAI |
| `reasoning_delta` | `run_id`、`id`、`delta` | 否 | SSE / Debug |
| `tool_calls_planned` | `run_id`、`step`、`calls[]` | 否（M4 起） | SSE / Debug |
| `tool_call_started` | `run_id`、`step`、`toolname`、`call_id`、`arguments_json`、`source` | 是 | SSE / Debug |
| `tool_call_finished` | 上述 + `success`、`result`、`error_message`、`duration_ms` | 是 | SSE / Debug |
| `sub_agent_event` | `parent_run_id`、`delegate_target`、`delegate_type`、`event` | 是 | SSE / Debug |
| `answer` | `run_id`、`content`（幂等镜像） | 是 | SSE / OpenAI |
| `run_finished` | `run_id`、`total_steps`、`finish_reason`、`error_message`、`duration_ms`、`usage` | 是 | SSE / OpenAI / Debug |

## 附录 B：M0 可直接开工的最小切片

1. 删 `model/mock.rs`（D-2）与 `model/mod.rs` 对应导出；`ChainAgent` / `ParallelAgent` **保留**（见 §2.5）；
2. 删 `dto::agent_run::AgentRunResp`（D-3）、`AgentRunReq.debug`（D-4）、`ConversationMessage.metadata`（D-5）；
3. 删 `AdapterOutput::Extension` 分支与 `NUCLEUS_EVENT_PREFIX`（D-6）；
4. `SseEventData::from_agent_event` 改为 `event.event_type()`；`ModelCaller::log_item` 删除或改用 `Debug`（R-1）；
5. `RunOutcome` 增加 `run_id` / `session_id`，`from_stream` 从 `RunStarted` 提取（F-9）；
6. `cargo check --workspace` + `cargo clippy` + 现有单测全绿。

预计净减约 700 行，无行为变化。
