---
title: Skill 内置安装模式 v1
date: 2026-05-12
scope: agent-base, agent-service, entity, migration
audience: Rust 开发人员
---

# Skill 内置安装模式 v1

## 1. 功能说明

为智能体平台提供"内置安装通道"，允许 **CLI** 与 **LLM Agent** 通过统一的安装器
（`SkillInstaller`）将一个 Skill 包持久化到数据库，运行时由 `SkillRegistry` 在
扫描阶段惰性解压到本地缓存目录，并与本地 `.codebuddy/skills/` 目录中的 Skill
**联合加载**。

设计要点：

- **单一持久化模型**：`t_agent_skill_package` 以 zip 字节流存储完整包，sha256 校验
  完整性
- **三层加载优先级**：本地目录（dev override）> DB 已安装 > Builtin
- **来源协议可扩展**：当前内置仅支持本地目录与字节流，远端协议（git/npm/zip URL）
  由外层 fetcher 转换后再写入
- **安全可控**：`SkillInstallTool`（LLM 通道）必须配置受信任路径前缀白名单，
  否则拒绝所有调用，防 prompt-injection 装入恶意目录

## 2. 新增内容

### 2.1 实体与迁移

| 文件 | 说明 |
|---|---|
| `crates/core/entity/src/agent/skill_package.rs` | `t_agent_skill_package` SeaORM 实体 |
| `crates/core/migration/src/agent/skill_package.rs` | 迁移定义，注册到 `MigratorTrait::migrations()` |

关键字段：

- `skill_code` (UNIQUE)：与解压后顶层目录名一致
- `content_zip` (BLOB)：完整 zip 字节流
- `content_sha256` (CHAR(64))：内容指纹，缓存命中判断
- `source_type`：复用 `enums::skill_rel::SkillSourceType`
- `installed_by`：安装者 user_id，0 表示系统/启动安装
- `status`：`SkillPackageStatus`（Installed/Disabled/Broken）

### 2.2 agent-base 新模块 `skill::installer`

| 子模块 | 职责 |
|---|---|
| `types.rs` | `SkillPackage` / `SkillPackageMeta` / `SkillPackageStatus` |
| `source.rs` | `SkillSource`：`LocalDir(PathBuf)` / `InlineZip { bytes, .. }` |
| `zip_codec.rs` | 打包 / 解包 / sha256 / zip-slip 防御 |
| `store.rs` | `SkillPackageStore` trait + `InMemorySkillPackageStore` |
| `installer.rs` | `SkillInstaller`：source → zip → 校验 → store.upsert |

### 2.3 内置 LLM 工具 `skill_install`

`crates/core/agent-base/src/skill/tools/skill_install.rs`

- 仅支持 `SkillSource::LocalDir`
- 必须通过 `with_allowed_path_prefixes()` 显式启用，空白名单时拒绝所有调用
- 输出：`{ skill_code, name, version, size_bytes, content_sha256 }`

### 2.4 `SkillRegistry` 扩展

`crates/core/agent-base/src/skill/registry.rs`

- `with_db_source(Arc<dyn SkillPackageStore>)`：接入数据库来源
- `scan()` 三阶段：DB hydrate → 本地扫描 → DB cache 扫描（同 code 时本地优先）
- 缓存目录：`<skills_root>/.skill_packages/<skill_code>@<sha_short>/`

### 2.5 agent-service DAO + SeaORM Store

| 文件 | 说明 |
|---|---|
| `service-hub/agent-service/src/dao/skill_package.rs` | `SkillPackageDao` + `SeaOrmSkillPackageStore`（实现 `SkillPackageStore`）|

## 3. 修改内容

| 文件 | 修改 |
|---|---|
| `crates/core/agent-base/Cargo.toml` | 新增 `zip` / `sha2` / `hex` |
| `crates/core/agent-base/src/skill/mod.rs` | 重导出 installer 与 `SkillInstallTool` |
| `crates/core/agent-base/src/skill/tools/mod.rs` | 注册 `skill_install` |
| `crates/core/entity/src/agent/mod.rs` | 注册 `skill_package` 实体 |
| `crates/core/migration/src/lib.rs` | 加入 skill_package 迁移 |
| `service-hub/agent-service/src/lib.rs` | 重导出 `SkillPackageDao` / `SeaOrmSkillPackageStore` |

## 4. 注意事项

1. **包大小上限**：默认 `DEFAULT_SKILL_PACKAGE_MAX_BYTES`，超过将拒绝安装；
   修改请通过 `SkillInstaller::with_max_bytes()`
2. **zip-slip 防御**：所有解压条目通过 `Component::Normal` 校验，禁止绝对路径与 `..`
3. **缓存目录** `<skills_root>/.skill_packages/` 在本地扫描阶段被显式跳过，
   避免与用户手工放置的 skill 冲突
4. **agent-service 的 Service / Controller / Router**：本期未实现，留作下一批
5. **`installed_by = 0`** 表示系统/CLI；正常安装应填入用户上下文 ID

## 5. 使用示例

### 示例 1：CLI / 后台代码直接安装本地目录

```rust,no_run
use std::sync::Arc;
use agent_base::skill::{SkillInstaller, SkillSource, SkillPackageStore};
use agent_service::SeaOrmSkillPackageStore;

# async fn run(dao_store: Arc<SeaOrmSkillPackageStore>) -> anyhow::Result<()> {
let store: Arc<dyn SkillPackageStore> = dao_store;
let installer = SkillInstaller::new(store);

let meta = installer
    .install(
        Some("web-design-guidelines".to_string()),
        SkillSource::LocalDir("/tmp/agent-skills/web-design-guidelines".into()),
        /* installed_by */ 0,
    )
    .await?;
println!("installed: {} v{}", meta.skill_code, meta.version);
# Ok(()) }
```

### 示例 2：将 `skill_install` 作为 LLM 工具暴露

```rust,no_run
use std::path::PathBuf;
use std::sync::Arc;
use agent_base::skill::{SkillInstallTool, SkillPackageStore};

# fn build(store: Arc<dyn SkillPackageStore>) {
let tool = SkillInstallTool::with_store(store)
    .with_allowed_path_prefixes(vec![
        PathBuf::from("/var/lib/nucleus/skill-staging"),
    ]);
// 注入到 Agent runtime 的工具集中
# }
```

### 示例 3：`SkillRegistry` 联合加载本地 + DB

```rust,no_run
use std::sync::Arc;
use agent_base::skill::{SkillRegistry, SkillPackageStore};

# async fn build(store: Arc<dyn SkillPackageStore>) -> anyhow::Result<()> {
let mut registry = SkillRegistry::new(".codebuddy/skills")
    .with_local_scan(true)
    .with_db_source(store);
registry.scan().await?;
let configs = registry.build_skill_configs();
# Ok(()) }
```

## 6. 兼容性说明

- **数据库**：新增 `t_agent_skill_package` 表，不修改现存表，存量部署只需运行
  增量迁移
- **API**：`SkillRegistry` 与 `SkillInstaller` 全部为新增类型，原有
  `SkillRegistry::scan/build_skill_configs` 行为不变；未配置 `with_db_source`
  时与原版完全等价
- **依赖**：新引入 `zip` / `sha2` / `hex`（均为成熟 crate），与 workspace 其他
  使用者一致
