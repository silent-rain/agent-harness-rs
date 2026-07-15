## Skill Installer 重构 v2

> 版本：v2 ｜ 日期：2026-05-15 ｜ 不向后兼容

### 1. 功能说明

把原先单一的 `SkillInstaller` + `SkillInstallTool` 拆分为：

- **本地模式**：[`LocalDirInstaller`] —— 直接读写 `<skills_root>/<skill_code>/`
- **线上模式**：[`DbInstaller`] —— 通过 [`SkillPackageStore`] 写入数据库
  （`t_agent_skill_package` + `t_agent_skill_rel`）

并新增 [`SkillManagementTool`]（工具名 `skill_manage`）作为 LLM 通道，
通过 `action` 子命令统一支持 **install / update / uninstall / list / query**。

### 2. 新增内容

| 文件 | 内容 |
|------|------|
| [crates/core/agent-base/src/skill/installer/local_dir.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/installer/local_dir.rs) | `LocalDirInstaller` 本地目录安装器（install/update/uninstall/list_codes，原子覆盖） |
| [crates/core/agent-base/src/skill/installer/db.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/installer/db.rs) | `DbInstaller` 线上安装器（install/update/uninstall/list/query/find_rel） |
| [crates/core/agent-base/src/skill/tools/skill_manage.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/tools/skill_manage.rs) | `SkillManagementTool` + `SkillBackend` 统一管理工具 |
| [crates/core/agent-platform/src/loader/skill/db_store.rs](/home/one/code/nucleus-rs/crates/core/agent-platform/src/loader/skill/db_store.rs) | `DbSkillPackageStore`（SeaORM 实现） |

`installer/types.rs` 新增三个数据契约：

- `SkillPackageRecord`：`t_agent_skill_package` 行级记录（带 `id`、`user_id`、`meta`）
- `SkillRelRecord`：`t_agent_skill_rel` 行级记录
- `InstallOutcome`：`Installed` / `Updated` / `Skipped`，带 `meta()` / `is_mutated()` 辅助

`SkillPackageStore` trait 重新定义为线上专用 7 个原子方法：

```rust
async fn find_rel(user_id, agent_id, skill_code) -> Option<SkillRelRecord>;
async fn find_package(user_id, skill_code) -> Option<SkillPackageRecord>;
async fn upsert_package(user_id, pkg) -> SkillPackageRecord;
async fn link(user_id, agent_id, skill_package_id) -> SkillRelRecord;
async fn unlink(user_id, agent_id, skill_code) -> bool;
async fn list_rel(user_id, agent_id) -> Vec<SkillPackageMeta>;
async fn load_zip(package_id) -> Option<SkillPackage>;
```

### 3. 修改内容

| 文件 | 变更 |
|------|------|
| [crates/core/agent-base/src/skill/installer/store.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/installer/store.rs) | trait 完全重写；`InMemorySkillPackageStore` 跟随重写 |
| [crates/core/agent-base/src/skill/installer/types.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/installer/types.rs) | 新增 3 个领域类型 |
| [crates/core/agent-base/src/skill/installer/mod.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/installer/mod.rs) | 重导出表更新 |
| [crates/core/agent-base/src/skill/loader/local.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/loader/local.rs) | 移除 `with_db_store` / `hydrate_db_packages`，纯本地扫描；跳过 `.new` / `.old` 临时目录 |
| [crates/core/agent-base/src/skill/mod.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/mod.rs) | 重导出表更新 |
| [crates/core/agent-base/src/skill/tools/mod.rs](/home/one/code/nucleus-rs/crates/core/agent-base/src/skill/tools/mod.rs) | 模块由 `skill_install` 重命名为 `skill_manage` |
| [crates/core/agent-platform/src/loader/skill/mod.rs](/home/one/code/nucleus-rs/crates/core/agent-platform/src/loader/skill/mod.rs) | 新增 `db_store` 子模块 |

### 4. 删除内容

- `crates/core/agent-base/src/skill/installer/core.rs` —— 旧 `SkillInstaller`，被 `LocalDirInstaller` + `DbInstaller` 取代
- `crates/core/agent-base/src/skill/tools/skill_install.rs` —— 已重命名为 `skill_manage.rs`
- `LocalSkillLoader::with_db_store` —— DB 模式内容加载由 `DbSkillLoader` 单独负责，installer 与 loader 完全解耦

### 5. 注意事项

#### 5.1 update 语义为方案 B（覆盖 package）

`DbInstaller::update` 会调用 `upsert_package` 直接覆盖 `(user_id, skill_code)`
对应行的 zip 字节流、sha、version 等字段；**所有引用该 package 的 Agent 自动用上新版本**。
若需要"按 agent 独立换版本"，请改为调用 `uninstall` + `install`。

#### 5.2 工具的实例化模式

`SkillManagementTool` 不会注册到全局 tool pool，而是在 **构建主 agent 时按需注入**：

```text
全局共享：Arc<DbInstaller> + Arc<dyn SkillPackageStore>
                │
        ┌───────┴───────┐
        ▼               ▼
  agent_id=A         agent_id=B
  SkillManagementTool::db(installer.clone(), user_id, A)
```

子 agent / worker 不挂这个工具。

#### 5.3 安全策略

- LLM 通道必须配置 `with_allowed_path_prefixes(...)` 白名单
- 白名单为空时 `install` / `update` 直接拒绝
- `SkillSource::parse` 仍然拒绝远端协议（http/https/git+/npm:）

#### 5.4 LocalDirInstaller 的原子覆盖

`update` 会创建 `<skill_code>.new` 临时目录，再 rename 到位；
`LocalSkillLoader.load()` 会跳过所有 `.new` / `.old` / 以 `.` 开头的目录，
确保扫描期间读到一致状态。

### 6. 使用示例

#### 示例 1：本地模式 —— CLI 启动时自助安装一个 skill

```rust
use std::sync::Arc;
use agent_base::skill::{LocalDirInstaller, SkillSource};

# async fn example() -> anyhow::Result<()> {
let installer = Arc::new(LocalDirInstaller::new(".codebuddy/skills"));
let meta = installer
    .install(
        Some("my-skill".to_string()),
        SkillSource::LocalDir("/abs/path/to/staging/my-skill".into()),
    )
    .await?;
println!("installed: {} v{}", meta.skill_code, meta.version);
# Ok(())
# }
```

#### 示例 2：线上模式 —— 主 agent 装配 `skill_manage` 工具

```rust
use std::sync::Arc;
use agent_base::skill::{
    DbInstaller, SkillBackend, SkillManagementTool, SkillPackageStore,
};
use agent_platform::loader::skill::DbSkillPackageStore;
use sea_orm::DatabaseConnection;

# fn example(db: DatabaseConnection, user_id: i32, agent_id: i64) {
// 全局只构造一次
let store: Arc<dyn SkillPackageStore> = Arc::new(DbSkillPackageStore::new(db));
let installer = Arc::new(DbInstaller::new(store));

// 构建主 agent 时按需注入
let tool = SkillManagementTool::db(installer.clone(), user_id, agent_id)
    .with_allowed_path_prefixes(vec![std::path::PathBuf::from("/srv/skills/staging")]);

// 把 `tool` 加入主 agent 的 tools 列表（伪代码）
// agent_builder.add_tool(tool);
# }
```

#### 示例 3：LLM 通过 `skill_manage` 卸载技能

```json
{
  "name": "skill_manage",
  "args": {
    "action": "uninstall",
    "skill_code": "my-skill"
  }
}
```

返回值（线上模式）：

```json
{
  "result": "uninstalled",
  "skill_code": "my-skill",
  "removed": true
}
```

> 注意：线上模式下 `uninstall` 仅解除 (user, agent) 与该 skill 的绑定，
> 不删除 `t_agent_skill_package` 中的物理记录（其他 agent 可能仍在使用）。

### 7. 兼容性说明

- **不向后兼容**：旧 `SkillInstaller` / `SkillInstallTool` 全部移除
- **API 调用方影响**：所有原引用 `SkillInstaller::*` 的代码必须改为 `LocalDirInstaller`
  或 `DbInstaller`；原 `with_db_store` 调用必须改为接 `DbSkillLoader`
- **数据库表结构**：未变更，沿用现有 `t_agent_skill_package` / `t_agent_skill_rel`
- **测试覆盖**：21 个 installer 单测 + 4 个工具单测 + 5 个 loader 单测，全部通过

### 8. 涉及组件

```text
agent-base
├── skill/installer/      ← 本次重构核心
│   ├── local_dir.rs      ★ 新增
│   ├── db.rs             ★ 新增
│   ├── store.rs          ✎ 重写
│   ├── types.rs          ✎ 新增类型
│   └── mod.rs            ✎ 重导出
├── skill/loader/local.rs ✎ 解耦 db_store
├── skill/tools/
│   ├── skill_manage.rs   ★ 改名 + 重写（原 skill_install.rs）
│   └── mod.rs            ✎ 重导出
└── skill/mod.rs          ✎ 重导出

agent-platform
└── loader/skill/
    ├── db_store.rs       ★ 新增（SeaORM 实现）
    └── mod.rs            ✎ 重导出
```
