# Skill 三阶段加载改造

## 功能说明

将 Skill 系统从"全量注入 System Prompt"改造为"三阶段按需加载"模式，大幅降低 token 消耗，提升 LLM 注意力精确度。

### 三阶段加载模型

1. **启动时（阶段 1）**：仅将 skill 的 `name` + `description` 摘要注入 System Prompt（表格形式）
2. **命中时（阶段 2）**：LLM 调用 `use_skill` 工具按需加载完整 SKILL.md 正文 + 目录资源清单
3. **执行时（阶段 3）**：LLM 按 SKILL.md 指令自主调用 `read_file`、`run_command` 等已有工具读取引用资源

### Token 开销对比

| 维度 | 改造前（全量注入） | 改造后（三阶段） |
|---|---|---|
| 10 个 skill 的 System Prompt 开销 | ~30,000 tokens | ~500 tokens（摘要）|
| 单次命中加载 | 0（已全量注入） | ~3,000 tokens（按需加载 1 个） |
| LLM 注意力分散 | 严重 | 最小化 |

## 新增内容

### 1. `UseSkillTool` — 技能按需加载工具

**文件**: `crates/core/agent-base/src/tools/use_skill.rs`

实现 `rig::Tool` trait，当 LLM 判断用户需求匹配某个 skill 时调用。

功能：

- 根据 `skill_code` 定位本地 skill 目录
- 读取 `SKILL.md` 完整正文（不含 frontmatter）
- 递归扫描目录内可用资源文件（`references/`、`assets/` 等）
- 返回正文内容 + 资源文件列表

```rust
use agent_base::tools::UseSkillTool;

// 创建工具实例
let tool = UseSkillTool::new(
    ".codebuddy/skills",
    vec!["service-hub-generator".to_string(), "rust-expert".to_string()],
);

// LLM 调用时
let output = tool.call(UseSkillArgs { skill_code: "rust-expert".to_string() }).await?;
// output.content = SKILL.md 完整正文
// output.resource_files = ["references/guide.md", "assets/scaffold.sh"]
```

### 2. `t_agent_skill_rel` 表增加 `name`、`description` 字段

**文件**: `crates/core/migration/src/agent/skill_rel.rs`、`crates/core/entity/src/agent/skill_rel.rs`

新增字段：

- `name` (VARCHAR 256, NOT NULL, DEFAULT '') — 技能名称，安装时从 SKILL.md frontmatter 提取
- `description` (TEXT, NOT NULL, DEFAULT '') — 技能描述，安装时从 SKILL.md frontmatter 提取

## 修改内容

### 1. `AgentRuntimeBuilder::build()` — 自动注册 `use_skill` 工具

**文件**: `crates/core/agent-base/src/runtime/builder.rs`

- `load_skills()` 重命名为 `load_skill_summaries()`，仅加载摘要（`content` 清空）
- 新增 `resolve_skills_root()` 方法提取 skills 根目录路径
- `build()` 中当有可用 skill 时自动注册 `UseSkillTool` 到 ToolSet

### 2. `AgentRuntime::build_skill_section()` — 改为摘要表格注入

**文件**: `crates/core/agent-base/src/runtime/agent_runtime.rs`

- 不再注入完整 SKILL.md 正文（`### <name>\n\n<content>`）
- 改为注入摘要表格（`| 技能编码 | 名称 | 描述 |`）
- 引导文本提示 LLM 使用 `use_skill` 工具加载完整指令

### 3. `DbAgentDefinitionProvider::load_skills()` — 从 DB 加载 name/description

**文件**: `crates/core/agent-platform/src/provider/db_provider.rs`

- 从 `t_agent_skill_rel` 的 `name`、`description` 字段读取并填充到 `SkillConfig`

## 注意事项

1. **数据库迁移**：需要执行 migration 为 `t_agent_skill_rel` 表添加 `name` 和 `description` 字段
2. **Provider 层不受影响**：`LocalCodeBuddyProvider` 仍然扫描完整 SKILL.md 内容，摘要化在 builder 层完成
3. **`use_skill` 工具不受白名单约束**：它是在 builder 中直接注册到 ToolSet 的，不经过 `tool_configs` 白名单
4. **资源文件按需读取**：SKILL.md 中引用的 `references/` 和 `assets/` 文件由 LLM 使用已有的 `read_file`、`run_command` 工具自行读取

## 使用示例

### 示例 1：System Prompt 中的 Skill 摘要（阶段 1）

```markdown
## Skills

以下是你具备的专项能力。当用户需求匹配某个技能时，
请先调用 `use_skill` 工具加载该技能的完整指令后再执行。

| 技能编码 | 名称 | 描述 |
|---|---|---|
| service-hub-generator | Service Hub Generator | Service Hub 服务开发规范技能 |
| rust-expert | Rust Expert | Rust 编程专家指令 |
```

### 示例 2：LLM 调用 use_skill 加载完整指令（阶段 2）

```json
{
  "name": "use_skill",
  "arguments": { "skill_code": "service-hub-generator" }
}
```

返回：

```json
{
  "skill_code": "service-hub-generator",
  "name": "Service Hub Generator",
  "content": "# Service Hub Generator\n\n## 执行步骤\n\n1. 读取 references/project-structure.md...",
  "resource_files": ["references/project-structure.md", "references/api-spec.md", "assets/scaffold.sh"]
}
```

### 示例 3：LLM 按指令读取引用资源（阶段 3）

```json
{
  "name": "read_file",
  "arguments": { "path": ".codebuddy/skills/service-hub-generator/references/project-structure.md" }
}
```

## 兼容性说明

- **不向后兼容**：`build_skill_section` 输出格式从 H3 标题改为表格，测试已同步更新
- **数据库变更**：`t_agent_skill_rel` 新增 `name`、`description` 字段（NOT NULL DEFAULT ''），已有数据不受影响
- **工具集变更**：当有可用 skill 时，ToolSet 中会自动多出一个 `use_skill` 工具
