# Phase 5：Rules、Skills、MCP 与本地资源协同实现记录

## 功能说明

Phase 5 为智能体运行时补齐了**规则（Rules）**、**技能（Skills）**、**MCP 服务绑定**和**子智能体绑定**能力，使 Agent 能够：

- 从数据库加载并注入规则（Rules）到 System Prompt
- 从数据库加载并注入技能（Skills）到 System Prompt（支持摘要/完整两种注入模式）
- 绑定 MCP 服务并配置白名单工具
- 绑定子智能体并配置委派策略
- 从 `.codebuddy/sub-agents/` 目录加载本地子智能体描述文件
- 多来源配置（数据库 + 本地）融合时正确合并 mcp_services 和 sub_agents

---

## 新增内容

### 1. 数据库表（Migration）

#### `t_agent_rule_registry` — 规则注册表

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 规则ID |
| `user_id` | BIGINT | 所属用户ID（0=平台内建） |
| `rule_code` | VARCHAR(128) | 规则唯一编码 |
| `name` | VARCHAR(128) | 规则名称 |
| `description` | TEXT | 规则描述 |
| `content` | TEXT | 规则内容（注入 System Prompt 的文本） |
| `priority` | INT | 注入优先级（越大越靠前） |
| `source_type` | SMALLINT | 来源类型: 0=builtin, 1=user, 2=local_file |
| `status` | SMALLINT | 状态: 0=disabled, 1=active |
| `deleted` | BOOLEAN | 是否删除 |
| `created_at` | DATETIME | 创建时间 |
| `updated_at` | DATETIME | 更新时间 |

#### `t_agent_rule_rel` — 智能体与规则绑定关系表

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 关系ID |
| `agent_id` | BIGINT | 智能体ID |
| `rule_id` | BIGINT | 规则ID |
| `priority` | INT | 注入优先级覆盖 |
| `enabled` | BOOLEAN | 是否启用 |
| `created_at` | DATETIME | 创建时间 |

#### `t_agent_skill_registry` — 技能注册表

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 技能ID |
| `user_id` | BIGINT | 所属用户ID（0=平台内建） |
| `skill_code` | VARCHAR(128) | 技能唯一编码 |
| `name` | VARCHAR(128) | 技能名称 |
| `description` | TEXT | 技能描述（摘要，注入 Prompt 时使用） |
| `content` | TEXT | 技能完整内容（按需展开时使用） |
| `inject_mode` | SMALLINT | 注入模式: 0=summary, 1=full |
| `source_type` | SMALLINT | 来源类型: 0=builtin, 1=user, 2=local_file |
| `status` | SMALLINT | 状态: 0=disabled, 1=active |
| `deleted` | BOOLEAN | 是否删除 |
| `created_at` | DATETIME | 创建时间 |
| `updated_at` | DATETIME | 更新时间 |

#### `t_agent_skill_rel` — 智能体与技能绑定关系表

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 关系ID |
| `agent_id` | BIGINT | 智能体ID |
| `skill_id` | BIGINT | 技能ID |
| `inject_mode` | SMALLINT | 注入模式覆盖（null=使用技能默认值） |
| `enabled` | BOOLEAN | 是否启用 |
| `created_at` | DATETIME | 创建时间 |

#### `t_agent_mcp_service_rel` — 智能体 MCP 服务绑定关系表

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 关系ID |
| `agent_id` | BIGINT | 智能体ID |
| `mcp_service_name` | VARCHAR(128) | MCP 服务名称 |
| `mcp_service_url` | VARCHAR(512) | MCP 服务地址（stdio 类型为空） |
| `transport_type` | VARCHAR(32) | 传输类型: stdio, sse, http |
| `allowed_tools` | TEXT | 白名单工具列表（JSON 数组，null=全部允许） |
| `config_json` | TEXT | MCP 服务配置（JSON） |
| `enabled` | BOOLEAN | 是否启用 |
| `created_at` | DATETIME | 创建时间 |
| `updated_at` | DATETIME | 更新时间 |

#### `t_agent_sub_agent_rel` — 智能体子智能体绑定关系表

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT PK | 关系ID |
| `agent_id` | BIGINT | 主智能体ID |
| `sub_agent_id` | BIGINT | 子智能体ID |
| `alias` | VARCHAR(64) | 子智能体别名 |
| `description` | TEXT | 委派描述 |
| `max_depth` | INT | 最大递归委派深度（默认 3） |
| `enabled` | BOOLEAN | 是否启用 |
| `created_at` | DATETIME | 创建时间 |

### 2. Entity（SeaORM）

- `entity::agent::rule_registry::Model` — 规则注册表实体
- `entity::agent::rule_rel::Model` — 规则绑定关系实体
- `entity::agent::skill_registry::Model` — 技能注册表实体
- `entity::agent::skill_rel::Model` — 技能绑定关系实体
- `entity::agent::mcp_service_rel::Model` — MCP 服务绑定关系实体
- `entity::agent::sub_agent_rel::Model` — 子智能体绑定关系实体

### 3. agent-base：AgentDefinitionSnapshot 扩展

新增两个字段：

```rust
pub struct AgentDefinitionSnapshot {
    // ... 已有字段 ...
    /// MCP 服务绑定列表
    pub mcp_services: Vec<McpServiceConfig>,
    /// 子智能体绑定列表
    pub sub_agents: Vec<SubAgentConfig>,
}

/// MCP 服务配置
pub struct McpServiceConfig {
    pub service_name: String,
    pub service_url: Option<String>,
    pub transport_type: String,
    pub allowed_tools: Vec<String>,
    pub config_json: Option<String>,
}

/// 子智能体配置
pub struct SubAgentConfig {
    pub sub_agent_id: i64,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub max_depth: i32,
}
```

---

## 修改内容

### `agent-platform/src/provider/db_provider.rs`

新增 4 个查询方法：

| 方法 | 说明 |
|------|------|
| `load_rules()` | 查询已启用的规则，按优先级降序排列 |
| `load_skills()` | 查询已启用的技能，支持注入模式覆盖 |
| `load_mcp_services()` | 查询已启用的 MCP 服务绑定 |
| `load_sub_agents()` | 查询已启用的子智能体绑定 |

`assemble_snapshot` 函数签名更新，接受 rules/skills/mcp_services/sub_agents 参数。

### `agent-platform/src/provider/local_provider.rs`

- 新增 `.codebuddy/sub-agents/` 目录支持
- `build_snapshot` 函数新增 `sub_agents` 参数
- 本地 sub-agents 文件转换为 `SubAgentConfig`（`sub_agent_id=0`，仅描述性）

### `agent-platform/src/provider/composite.rs`

`merge_snapshots` 函数新增融合逻辑：

- `mcp_services` 取并集（以 `service_name` 去重）
- `sub_agents` 取并集（以 `sub_agent_id` 去重，`id=0` 的本地描述性条目跳过）

---

## 注意事项

1. **技能注入模式**：`inject_mode=0` 使用 `description` 字段（摘要），`inject_mode=1` 使用 `content` 字段（完整内容）。`skill_rel` 中的 `inject_mode` 可覆盖注册表默认值。

2. **MCP 服务白名单**：`allowed_tools` 为 JSON 数组，`null` 表示允许该 MCP 服务的全部工具。MCP 工具的实际调用在 Phase 6 实现。

3. **子智能体委派**：`sub_agent_rel` 仅记录绑定关系，实际委派调度在 Phase 6 实现。`max_depth` 用于防止无限递归委派。

4. **本地 sub-agents 文件**：`.codebuddy/sub-agents/` 下的文件仅提供描述性内容（`sub_agent_id=0`），在 `CompositeProvider` 融合时会被跳过（只合并有真实 ID 的条目）。

5. **规则优先级**：数据库中规则按 `priority` 降序排列（越大越靠前），本地文件按文件名字典序排列。

---

## 使用示例

### 示例 1：为智能体绑定规则

```rust
// 在数据库中创建规则
let rule = rule_registry::ActiveModel {
    user_id: Set(user_id),
    rule_code: Set("code_review_rule".to_string()),
    name: Set("代码审查规则".to_string()),
    content: Set("你必须在回答中指出代码的潜在问题和改进建议。".to_string()),
    priority: Set(100),
    source_type: Set(1),
    status: Set(1),
    deleted: Set(false),
    ..Default::default()
};
let rule_model = rule_registry::Entity::insert(rule).exec(&db).await?;

// 绑定到智能体
let rel = rule_rel::ActiveModel {
    agent_id: Set(agent_id),
    rule_id: Set(rule_model.last_insert_id),
    priority: Set(0),
    enabled: Set(true),
    ..Default::default()
};
rule_rel::Entity::insert(rel).exec(&db).await?;
```

### 示例 2：绑定 MCP 服务（白名单工具）

```rust
let mcp_rel = mcp_service_rel::ActiveModel {
    agent_id: Set(agent_id),
    mcp_service_name: Set("filesystem".to_string()),
    mcp_service_url: Set(None),
    transport_type: Set("stdio".to_string()),
    // 只允许 read_file 和 list_directory 两个工具
    allowed_tools: Set(Some(r#"["read_file","list_directory"]"#.to_string())),
    config_json: Set(None),
    enabled: Set(true),
    ..Default::default()
};
mcp_service_rel::Entity::insert(mcp_rel).exec(&db).await?;
```

### 示例 3：本地 `.codebuddy` 目录结构

```text
.codebuddy/
├── agents/
│   └── 1001          # 智能体 ID=1001 的 system prompt
├── rules/
│   ├── 01-safety.md  # 安全规则
│   └── 02-format.md  # 格式规则
├── skills/
│   └── code-review.md # 代码审查技能
└── sub-agents/
    └── search-agent.md # 搜索子智能体描述
```

加载后，`AgentDefinitionSnapshot` 将包含：

- `rules`: 2 条（来自 `rules/` 目录）
- `skills`: 1 条（来自 `skills/` 目录）
- `sub_agents`: 1 条（来自 `sub-agents/` 目录，`sub_agent_id=0`）

---

## 兼容性说明

- `AgentDefinitionSnapshot` 新增了 `mcp_services` 和 `sub_agents` 字段，所有现有的快照构建代码需要补充这两个字段（默认为空 `vec![]`）
- `DbAgentDefinitionProvider::load()` 现在会自动查询 rules/skills/mcp/sub-agents，无需额外调用
- `LocalCodeBuddyProvider` 新增 `sub-agents/` 目录支持，不影响已有的 `rules/` 和 `skills/` 加载
- `CompositeProvider` 的融合逻辑向后兼容，新字段取并集不影响已有字段的融合行为
