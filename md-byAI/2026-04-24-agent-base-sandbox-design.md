## `agent-runtime` 跨平台沙箱设计方案（备用）

> 本文件为沙箱能力的独立设计备忘，当前暂不落地，仅用于存档参考。主架构报告见 [2026-04-24-agent-runtime-sub-agent-architecture-analysis.md](./2026-04-24-agent-runtime-sub-agent-architecture-analysis.md)。

---

### 1. 背景与目标

智能体在执行任务时需要调用文件系统、外部命令、MCP 服务、Skill 脚本等能力，这些能力会带来以下风险：

- 越权读取宿主机敏感文件（`/etc/passwd`、用户私钥、环境变量文件等）
- 越权写入工作区以外目录
- 无限制访问网络
- 消耗过多 CPU / 内存 / 进程数

目标：在**运行时瞬间加载、不依赖 Docker、跨 Linux / macOS / Windows 三平台**的前提下，为 `agent-runtime` 提供一套统一的轻量沙箱能力。

---

### 2. 需求对齐的核心约束

- **零容器开销**：沙箱应在毫秒级内完成准备，不能引入容器/VM
- **按 Agent 粒度配置**：不同智能体可拥有不同的可读/可写目录、网络权限
- **跨平台统一接口**：上层代码无需感知底层隔离机制
- **可降级**：在不支持强隔离的平台上，至少提供"审计 + 白名单"降级方案
- **与装配链路对齐**：沙箱策略应通过 `AgentRuntimeBuilder` 注入，与 rule / skill / tool / mcp 同层

---

### 3. Rust 生态候选方案对比

| 方案 | Linux | macOS | Windows | 子进程约束 | 启动开销 | 成熟度 |
|---|---|---|---|---|---|---|
| `cap-std` | ✅ | ✅ | ✅ | ❌（仅约束 Rust 代码） | 零 | 🟢 高 |
| Wasmtime + WASI | ✅ | ✅ | ✅ | ✅（WASM 内） | 毫秒级 | 🟢 高 |
| Deno Core（V8 隔离） | ✅ | ✅ | ✅ | ✅（JS/TS 内） | 几十 ms | 🟢 高 |
| `landlock` | ✅ | ❌ | ❌ | ✅ | 零 | 🟢 高 |
| `extrasafe`（landlock + seccomp） | ✅ | ❌ | ❌ | ✅ | 零 | 🟢 高 |
| `seccompiler` / `libseccomp-rs` | ✅ | ❌ | ❌ | ✅（syscall 级） | 零 | 🟢 高 |
| `bubblewrap`（外部命令） | ✅ | ❌ | ❌ | ✅ | 几十 ms | 🟢 高 |
| `sandbox-exec`（macOS 原生） | ❌ | ✅ | ❌ | ✅ | 几 ms | 🟡 中（官方标 deprecated，实际长期可用） |
| Job Object + AppContainer | ❌ | ❌ | ✅ | ✅ | 几 ms | 🟢 高 |
| Firecracker / gVisor | ✅ | ❌ | ❌ | ✅ | 几十 ms | 🟢 高 |

**结论**：没有"一套代码跨三平台隔离子进程"的银弹。跨平台真正通用的只有 `cap-std`（Rust 内代码）与 WASM 沙箱（受沙箱化代码）。约束原生子进程必须做**平台分发**。

---

### 4. 推荐方案：分层沙箱 + 平台分发

整体采用"**Rust 内代码用 `cap-std` + 原生子进程按平台分发 + 可沙箱化工具走 WASM + 兜底白名单**"的四层组合。

```text
┌────────────────────────────────────────────────────┐
│ Layer 1: Rust 内代码 FS 访问                       │
│   └─ cap-std Dir 句柄（三平台统一，零开销）        │
├────────────────────────────────────────────────────┤
│ Layer 2: 原生子进程执行（run_command / MCP stdio） │
│   ├─ Linux   → landlock + seccomp                  │
│   ├─ macOS   → sandbox-exec 动态 profile           │
│   └─ Windows → Job Object + AppContainer           │
├────────────────────────────────────────────────────┤
│ Layer 3: 可沙箱化 Skill / 工具                     │
│   └─ Wasmtime + WASI preopen dirs                  │
├────────────────────────────────────────────────────┤
│ Layer 4: 兜底策略（所有平台通用）                  │
│   └─ 命令白名单 + 参数校验 + 人机确认              │
└────────────────────────────────────────────────────┘
```

### 4.1 命令分类与沙箱策略

将智能体可调用的命令分为三类：

| Tier | 类型 | 执行方式 | 沙箱强度 |
|---|---|---|---|
| 1 | 内置 FS 工具（`read_file / write_file / grep`） | `cap-std` | 编译期能力约束 |
| 2 | 白名单原生命令（`git / cargo / rustc / npm`） | 平台沙箱 + 参数校验 | 内核/系统强制 |
| 3 | 任意命令（`bash -c ...`） | 默认禁用，显式开启 + 人机确认 | 人为兜底 |

---

### 5. 模块设计

```text
crates/core/agent-runtime/src/sandbox/
├── mod.rs              # 模块声明与重导出（无逻辑）
├── types.rs            # SandboxPolicy / ResourceLimits / Violation
├── traits.rs           # Sandbox trait
├── command.rs          # SandboxedCommand（系统命令执行入口）
├── fs_scope.rs         # cap-std 封装的 Rust 内 FS 访问
├── wasm_runner.rs      # Wasmtime 封装（feature gated）
└── platform/
    ├── mod.rs
    ├── linux.rs        # landlock + seccomp 实现
    ├── macos.rs        # sandbox-exec 实现
    ├── windows.rs      # Job Object + AppContainer 实现
    └── fallback.rs     # 无沙箱降级（仅审计日志）
```

### 5.1 `types.rs`

```rust
//! 沙箱策略与资源限制类型定义

use std::path::PathBuf;
use std::time::Duration;

/// 沙箱策略
///
/// 描述一个智能体（或其子进程 / Skill）允许的文件、网络、资源访问范围。
#[derive(Debug, Clone, Default)]
pub struct SandboxPolicy {
    /// 只读允许的目录
    pub readable_dirs: Vec<PathBuf>,
    /// 读写允许的目录
    pub writable_dirs: Vec<PathBuf>,
    /// 是否允许出站网络
    pub network: bool,
    /// 环境变量白名单（不在列表内的变量不会传递给子进程）
    pub env_whitelist: Vec<String>,
    /// 资源限制
    pub limits: ResourceLimits,
}

/// 资源限制
#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    /// CPU 时间上限（秒）
    pub cpu_seconds: Option<u64>,
    /// 内存上限（字节）
    pub memory_bytes: Option<u64>,
    /// 墙钟时间上限
    pub wall_time: Option<Duration>,
    /// 最大子进程数
    pub max_processes: Option<u32>,
}

/// 沙箱违规事件
#[derive(Debug, Clone)]
pub enum Violation {
    /// 路径越权
    PathDenied { path: PathBuf },
    /// 网络越权
    NetworkDenied,
    /// 资源超限
    LimitExceeded { kind: &'static str },
}
```

### 5.2 `traits.rs`

```rust
//! 沙箱执行抽象

use std::ffi::OsStr;
use std::process::Output;
use async_trait::async_trait;

use super::types::SandboxPolicy;

/// 沙箱执行接口
#[async_trait]
pub trait Sandbox: Send + Sync + 'static {
    /// 在沙箱中执行命令
    async fn run(
        &self,
        cmd: &OsStr,
        args: &[&OsStr],
        policy: &SandboxPolicy,
    ) -> Result<Output, SandboxError>;

    /// 当前平台是否支持真实隔离
    fn is_enforcing(&self) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("sandbox setup failed: {0}")]
    Setup(String),
    #[error("command execution failed: {0}")]
    Exec(String),
    #[error("policy violation: {0:?}")]
    Violation(super::types::Violation),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

### 5.3 平台分发入口

```rust
//! 当前平台默认沙箱工厂

use std::sync::Arc;
use super::traits::Sandbox;

/// 根据当前平台创建默认沙箱实现
pub fn default_sandbox() -> Arc<dyn Sandbox> {
    #[cfg(target_os = "linux")]
    return Arc::new(super::platform::linux::LandlockSandbox::new());

    #[cfg(target_os = "macos")]
    return Arc::new(super::platform::macos::SandboxExec::new());

    #[cfg(target_os = "windows")]
    return Arc::new(super::platform::windows::JobObjectSandbox::new());

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return Arc::new(super::platform::fallback::AuditOnlySandbox::new());
}
```

---

### 6. 平台实现要点

### 6.1 Linux：`landlock` + `seccomp`

依赖：

```toml
landlock = "0.4"
seccompiler = "0.4"
nix = { version = "0.29", features = ["process", "signal"] }
```

核心流程：

1. `fork()` 子进程
2. 子进程内调用 `landlock::Ruleset` 添加路径白名单并 `restrict_self()`
3. 应用 seccomp filter 禁用网络 syscall（可选）
4. `exec()` 目标命令

```rust
use landlock::{
    ABI, Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
};

fn confine(policy: &SandboxPolicy) -> anyhow::Result<()> {
    let abi = ABI::V2;
    let mut rs = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))?
        .create()?;

    for d in &policy.readable_dirs {
        rs = rs.add_rule(PathBeneath::new(
            PathFd::new(d)?,
            AccessFs::from_read(abi),
        ))?;
    }
    for d in &policy.writable_dirs {
        rs = rs.add_rule(PathBeneath::new(
            PathFd::new(d)?,
            AccessFs::from_all(abi),
        ))?;
    }
    rs.restrict_self()?;
    Ok(())
}
```

前置条件：Linux 内核 ≥ 5.13，生产建议 ≥ 5.19。

### 6.2 macOS：`sandbox-exec`

动态生成 `.sb` profile（TinyScheme 语法），通过 `sandbox-exec -f profile.sb <cmd>` 启动：

```scheme
(version 1)
(deny default)
(allow process-exec)
(allow process-fork)
(allow file-read* (subpath "/Users/foo/workspace"))
(allow file-write* (subpath "/Users/foo/workspace/.tmp"))
(deny network*)
```

实现要点：

- profile 写入临时文件，进程退出后清理
- 参数转义使用 `"` 包裹并处理特殊字符
- 检测 `sandbox-exec` 可用性，不可用时降级到 `fallback`

Apple 官方虽标记 deprecated，但 Chromium / Deno / Bun / GitHub Codespaces 仍在生产使用。

### 6.3 Windows：Job Object + AppContainer

依赖：

```toml
win32job = "2"
windows = { version = "0.58", features = ["Win32_System_JobObjects", "Win32_Security"] }
```

基本思路：

- Job Object 做资源限制（内存、CPU、进程数）+ `kill_on_job_close`
- AppContainer SID + 目录 ACL 做文件系统隔离
- 简化版可降级为"低完整性级别进程（Low Integrity）+ 目录 ACL"

```rust
use win32job::{ExtendedLimitInfo, Job};

fn spawn_in_job(cmd: &OsStr, args: &[&OsStr], policy: &SandboxPolicy) -> anyhow::Result<Child> {
    let mut info = ExtendedLimitInfo::new();
    if let Some(mem) = policy.limits.memory_bytes {
        info.limit_working_memory(mem / 2, mem);
    }
    info.limit_kill_on_job_close();
    let job = Job::create_with_limit_info(&mut info)?;

    let child = std::process::Command::new(cmd).args(args).spawn()?;
    job.assign_process(child.as_raw_handle() as _)?;
    Ok(child)
}
```

### 6.4 Fallback：审计降级

在不支持真实隔离的平台（或未启用沙箱时）：

- 命令白名单校验
- 参数正则校验
- 所有调用写入审计日志
- `is_enforcing()` 返回 `false`，上层可据此提示用户风险

---

### 7. 与 `AgentRuntimeBuilder` 的集成

沙箱策略作为 Builder 链路的一级输入：

```rust
impl AgentRuntimeBuilder {
    /// 设置当前运行时的沙箱策略
    pub fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.sandbox_policy = Some(policy);
        self
    }
}
```

`build()` 时的下发点：

1. **内置 FS 工具**（`read_file / write_file` 等）
   - 使用 `cap-std::fs::Dir` 按 `readable_dirs / writable_dirs` 打开能力句柄
   - 存入 `RuntimeServices::fs_scope`
2. **命令执行工具**（`run_command`）
   - 使用 `default_sandbox().run(cmd, args, &policy)` 执行
3. **MCP stdio 服务启动**
   - `McpRegistry` 启动 stdio server 时复用 `Sandbox::run`
4. **Skill 脚本执行**
   - 可选 `Wasmtime` runner，或走原生沙箱路径

### 7.1 子智能体的沙箱继承

子智能体的沙箱策略遵循"收敛不放大"原则：

- 子 Agent 策略必须是父 Agent 的**子集**
- `readable_dirs` / `writable_dirs` 只能收窄，不能新增父未授权的路径
- `network` 只能在父允许时开启
- 资源上限不得超过父上限

该校验在 `sub_agent::runtime_factory::build_sub_runtime` 中进行。

---

### 8. WASM 子层（可选 feature）

用于"可沙箱化 Skill"：

```toml
[features]
wasm-sandbox = ["wasmtime", "wasmtime-wasi"]
```

```rust
use wasmtime::*;
use wasmtime_wasi::WasiCtxBuilder;

let wasi = WasiCtxBuilder::new()
    .preopened_dir(
        cap_std::fs::Dir::open_ambient_dir(&workspace, cap_std::ambient_authority())?,
        "/workspace",
    )?
    .build();
```

优势：

- 三平台行为一致
- 真正的能力隔离（无 preopen 无访问）
- 毫秒级启动

限制：

- 只能跑 WASM 模块
- 需要把 Skill 编译为 WASM

---

### 9. 选型建议（按场景）

| 场景 | 推荐方案 |
|---|---|
| 智能体自身 Rust 代码做 FS IO | `cap-std` |
| 智能体调用的子进程（bash / python 等） | Linux: `landlock` ／ macOS: `sandbox-exec` ／ Windows: Job Object |
| 需要跨三平台一致（含 Windows / macOS） | `cap-std` + WASM Skill |
| 跑用户提供的不可信脚本 | Wasmtime + WASI 或 bubblewrap（仅 Linux） |
| 既要限文件又要禁网络 | Linux: `extrasafe`（landlock + seccomp） |

---

### 10. 落地路线

分阶段推进，避免一次性引入过多平台依赖：

1. **Phase 1**：`SandboxPolicy` + `cap-std` 内建 FS 工具 + 命令白名单
   - 三平台零差异，覆盖 80% 风险面
2. **Phase 2**：Linux `landlock` 实现
   - 覆盖服务器生产环境
3. **Phase 3**：macOS `sandbox-exec` + Windows Job Object
   - 达成三平台对齐
4. **Phase 4**：Wasmtime Skill runner（feature gated）
   - 长期替换部分高风险原生命令

---

### 11. 风险与注意事项

- **macOS sandbox-exec** 官方标记 deprecated，需要持续关注替代方案
- **Windows AppContainer** 配置复杂，简化实现可能留有侧信道
- **Landlock 内核版本要求**，老旧环境需降级到 `fallback`
- **子进程沙箱逃逸**：必须禁用 `ptrace`、`unshare`、`chroot` 等可能破坏沙箱的 syscall
- **性能**：`cap-std` 零开销；平台沙箱每次调用有 μs 级开销，批量命令执行可复用 sandbox context
- **审计**：所有沙箱违规事件必须进入统一审计日志，与 delegation trace 对齐

---

### 12. 一句话总结

`cap-std` 管 Rust 内代码做跨平台 FS 能力约束；`Sandbox` trait 在子进程层面按平台分发到 `landlock` / `sandbox-exec` / Job Object；可沙箱化 Skill 走 Wasmtime；所有场景共用 `SandboxPolicy` 作为统一策略入口，由 `AgentRuntimeBuilder` 一次性注入并下发。
