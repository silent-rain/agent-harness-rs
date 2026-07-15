## 智能体基座开发计划

### 1. 功能说明

本文档是 [agent-platform-foundation-plan.md](/home/one/code/nucleus-rs/md-byAI/agent-platform-foundation-plan.md) 的实施配套文档，目标是将规划内容收敛为**可执行的开发阶段、任务清单、交付物与验收标准**。

本计划面向当前 `nucleus-rs` 仓库，围绕以下核心目标推进：

- 建设 `agent-runtime` 作为智能体运行时基座
- 建设 `agent-platform` 作为配置装配、治理与安全边界层
- 建设 `agent-service` 作为外部 API、调试和流式运行入口
- 优先复用现有 `rag-core`、`job-engine` 与现有服务体系
- 采用 **function calling / 结构化调用协议优先** 的运行循环

---

### 2. 开发原则

#### 2.1 总体原则

- **先基座，后增强**：优先把单 Agent 运行时跑通，再补知识、记忆、多 Agent 和治理能力
- **先抽象，后接入**：先稳定 `agent-runtime` trait 与 runtime 抽象，再接数据库、本地资源与服务层
- **先低风险能力，后高风险能力**：优先只读能力与受控工具，后续再开放终端写入、MCP 和多 Agent 委派
- **先结构化动作，后自然语言呈现**：可执行动作只走函数调用协议，最终用户输出才走自然语言
- **先最小可用，后平台化完善**：先保证能运行、可调试、可约束，再补监控、回放、灰度和回滚

#### 2.2 关键技术决策

实施过程中应始终坚持以下约束：

- **`agent-runtime` 不直接依赖数据库查询细节**
- **运行前自动探测 `.codebuddy`，存在则参与装配，不存在则忽略**
- **工具、RAG、记忆召回、子智能体委派统一走结构化动作协议**
- **所有文件与命令类能力必须经过工作区策略校验**
- **记忆主存储在数据库，检索增强优先复用 `rag-core`**

---

### 3. 范围与边界

#### 3.1 本次计划覆盖范围

本计划覆盖以下内容：

- `agent-runtime` crate 的核心运行时能力
- `agent-platform` crate 的装配与治理能力
- `agent-service` 的服务入口与流式运行接口
- Agent 域数据库模型与运行快照设计
- 与 `rag-core`、`job-engine`、`.codebuddy` 的集成策略

#### 3.2 本次计划暂不覆盖范围

以下内容不作为首轮强制交付：

- 可视化 DAG 编排器
- 图形化 Prompt / Skill 编辑器
- 跨集群分布式多 Agent 协同
- 完整 A2A / AG-UI 标准协议适配
- 大规模自动化评测平台

---

### 4. 目标架构与实施顺序

#### 4.1 目标模块结构

建议形成如下模块结构：

```text
crates/core/agent-runtime
crates/core/agent-platform
service-hub/agent-service
```

依赖关系建议如下：

```text
agent-service
  └── agent-platform
        ├── agent-runtime
        ├── rag-core
        ├── job-engine
        ├── database / entity / migration
        └── rig-core (optional)
```

#### 4.2 推荐实施顺序

建议按照以下顺序落地：

1. `agent-runtime` 领域模型、运行时协议、函数调用协议、事件流模型
2. `agent-platform` 快照解析、配置融合、工作区策略、运行装配
3. Agent 域数据库表、迁移与 Repository
4. `agent-service` API、SSE 流式运行、调试入口
5. `rag-core` 适配、记忆读写、`.codebuddy` 自动发现
6. MCP、子智能体、记忆蒸馏、治理与回放

这样可以避免服务层先行、底层抽象频繁返工的问题。

---

### 5. 阶段开发计划

### 5.1 Phase 0：方案冻结与基础骨架

**目标**：冻结抽象边界，避免开发期反复改接口。

#### 主要任务

- 明确 `agent-runtime` / `agent-platform` / `agent-service` 目录结构
- 明确 `AgentDefinitionSnapshot`、`AgentRuntime`、`AgentStreamEvent` 基础模型
- 明确 Planner 与 Executor 之间的结构化调用协议
- 明确工作区安全模型 `WorkspaceRuntimePolicy`
- 明确数据库首批表结构与命名规范
- 明确 `.codebuddy` 的扫描目录和合并优先级

#### 交付物

- 模块目录草图
- 核心 trait 与 struct 草案
- 首批数据库 ER 草图
- 文档中的关键字段冻结版本

#### 验收标准

- 关键模块职责边界清晰
- 关键命名和字段命名不再大幅调整
- 运行循环明确采用 `function calling` / `JSON Schema` 结构化动作协议

---

### 5.2 Phase 1：最小可用运行时

**目标**：让单 Agent 在受控工作区内可运行、可流式输出、可调用基础工具。

#### 主要任务

- 新建 `agent-runtime`
- 实现基础领域模型：
  - `AgentDefinitionSnapshot`
  - `AgentRuntime`
  - `AgentAction`
  - `PlannerDecision`
  - `AgentStreamEvent`
- 实现基础 trait：
  - `AgentPlanner`
  - `AgentExecutor`
  - `AgentPolicyGuard`
  - `AgentEventPublisher`
- 实现函数注册与分发：
  - `AgentFunctionRegistry`
  - `AgentFunctionSchema`
  - `AgentActionDispatcher`
- 实现基础工作区策略校验
- 支持基础会话窗口与短期上下文
- 支持 SSE 输出最终回答和步骤事件

#### 首批建议支持的动作

- `call_tool`
- `finalize`

#### 首批建议支持的工具

- 文件读取
- 文件搜索
- 目录搜索
- 受控终端命令

#### 交付物

- `crates/core/agent-runtime`
- 基础运行时测试
- 最小可运行调试样例

#### 验收标准

- 能加载一个内存快照并运行
- 能以结构化调用方式触发工具
- 能在工作区限制下执行文件与命令能力
- 能流式输出步骤事件与最终回答

---

### 5.3 Phase 2：平台装配与配置持久化

**目标**：让运行时从平台配置中装配出来，而不是依赖手工构造。

#### 主要任务

- 新建 `agent-platform`
- 实现：
  - `AgentDefinitionProvider`
  - `DbAgentDefinitionProvider`
  - `LocalCodeBuddyProvider`
  - `CompositeAgentDefinitionProvider`
  - `AgentDefinitionResolver`
  - `AgentRuntimeResolver`
- 完成 Agent 域首批数据库表：
  - `t_agent`
  - `t_agent_version`
  - `t_agent_llm_model`
  - `t_agent_model_binding`
  - `t_agent_tool_registry`
  - `t_agent_tool_rel`
  - `t_agent_workspace`
  - `t_agent_workspace_rel`
- 建立快照生成逻辑
- 支持请求级覆盖、本地 `.codebuddy` 与数据库配置合并

#### 交付物

- `crates/core/agent-platform`
- 首批 migration / entity / repository
- 配置快照解析逻辑

#### 验收标准

- 可从数据库记录生成 `AgentDefinitionSnapshot`
- `.codebuddy` 存在时可自动扫描并参与配置融合
- 工作区配置可正确下发到运行时
- 不同来源配置冲突可按策略合并

---

### 5.4 Phase 3：服务入口与调试运行

**目标**：提供对外运行入口和调试能力，使平台可被真实调用。

#### 主要任务

- 新建 `service-hub/agent-service`
- 提供接口：
  - `POST /agent`
  - `PUT /agent/{id}`
  - `POST /agent/{id}/publish`
  - `POST /agent/{id}/rollback`
  - `GET /agent/{id}/versions`
  - `POST /agent/run`
  - `POST /agent/run/stream`
  - `POST /agent/debug/run`
- 接入 SSE 输出 `AgentStreamEvent`
- 记录基础 run log
- 支持调试模式输出函数调用与 observation 摘要

#### 交付物

- `service-hub/agent-service`
- 基础 API 路由与 DTO
- SSE 调试运行接口

#### 验收标准

- 可通过 API 发起单 Agent 运行
- 可流式看到结构化步骤事件
- 可查看基础运行结果与错误信息

---

### 5.5 Phase 4：知识与记忆接入

**目标**：让 Agent 具备知识检索和多轮上下文沉淀能力。

#### 主要任务

- 在 `agent-runtime` 中补齐知识与记忆运行时抽象：
  - `AgentKnowledgeAdapter`
  - `AgentKnowledgeRuntime`
  - `AgentMemoryRetriever`
  - `AgentMemoryWriter`
- 接入现有 `rag-core`
- 实现知识动作：
  - `query_knowledge`
- 实现记忆动作：
  - `recall_memory`
- 新增数据库表：
  - `t_agent_memory_stream`
  - `t_agent_memory_distilled`
  - `t_agent_memory_embedding`
- 支持运行后写入流水记忆
- 支持规划前召回高价值记忆

#### 交付物

- `rag-core` 适配层
- 记忆读写服务
- 记忆表结构与基础检索逻辑

#### 验收标准

- Agent 能按配置执行知识检索
- Agent 能写入与召回流水记忆
- Agent 能在最终回答中结合知识与记忆结果

---

### 5.6 Phase 5：Rules、Skills、MCP 与本地资源协同

**目标**：补齐运行时增强资源与外部能力接入。

#### 主要任务

- 实现 Rule 装配与分层注入
- 实现 Skill 装配与摘要注入 / 按需展开
- 接入 MCP 服务注册与白名单工具暴露
- 接入 `.codebuddy/rules`、`.codebuddy/skills`、`.codebuddy/agents`、`.codebuddy/sub-agents`
- 完成表结构：
  - `t_agent_rule_registry`
  - `t_agent_rule_rel`
  - `t_agent_skill_registry`
  - `t_agent_skill_rel`
  - `t_agent_mcp_service_rel`
  - `t_agent_sub_agent_rel`

#### 交付物

- Rule / Skill Provider
- MCP Runtime 适配层
- `.codebuddy` 本地资源索引逻辑

#### 验收标准

- Agent 能按启用清单装配 Rules 与 Skills
- Agent 能受控暴露 MCP 工具
- 本地与持久化资源可按策略协同工作

---

### 5.7 Phase 6：多 Agent 与调度能力

**目标**：让主 Agent 可以安全地委派子 Agent 完成复杂任务。

#### 主要任务

- 实现：
  - `SubAgentRegistry`
  - `SubAgentScheduler`
  - `DelegationPolicy`
- 支持动作：
  - `delegate_sub_agent`
  - `fan_out_sub_agents`
- 增加预算与限制：
  - 最大递归深度
  - 最大并发子 Agent 数
  - 最大总步数
  - 最大工具调用次数
  - 最大 token 成本
- 实现聚合结果归并

#### 交付物

- 子智能体调度器
- 多 Agent 聚合执行样例

#### 验收标准

- 主 Agent 可按结构化动作委派子 Agent
- 子 Agent 调用受预算和策略限制
- 多 Agent 结果可被主 Agent 安全聚合

---

### 5.8 Phase 7：治理、回放与生产能力

**目标**：让系统具备生产可观测性、审计能力与版本治理能力。

#### 主要任务

- 接入 tracing / metrics / hooks
- 增加：
  - run record
  - replay data
  - config snapshot 持久化
  - 工具调用审计
  - 成本统计
- 支持：
  - 版本发布
  - 灰度发布
  - 回滚
  - TTL 清理
  - 记忆蒸馏任务
- 通过 `job-engine` 执行异步蒸馏与补偿任务

#### 交付物

- 审计与回放能力
- 指标与 tracing 接入
- 版本治理流程

#### 验收标准

- 一次运行可完整回放关键步骤
- 配置可发布、回滚、追踪差异
- 工具、知识、子 Agent 调用具备审计记录

---

### 6. 里程碑建议

#### 6.1 里程碑划分

- **M1**：Phase 0 + Phase 1 完成，单 Agent 最小运行时可用
- **M2**：Phase 2 + Phase 3 完成，平台配置装配与服务入口可用
- **M3**：Phase 4 + Phase 5 完成，知识、记忆、Rules、Skills、MCP 可用
- **M4**：Phase 6 + Phase 7 完成，多 Agent 与生产治理能力可用

#### 6.2 推荐节奏

如果资源有限，建议优先保交付顺序：

- 第一阶段先做到 **M1**
- 第二阶段推进到 **M2**
- 只有当前两阶段稳定后，再进入 **M3 / M4**

这样可以把返工成本控制在最低。

---

### 7. 任务拆分建议

#### 7.1 基座开发任务

- 定义运行时核心结构体
- 定义 Planner / Executor trait
- 定义函数调用协议与参数校验机制
- 定义统一事件流协议
- 定义工作区策略校验器

#### 7.2 平台装配任务

- 数据库建模与 migration
- Repository 与配置读取
- 快照构建与合并策略实现
- `.codebuddy` 扫描器与缓存

#### 7.3 服务层任务

- API DTO 与路由
- SSE 推流
- 调试运行与错误返回
- 运行记录查询接口

#### 7.4 基础设施任务

- tracing 埋点
- metrics 指标
- run record 存储
- 回放数据格式定义
- 后台 job 接入

---

### 8. 风险与应对

#### 8.1 主要风险

- **抽象过度**：首轮做太多 trait 和扩展点，导致实现复杂度过高
- **边界漂移**：把数据库、服务逻辑或 RAG 细节塞进 `agent-runtime`
- **协议不统一**：部分动作走结构化调用，部分动作仍靠文本解析
- **多源配置冲突**：数据库与 `.codebuddy` 合并策略不清晰
- **安全漏洞**：工具绕过工作区边界直接访问本地路径或执行命令
- **可观测不足**：运行失败后无法回放与定位问题

#### 8.2 应对策略

- 首轮仅保留必要抽象，避免过早泛化
- 用模块边界和 crate 依赖强制限制职责
- 把结构化动作协议写成运行时硬约束
- 先实现默认合并策略，再开放可配置项
- 所有文件与命令能力统一走 `WorkspaceGuard`
- 事件流、run log、snapshot 从第一阶段起就预留扩展字段

---

### 9. 测试与验收策略

#### 9.1 单元测试

重点覆盖：

- `AgentAction` 解析与校验
- 工作区路径判断
- 配置合并策略
- 结构化动作分发
- 事件流对象序列化

#### 9.2 集成测试

重点覆盖：

- 从数据库到快照的完整装配链路
- `.codebuddy` 自动发现与融合
- API 流式运行
- 知识检索接入
- 多 Agent 委派链路

#### 9.3 回归测试

建议逐步建立：

- 单 Agent 编码任务样例
- 纯知识问答样例
- 工具调用样例
- 子 Agent 委派样例
- 错误恢复样例

---

### 10. 建议优先级

#### 10.1 必做项

- `agent-runtime` 最小运行时
- 工作区安全边界
- 结构化动作协议
- 平台配置快照装配
- SSE 流式输出
- 基础 run log

#### 10.2 高优先项

- `rag-core` 适配
- 记忆流水写入与召回
- `.codebuddy` 自动发现
- Rules / Skills 装配
- MCP 白名单暴露

#### 10.3 后续增强项

- 多 Agent 并发调度
- 记忆蒸馏与 TTL
- 回放与灰度治理
- 自动化评测与质量评分

---

### 11. 使用建议

#### 示例 1：先跑最小运行时

**场景**：验证 `agent-runtime` 的运行循环和结构化动作协议。

建议做法：

- 先使用内存快照
- 只开放只读文件类工具
- 通过 SSE 观察事件流
- 暂不接数据库和多 Agent

#### 示例 2：接入平台配置

**场景**：验证数据库快照装配和 `.codebuddy` 本地配置融合。

建议做法：

- 建立最小 Agent 配置数据
- 绑定模型、工具和工作区
- 在工作区放置 `.codebuddy` 资源做对照验证

#### 示例 3：接入知识与记忆

**场景**：验证 `rag-core` 与记忆层的组合效果。

建议做法：

- 先只接一个知识库
- 开启流水记忆写入
- 再逐步补精炼记忆和向量召回

---

### 12. 兼容性说明

- 本计划兼容当前 `nucleus-rs` 的模块结构和技术栈
- 本计划不要求推翻现有 `rag-core` / `rag-service` 体系
- 本计划允许分阶段新增 crate 和 service，不影响现有业务服务
- 本计划兼容 `.codebuddy` 本地资源与数据库双源装配模式
- 本计划坚持 `function calling` / `JSON Schema` 结构化调用协议优先，不以自由文本解析作为动作执行主路径

---

### 13. 推荐结论

建议把开发执行路线收敛为下面三句话：

- **先把 `agent-runtime` 的结构化运行时做扎实，再扩知识、记忆和多 Agent 能力**
- **先把 `agent-platform` 的装配、安全和快照治理做清楚，再开放更复杂的工具和调度能力**
- **始终坚持“动作层结构化，展示层自然语言”，避免让文本解析重新进入运行时主链路**

如果按这个开发计划推进，智能体基座会更容易在当前仓库内稳定落地，并且更适合后续持续演进。
