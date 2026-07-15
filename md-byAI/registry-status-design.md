# 注册表状态字段设计

## 背景

以下五张注册表目前均使用 `enabled: bool` 字段来控制条目的启用/禁用状态：

| 表名                           | 实体文件                  | 说明                                       |
| ------------------------------ | ------------------------- | ------------------------------------------ |
| `t_agent_a2a_service_registry` | `a2a_service_registry.rs` | A2A 远程 Agent 服务注册表                  |
| `t_agent_mcp_service_registry` | `mcp_service_registry.rs` | MCP 服务注册表                             |
| `t_agent_rule_registry`        | `rule_registry.rs`        | 智能体规则注册表                           |
| `t_agent_command_registry`     | `command_registry.rs`     | 智能体命令注册表                           |
| `t_agent_skill_package`        | `skill_package.rs`        | 技能安装包（已使用 `status: i16`，见下文） |

---

## 当前设计（`enabled: bool`）

### 字段定义

```rust
/// 是否启用
pub enabled: bool,
```

### 使用方式

`enabled` 在 DAO 层作为**可选过滤条件**出现，由前端传参控制，并非强制过滤：

```rust
// dao/a2a_service_registry.rs
.apply_if(req.enabled, |query, v| {
    query.filter(a2a_service_registry::Column::Enabled.eq(v))
})
```

Service 层提供独立的 `update_enabled` 方法，用于切换启用状态：

```rust
pub async fn update_enabled(
    &self,
    user_id: i64,
    req: UpdateA2aServiceRegistryEnabledReq,
) -> anyhow::Result<u64>
```

### 当前问题

1. **语义单薄**：`bool` 只能表达"启用/禁用"两种状态，无法区分"草稿未发布"与"已发布但被禁用"
2. **市场场景缺失**：这些表未来将演化为市场表（全局共享、按需关联），需要"草稿 → 发布 → 下架"的生命周期管理
3. **消费者视角缺乏保护**：当前没有强制的"只读已发布条目"查询路径，容易在运行时误加载草稿条目
4. **魔数风险**：`enabled: bool` 无法通过枚举消除魔数，扩展新状态时需要 DDL 变更

---

## 特殊情况：`t_agent_skill_package`

`skill_package` 已经使用了 `status: i16` 字段，并在注释中定义了三种状态：

```rust
/// 状态：0=Installed，1=Disabled，2=Broken
pub status: i16,
```

> ⚠️ 注意：该字段目前仍以魔数注释的形式存在，尚未定义对应的枚举类型，后续优化时需要补充。

---

## 未来演化方向（待实现）

### 目标

将 `enabled: bool` 替换为 `status: i16` 枚举，引入"草稿/发布"生命周期，支持市场化分发场景。

### 拟定枚举设计

```rust
/// 注册表条目状态
///
/// 适用于 A2A 服务、MCP 服务、规则、命令等注册表表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum RegistryStatus {
    /// 草稿（创建后默认状态，仅创作者可见，不可被 Agent 加载）
    Draft = 0,
    /// 已发布（可被消费者浏览和 Agent 加载）
    Published = 1,
    /// 已下架（曾发布但已撤回，消费者不可见，已绑定的 Agent 不受影响）
    Unpublished = 2,
}

impl From<RegistryStatus> for i16 {
    fn from(s: RegistryStatus) -> Self {
        s as i16
    }
}
```

### 查询层分层设计

替换后，DAO 层需要提供两类查询方法，明确区分视角：

```rust
// 管理视角（创作者）：status 作为可选过滤参数，草稿也可见
pub async fn list_by_owner(user_id: i64, status: Option<RegistryStatus>, ...) -> ...

// 消费者视角（市场浏览 / Agent 运行时）：强制只返回已发布条目
pub async fn list_published(...) -> ...
// 内部固定加上：.filter(Column::Status.eq(RegistryStatus::Published as i16))
```

### 各场景过滤策略

| 查询场景                         | 是否强制过滤 status | 说明                                           |
| -------------------------------- | ------------------- | ---------------------------------------------- |
| 管理后台列表（创作者视角）       | ❌ 可选              | 作者需要看到自己的草稿、已发布、已下架全部内容 |
| 市场/选择器列表（消费者视角）    | ✅ 强制 `Published`  | 只展示已发布的条目，草稿不可见                 |
| Agent 运行时加载（绑定关系解析） | ✅ 强制 `Published`  | 只有已发布的服务才能被 Agent 实际使用          |
| info 详情（编辑/权限校验）       | ❌ 不过滤            | 作者需要能查到自己的草稿进行编辑               |
| update / delete                  | ❌ 不过滤            | 草稿也可以被编辑和删除                         |
| 名称唯一性校验                   | ❌ 不过滤            | 草稿状态的名称也应占用命名空间，避免冲突       |

### 迁移注意事项

1. **DDL 变更**：`ALTER TABLE` 将 `enabled BOOLEAN` 改为 `status SMALLINT`，存量数据迁移规则：`enabled=true → status=1(Published)`，`enabled=false → status=2(Unpublished)`
2. **`update_enabled` 接口**：重命名为 `update_status`，入参改为 `RegistryStatus` 枚举
3. **`skill_package.status`**：同步补充 `SkillPackageStatus` 枚举，消除现有魔数注释
4. **API 兼容性**：前端 `enabled` 字段需同步改为 `status`，注意版本兼容

---

## 相关文件

- 实体定义：`agent-harness/crates/agent-entity/src/agent/`
- DAO 层：`agent-harness/service/agent-service/src/dao/`
- Service 层：`agent-harness/service/agent-service/src/service/`
