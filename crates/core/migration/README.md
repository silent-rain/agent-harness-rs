# Migration

基于 SeaORM Migration 的 ORM 迁移模块，通过 Rust 代码定义数据库的**表结构、数据初始化和版本化迁移**。

## Crate 结构设计

本 crate 包含三部分核心内容，每部分职责清晰、组织方式依据其增长特征各有不同：

```text
migration/src/
├── lib.rs                       # Migrator，统一注册所有 Migration
├── main.rs                      # CLI 入口
├── utils.rs                     # 公共工具（索引、触发器等）
│
├── schema/                      # 【1】表定义（DDL：建表、索引等）
│   ├── mod.rs                   #   按依赖顺序注册所有建表迁移
│   └── agent/                   #   按业务域分组
│       ├── mod.rs
│       ├── agent_config.rs      #   t_agent_config 建表
│       └── agent_session.rs     #   t_agent_session 建表
│
├── seed/                        # 【2】数据初始化（DML：INSERT 初始数据）
│   ├── mod.rs                   #   导出所有 seed Migration
│   └── s_agent_tool_registry.rs #   工具注册表预置数据
│
└── migration/                   # 【3】版本化迁移（表结构变更、数据迁移）
    └── mod.rs                   #   导出所有业务域（当前无版本化迁移）
```

### 设计原则

| 目录         | 职责            | 增长模式           | 组织方式           | 命名规则                         |
| ------------ | --------------- | ------------------ | ------------------ | -------------------------------- |
| `schema/`    | 建表（DDL）     | 线性增长，新表才加 | **按业务域子目录** | 与表名一致，如 `agent_config.rs` |
| `seed/`      | 初始数据（DML） | 收敛，极少增长     | **平铺**           | `s_` 前缀，如 `s_agent_tool_registry.rs` |
| `migration/` | 版本化迁移      | 持续增长，只增不减 | **按业务域子目录** | `m{日期}_{序号}_{描述}.rs`       |

### 为什么三者组织方式不同？

1. **`schema/` 保留子目录** — 表与文件一一对应，随业务扩展持续增长，按业务域归类是必要的。
2. **`seed/` 平铺** — 不是每张表都需要 seed（缓存表、关联表等不需要），数量天花板很低（3~8 个），一个文件可能覆盖多张表，子目录只会增加无意义的嵌套深度。
3. **`migration/` 保留子目录** — 版本化迁移只增不减，长期积累可能达数十甚至上百个文件，按业务域分子目录是必要的。

### 建表写法：entity-first

建表迁移不手写列定义，直接由实体派生：

```rust
let db = manager.get_connection();
// 列类型/默认值/唯一约束/索引/注释均由实体属性派生
db.get_schema_builder().register(AgentConfig).apply(db).await?;
// 实体属性覆盖不到的联合索引再手动补（幂等）
if_not_exists_create_index(manager, AgentConfig, vec![...]).await?;
```

### Migrator 注册顺序

在 `lib.rs` 中，Migration 的注册顺序决定了执行顺序，**必须严格遵守**：

```text
schema → seed → migration
```

- **schema** 先执行，确保表结构存在
- **seed** 再执行，在空表中插入初始数据
- **migration** 最后执行，应用后续的增量变更

```rust
// lib.rs
fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    let mut migrations: Vec<Box<dyn MigrationTrait>> = vec![];
    migrations.extend(schema::migrations());
    migrations.extend(seed::migrations());
    migrations.extend(migration::migrations());
    migrations
}
```

### 新增表时的操作清单

1. 在 `schema/{domain}/` 下新增建表文件，并在对应 `migrations()` 中按依赖顺序注册
2. 如需初始数据，在 `seed/` 下新增 `s_{table}.rs`，并在 `seed/mod.rs` 注册
3. 后续表结构变更时，在 `migration/{domain}/` 下新增版本化迁移文件

> 注意：`seed` 中的批量写入（`insert_many`）不触发 `before_save` 钩子，
> 需显式调用 `ActiveModel::ensure_timestamps()` 补齐 `created_at` / `updated_at`。

## 使用方式

```rust
use migration::Migrator;
use sea_orm_migration::MigratorTrait;

// 执行全部迁移
Migrator::up(&db, None).await?;

// 回滚全部迁移
Migrator::down(&db, None).await?;
```

## CLI 命令

```sh
cd crates/core/migration

# 执行迁移
cargo run -p migration -- up

# 回滚迁移
cargo run -p migration -- down

# 查看状态
cargo run -p migration -- status

# 一步到位（读 .env 的 DATABASE_URL，库不存在则自动创建）
cargo run -p migration --example migration
```
