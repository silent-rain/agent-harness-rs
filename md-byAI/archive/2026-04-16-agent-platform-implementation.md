# agent-platform — 智能体平台装配层

## 1. 功能说明

`agent-platform` 是智能体平台的配置装配层，负责：

- 从数据库 / 本地 `.codebuddy` 目录加载智能体配置
- 将配置装配为 `AgentDefinitionSnapshot`
- 将快照组装为可运行的 `AgentRuntime`

本 crate 是 `agent-runtime`（运行时基座）与上层服务（`agent-service`）之间的桥梁，
实现了"配置来源与运行时解耦"的设计目标。

---

## 2. 新增内容

### 2.1 文件结构

```text
crates/core/agent-platform/
├── Cargo.toml
├── src/
│   ├── lib.rs                      # 公共 API 导出
│   ├── error.rs                    # PlatformError 错误类型
│   ├── resolver.rs                 # AgentRuntimeResolver
│   └── provider/
│       ├── mod.rs                  # AgentDefinitionProvider trait
│       ├── db_provider.rs          # DbAgentDefinitionProvider（数据库）
│       ├── local_provider.rs       # LocalCodeBuddyProvider（本地文件）
│       └── composite.rs            # CompositeAgentDefinitionProvider（融合）
└── tests/
    └── integration_test.rs         # 集成测试（6 个测试用例）
```

### 2.2 核心类型

| 类型 | 说明 |
|------|------|
| `AgentDefinitionProvider` | 配置提供者 trait |
| `DbAgentDefinitionProvider` | 从数据库装配快照 |
| `LocalCodeBuddyProvider` | 从 `.codebuddy` 目录加载配置 |
| `CompositeAgentDefinitionProvider` | 融合多个来源的配置 |
| `AgentRuntimeResolver` | 将快照装配为 `AgentRuntime` |
| `ResolverConfig` | 解析器配置 |
| `PlatformError` | 平台层错误类型 |

---

## 3. 修改内容

### 3.1 `agent-runtime/src/domain/mod.rs`

- `WorkspaceRuntimePolicy` 添加 `Default` derive，支持 `unwrap_or_default()`

### 3.2 `Cargo.toml`（workspace）

- 注册 `agent-platform` crate
- 添加 `agent-platform` workspace 依赖声明

---

## 4. 注意事项

### 4.1 配置来源优先级

| 模式 | 优先级 |
|------|--------|
| `local_first` | 本地 `.codebuddy` > 数据库 |
| `db_only` | 仅数据库 |
| `merged` | 本地为主，数据库补充缺失字段 |

### 4.2 `.codebuddy` 目录约定

```text
.codebuddy/
└── agents/
    ├── 1.toml          # 按 agent_id 命名（优先）
    └── 1.json          # 或 JSON 格式
```

### 4.3 数据库查询涉及的表

| 表名 | 用途 |
|------|------|
| `t_agent_config` | 智能体基础信息 |
| `t_agent_model_binding` | 模型绑定配置 |
| `t_agent_llm_model` | LLM 模型定义 |
| `t_agent_workspace` | 工作区路径配置 |
| `t_agent_tool_rel` | 工具绑定关系 |
| `t_agent_tool_registry` | 工具注册表 |

### 4.4 工具注册（Phase 5 前的限制）

当前 `AgentRuntimeResolver` 构建的 `AgentFunctionRegistry` 为空注册表。
内置工具（`read_file`、`list_dir` 等）需由上层服务手动注册。
Phase 5 将实现根据 `snapshot.tools` 自动注册工具执行器。

---

## 5. 使用示例

### 示例 1：从本地 `.codebuddy` 加载并运行

```rust
use agent_platform::{
    AgentDefinitionProvider, AgentRuntimeResolver,
    LocalCodeBuddyProvider,
};
use agent_runtime::{AgentPlanner, planner::{PlannerDecision, PlannerInput}};
use std::sync::Arc;

// 1. 创建本地提供者（自动探测当前目录下的 .codebuddy）
let provider = LocalCodeBuddyProvider::from_cwd();

// 2. 加载快照
let snapshot = provider.load(1, user_id).await?.unwrap();

// 3. 装配运行时
let planner: Arc<dyn AgentPlanner> = Arc::new(MyPlanner::new());
let resolver = AgentRuntimeResolver::with_planner(planner);
let runtime = resolver.resolve(snapshot)?;

// 4. 运行
let answer = runtime.run("session-1", "帮我分析这段代码").await?;
println!("{}", answer);
```

### 示例 2：从数据库加载（在线模式）

```rust
use agent_platform::{
    AgentDefinitionProvider, AgentRuntimeResolver,
    DbAgentDefinitionProvider,
};
use sea_orm::Database;

let db = Database::connect("sqlite://./data.db").await?;
let provider = DbAgentDefinitionProvider::new(db);

let snapshot = provider.load(agent_id, user_id).await?.unwrap();
let resolver = AgentRuntimeResolver::with_planner(planner);
let runtime = resolver.resolve(snapshot)?;
```

### 示例 3：本地优先 + 数据库回退（推荐生产用法）

```rust
use agent_platform::{
    AgentDefinitionProvider, AgentRuntimeResolver,
    CompositeAgentDefinitionProvider,
    DbAgentDefinitionProvider, LocalCodeBuddyProvider,
};
use std::sync::Arc;

let local = Arc::new(LocalCodeBuddyProvider::from_cwd());
let db = Arc::new(DbAgentDefinitionProvider::new(db_conn));

// 本地有配置时优先使用，否则回退到数据库
let composite = CompositeAgentDefinitionProvider::local_first(local, db);

let snapshot = composite.load(agent_id, user_id).await?;
if let Some(snapshot) = snapshot {
    let runtime = resolver.resolve(snapshot)?;
    let answer = runtime.run(session_id, user_input).await?;
}
```

### 示例 4：`.codebuddy/agents/1.toml` 配置文件格式

```toml
[agent]
user_id = 1
name = "代码助手"
description = "专注于 Rust 代码分析与优化"
system_prompt = "你是一个专业的 Rust 开发助手，擅长代码审查和性能优化。"
welcome_message = "你好！我是你的 Rust 代码助手，有什么可以帮你的？"

[model]
provider = "openai"
model_code = "gpt-4o"
temperature = 0.2
max_tokens = 4096
max_turns = 20

[workspace]
exec_root_path = "/home/user/my-project"
terminal_enabled = true
file_write_enabled = false
delete_enabled = false

[[tools]]
tool_code = "read_file"
name = "读取文件"

[[tools]]
tool_code = "list_dir"
name = "列出目录"
```

---

## 6. 兼容性说明

- `agent-platform` 依赖 `agent-runtime`，但 `agent-runtime` 不依赖 `agent-platform`
- `DbAgentDefinitionProvider` 需要 SeaORM `DatabaseConnection`，不影响离线场景
- `LocalCodeBuddyProvider` 无外部依赖，可在任何环境使用
- `rig-planner` feature 可选，不启用时不引入 `rig-core` 依赖
