## `agent-runtime` 子智能体与能力装配架构分析报告

### 1. 背景与目标

确立 `agent-runtime` 的统一装配范式：

- **本地目录加载**（如 `.codebuddy`）：内置于各 `Registry` 中
- **动态配置加载**（数据库 / 远程配置）：通过 `Registry` 的动态装配方法传入
- **装配中枢集中于各 `Registry`，由 `AgentRuntimeBuilder` 组合后直接产出 `AgentRuntime`**

设计取向：

- **不引入中间抽象层**（不新增 `AgentBlueprint / AssemblySpec / CapabilityBundle / Composite Provider`）
- **不做兼容路径**，框架保持简洁、单路径、强约定
- **装配完成即进入运行时**，避免多层转换

---

### 2. 设计原则

- **Registry 即装配中枢**：本地扫描内置，动态配置通过方法注入，Registry 自己完成合并
- **Builder 两层 API**：底层 API（`with_*_registry()`）注入已装配的 Registry 对象；便捷 API（`with_*_configs()`）传配置由 Builder 内部路由到 Registry。两层可叠加、不做互斥
- **Provider 只做数据查询**：数据库 Provider 只负责按 `agent_id` 查出各关联表的原始记录
- **取消中间抽象**：不引入 `AgentBlueprint / AssemblySpec / CapabilityBundle / CompositeProvider`
- **`snapshot` 保持轻量**：只承载身份与模型元数据

---

### 3. 已完成能力总览

以下五项能力已完成改造，均遵循统一的 Registry 装配范式。

#### 3.1 `rule` ✅

`RuleRegistry` 采用"本地扫描 + 动态注入"双源模型，是其他 Registry 的参考模板。

- 本地目录扫描（`with_local_scan(true)` + `scan()`）
- 动态注入（`add_rules()`）
- 合并输出（`build_rule_configs()`）
- Builder 便捷 API：`with_rule_configs()` 追加到 Registry

#### 3.2 `skill` ✅

`SkillRegistry` 采用"本地扫描 + 白名单筛选"模型。

- 本地目录扫描（`with_local_scan(true)` + `scan()`）
- 白名单筛选（`with_whitelist()`）
- `build()` 时将最终 `skill_code` 集合注入 `UseSkillTool`
- Builder 便捷 API：`with_skill_whitelist()` 覆盖白名单

#### 3.3 `tool` ✅

`ToolRegistry` 采用"候选池 + 白名单"模型。

- 代码内注册候选工具（`with_tool()`）
- 白名单筛选（`with_tool_codes()`）
- Builder 便捷 API：`with_tool_whitelist()` 覆盖白名单

#### 3.4 `mcp` ✅

`McpRegistry` 采用"配置注册 → 延迟连接"两阶段模式。

- 配置注册（`add_config()` / `add_configs()`）
- 本地 manifest 扫描（`with_local_scan(true)` + `scan()`）
- 统一连接（`connect_all()`）
- Builder 便捷 API：`with_mcp_configs()` 追加到 pending 队列

#### 3.5 `command` ✅

`CommandRegistry` 采用"本地扫描 + 动态注入"双源模型，与 RuleRegistry 同形。

- 本地目录扫描（`with_local_scan(true)` + `scan()`）
- 动态注入（`add_commands()`）
- 合并输出（`build_command_configs()`）
- Frontmatter 解析（`CommandManifest::parse()`）
- Builder 便捷 API：`with_command_configs()` 追加到 Registry
- 触发放在宿主层，不进入主推理循环

---

### 4. Builder 两层 API

```rust
impl AgentRuntimeBuilder {
    // ===== 底层 API：注入已装配的 Registry 对象 =====
    pub fn with_tool_registry(self, registry: ToolRegistry) -> Self;
    pub fn with_mcp_registry(self, registry: McpRegistry) -> Self;
    pub fn with_rule_registry(self, registry: RuleRegistry) -> Self;
    pub fn with_skill_registry(self, registry: SkillRegistry) -> Self;

    pub fn with_command_registry(self, registry: CommandRegistry) -> Self;

    // ===== 便捷 API：传配置 =====
    pub fn with_rule_configs(self, configs: Vec<RuleConfig>) -> Self;
    pub fn with_skill_whitelist(self, skill_codes: Vec<String>) -> Self;
    pub fn with_tool_whitelist(self, tool_codes: Vec<String>) -> Self;
    pub fn with_mcp_configs(self, configs: Vec<McpServiceConfig>) -> Self;
    pub fn with_sub_agents(self, configs: Vec<SubAgentConfig>) -> Self;
    pub fn with_commands(self, configs: Vec<CommandConfig>) -> Self;
    pub fn with_command_configs(self, configs: Vec<CommandConfig>) -> Self;

    // ===== 通用配置 =====
    pub fn with_local_scan(self, enabled: bool) -> Self;
    pub fn with_max_turns(self, max_turns: u32) -> Self;

    pub async fn build(self) -> Result<AgentRuntime>;
}
```

叠加策略：

| 能力 | 同时使用时的行为 |
|---|---|
| **Rule** | 便捷 API 的配置通过 `add_rules()` **追加** |
| **Skill** | 便捷 API 的白名单**覆盖** Registry 的白名单 |
| **Tool** | 便捷 API 的白名单**覆盖** Registry 的白名单 |
| **MCP** | 便捷 API 的配置**追加**到 pending 队列 |
| **Command** | 便捷 API 的配置通过 `add_commands()` **追加** |

---

### 5. 本地目录约定

```text
.codebuddy/
└── agents/
    ├── main/
    │   ├── AGENT.md
    │   ├── rules/
    │   ├── skills/
    │   ├── mcp/
    │   ├── commands/
    │   └── agents/       # 子智能体递归目录
    └── reviewer/
        └── ...
```

约定：

- `AGENT.md`：Markdown + YAML frontmatter，正文即 system prompt
- `rules/`：本地规则，由 `RuleRegistry` 扫描
- `skills/`：本地技能包，由 `SkillRegistry` 扫描
- `mcp/*.json`：MCP manifest，由 `McpRegistry` 扫描
- `commands/*.md`：命令模板，由 `CommandRegistry` 扫描
- `agents/<child>/`：子智能体目录，递归解析

---

### 6. 合并策略

所有 Registry 统一采用：

- **动态配置 ⊕ 本地扫描结果** 合并
- 按能力对应的主键去重，**动态配置优先**
- 空集合 / 关闭开关时自然降级

主键约定：

| 能力 | 主键 |
|---|---|
| rule | `name` |
| skill | `skill_code` |
| tool | `tool_code` |
| mcp service | `service_name` |
| sub_agent | `alias` |
| command | `command_code` |

---

### 7. `commands` 模块设计

#### 7.1 定位

Commands 是一种面向开发场景的**指令式交互能力** ✅ 已实现，与 Skill 的核心区别：

- **Skill**：模型自主调用，注入 System Prompt，LLM 按需触发
- **Command**：用户 / 宿主显式触发，一条明确指令执行特定任务

典型场景：

- 用一条指令触发 AI 执行特定任务（如：生成文档、生成单测）
- 减少自然语言反复描述成本（如：预设好重构提示词的指令）
- 让 AI 行为更稳定、可预期、可复用（如：openspec、speckit 等固定流程）

#### 7.2 类型定义

```rust
/// 命令来源
pub enum CommandSource {
    /// 从本地文件系统加载
    Local,
    /// 在运行时动态注入
    Injected,
}

/// 命令配置
pub struct CommandConfig {
    /// 命令唯一编码（= 本地文件名不含扩展名，或数据库 command_code）
    pub command_code: String,
    /// 命令展示名（来自 COMMAND.md frontmatter `name`）
    pub name: String,
    /// 命令描述（来自 COMMAND.md frontmatter `description`）
    pub description: String,
    /// 命令提示词正文（COMMAND.md frontmatter 之后的 Markdown 正文）
    pub content: String,
    /// 命令来源
    pub source: CommandSource,
    /// 来源提示（文件路径或数据库标识）
    pub source_hint: Option<String>,
}
```

#### 7.3 `CommandRegistry`

与其他 Registry 接口同形：

```rust
impl CommandRegistry {
    pub fn new(commands_root: impl Into<PathBuf>) -> Self;
    pub fn with_local_scan(self, enable: bool) -> Self;
    pub fn add_command(&mut self, command: CommandConfig);
    pub fn add_commands(&mut self, commands: Vec<CommandConfig>);
    pub async fn scan(&mut self) -> Result<(), Error>;
    pub fn build_command_configs(&self) -> Vec<CommandConfig>;
}
```

#### 7.4 本地目录结构

```text
<commands_root>/
├── gen-doc.md          # command_code = "gen-doc"
├── gen-test.md         # command_code = "gen-test"
└── refactor.md         # command_code = "refactor"
```

每个 `.md` 文件的 frontmatter 定义命令元数据：

```md
---
name: 生成文档
description: 为指定模块生成 API 文档
---

请为以下模块生成完整的 API 文档...
```

#### 7.5 触发机制

**触发放在宿主层** ✅：`CommandRegistry` 只负责加载与注册，命令实际分发由 `agent-platform` 会话层处理，不进入 `AgentRuntime` 主推理循环。

`AgentRuntime` 持有 `command_configs: Vec<CommandConfig>`，宿主层可通过查询已注册命令列表来实现命令匹配与分发。

---

### 8. `sub_agent` 模块设计

在 `crates/core/agent-runtime/src/sub_agent/` 下新增独立模块：

```text
crates/core/agent-runtime/src/sub_agent/
├── mod.rs              # 模块声明与重导出（无逻辑）
├── types.rs            # AgentRef / SubAgentContextPolicy / SubAgentConfig
├── registry.rs         # SubAgentRegistry：本地扫描 + 动态注入 + 合并
├── resolver.rs         # 根据 AgentRef 解析子智能体 + 循环检测
├── runtime_factory.rs  # 根据解析结果产出子 AgentRuntime
└── context.rs          # 上下文继承策略执行
```

#### 8.1 `types.rs`

```rust
/// 子智能体引用
pub enum AgentRef {
    Local { agent_code: String },
    Online { agent_id: i64 },
}

/// 子智能体上下文继承策略
pub enum SubAgentContextPolicy {
    Isolated,
    InheritCurrentTurn,
    InheritSession,
}

/// 子智能体配置
pub struct SubAgentConfig {
    pub alias: String,
    pub target: AgentRef,
    pub context_policy: SubAgentContextPolicy,
    pub max_depth_override: Option<u32>,
}
```

#### 8.2 循环与深度控制

- `AgentResolveContext.visited` 维护父链路
- 每解析一个子 Agent 压入 `"local:<code>"` 或 `"online:<id>"`，重复即拒绝
- `depth` 累加，超过 `max_depth` 即拒绝

#### 8.3 上下文记忆策略

当前 `share_context: bool` 升级为三态枚举 `SubAgentContextPolicy`：

- `share_context = false` → `Isolated`
- `share_context = true`  → `InheritCurrentTurn`

`InheritSession` 必须带审计（delegation trace / parent run id / memory write source）。

---

### 9. Provider 层处理方式

**直接削减为数据查询层**：

- 删除 `CompositeAgentDefinitionProvider`
- `LocalCodeBuddyProvider` 删除，本地加载由各 Registry 自己扫描
- `DbAgentDefinitionProvider` 瘦身为数据仓储
- snapshot 的组装由宿主侧一次性完成并传入 Builder

---

### 10. 实施顺序

1. ✅ `AgentRuntimeBuilder` 新增动态配置接入方法 + 修复 `UseSkillTool` 白名单
2. ✅ `SkillRegistry / ToolRegistry / McpRegistry` 对齐同形装配契约
3. ✅ 新增 `command` 模块（types / manifest / registry）+ Builder 集成
4. ⚬ 新增 `sub_agent` 模块（types / registry / resolver / runtime_factory / context）
5. ⚬ 重构 `PlatformSubAgentScheduler`，调用统一 runtime factory
6. ⚬ 删除 `CompositeAgentDefinitionProvider` 与 `LocalCodeBuddyProvider`
7. ⚬ `DbAgentDefinitionProvider` 瘦身为数据仓储
8. ⚬ 后续按需扩展数据库 `context_policy` 字段

---

### 11. 风险点

- **不要把能力字段塞回 `snapshot`**：`snapshot` 保持只承载身份与模型元数据
- **不要把 `commands` 做成另一个 `skill`**：触发语义不同，必须独立模块
- **`InheritSession` 必须带审计**：一旦允许共享会话记忆，delegation trace / parent run id / memory write source 必须完整记录

---

### 12. 一句话总结

Registry 即装配中枢，Builder 提供两层 API（注入对象 + 传配置）可自由叠加、装配完成直接产出 `AgentRuntime`；Provider 退化为数据查询、Composite / Blueprint / AssemblySpec 全部取消；本地按 Agent 独立目录，动态配置覆盖本地；子智能体通过 `AgentRef + SubAgentContextPolicy` 统一建模，子运行时复用主链路。
