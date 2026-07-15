# 调度器重构：脚本任务执行支持

## 功能说明

本次重构在原有调度框架基础上，新增了**脚本任务执行**能力，并对数据模型、枚举命名进行了修正与扩展。

## 新增内容

### 1. 任务执行器模块 `scheduler::runner`

| 类型            | 说明                                 |
| --------------- | ------------------------------------ |
| `TaskRunner`    | 任务执行器 Trait，所有执行器必须实现 |
| `RunOutput`     | 执行输出（stdout/stderr）            |
| `BuiltinRunner` | 内置代码任务执行器，持有异步处理函数 |
| `BashRunner`    | Bash 脚本执行器，fork 子进程执行     |
| `PythonRunner`  | Python 脚本执行器，fork 子进程执行   |

### 2. 新增数据库字段（`t_schedule_job`）

| 字段        | 类型      | 说明                                   |
| ----------- | --------- | -------------------------------------- |
| `task_type` | TINYINT   | 执行类型：0=内置代码，1=Bash，2=Python |
| `script`    | TEXT NULL | 脚本内容（bash/python 任务使用）       |
| `timeout`   | INT NULL  | 超时时间（秒），NULL 表示不限制        |

### 3. 新增执行日志表 `t_job_run_log`

合并了原 `t_schedule_event_log` 与 `t_schedule_status_log` 的职责，一条记录代表一次完整执行：

| 字段          | 类型          | 说明                             |
| ------------- | ------------- | -------------------------------- |
| `job_id`      | INT           | 关联任务 ID                      |
| `run_id`      | VARCHAR(50)   | 本次运行 UUID                    |
| `status`      | TINYINT       | 0=运行中，1=成功，2=失败，3=超时 |
| `cost`        | BIGINT        | 耗时（毫秒）                     |
| `output`      | TEXT NULL     | 脚本标准输出                     |
| `error`       | TEXT NULL     | 错误信息                         |
| `started_at`  | DATETIME      | 开始时间                         |
| `finished_at` | DATETIME NULL | 结束时间                         |

### 4. 新增枚举

- `scheduler::enums::task_type::TaskType`：`Builtin(0)` / `Bash(1)` / `Python(2)`
- `service-hub/schedule` 中同步新增 `TaskType` 枚举

## 修改内容

### 枚举命名修正

| 位置                                                 | 旧值        | 新值       | 原因                         |
| ---------------------------------------------------- | ----------- | ---------- | ---------------------------- |
| `scheduler::enums::schedule_job::JobType`            | `Timer = 0` | `Cron = 0` | 语义错误，实际是 Cron 表达式 |
| `service-hub/schedule::enums::schedule_job::JobType` | `Timer = 0` | `Cron = 0` | 同上                         |

### `register.rs` 重构

- **修复 Bug**：`UserTaskRegister::register` 中原来同一个 job 被 `add_job` 了两次
- **整合执行器**：根据 `task_type` 字段自动选择 `BashRunner` 或 `PythonRunner`
- **过滤逻辑**：用户任务注册时增加 `status == Online` 过滤，避免注册已下线任务
- **`SysTaskRegister`**：`JobType::Timer` 引用改为 `JobType::Cron`

### `error.rs` 新增错误变体

```rust
ScriptEmpty(i32)       // 脚本内容为空
ScriptSpawnError(String) // 启动子进程失败
ScriptExitError(String)  // 脚本非零退出
Timeout(i32)           // 执行超时
```

### `entity/schedule/schedule_job.rs`

新增三个字段：`task_type: i16`、`script: Option<String>`、`timeout: Option<i32>`

## 注意事项

1. **`t_job_run_log` 为新增表**，原 `t_schedule_event_log` 和 `t_schedule_status_log` 保留，可在确认迁移完成后删除
2. **`sys_code` 字段由 `NOT NULL` 改为 `NULL`**，用户脚本任务不需要此字段
3. **Bash/Python 执行器依赖系统环境**：需确保目标机器安装了 `bash` 和 `python3`
4. **超时控制**：`timeout` 字段为 `NULL` 时不限制执行时间，建议生产环境设置合理超时
5. **`JobType::Cron` 值仍为 `0`**，与数据库中已有数据兼容，仅枚举名称变更

## 使用示例

### 场景一：注册系统内置任务

```rust
use scheduler::{SysTaskRegister, SysTaskTrait, Job, error::Error};
use database::PoolTrait;

struct CleanupTask;

impl<DB: PoolTrait + Clone + Send + Sync + 'static> SysTaskTrait<DB> for CleanupTask {
    fn sys_code(&self) -> String {
        "cleanup_task".to_string()
    }

    fn task_cron(&self, sys_id: i32, expression: String) -> Result<Job<DB>, Error> {
        // 通过 Job::new 构建内置任务
        todo!("构建 Job")
    }
}

// 注册
let mut register = SysTaskRegister::new(db.clone());
register.add_task(Box::new(CleanupTask));
register.register().await?;
```

### 场景二：注册用户 Bash 脚本任务（数据库中已有记录）

```sql
-- 在数据库中插入一条用户 Bash 任务
INSERT INTO t_schedule_job (name, source, job_type, task_type, expression, script, timeout, status)
VALUES ('daily_report', 0, 0, 1, '0 0 8 * * *', 'echo "daily report" >> /tmp/report.log', 60, 1);
```

```rust
// 启动时自动从数据库加载并注册
let mut register = UserTaskRegister::new(db.clone());
register.register().await?;
```

### 场景三：直接使用执行器运行脚本

```rust
use scheduler::runner::{BashRunner, PythonRunner, TaskRunner};

// Bash 脚本，30 秒超时
let bash = BashRunner::new(Some(30));
let output = bash.run(1, Some("echo hello && date")).await?;
println!("stdout: {:?}", output.stdout);

// Python 脚本，不限超时
let python = PythonRunner::new(None);
let output = python.run(2, Some("import sys; print(sys.version)")).await?;
println!("python version: {:?}", output.stdout);

// 自定义 Python 解释器路径
let python = PythonRunner::new(Some(60)).with_python_bin("/usr/bin/python3.11");
```

## 涉及组件

| 组件                                     | 变更类型                                                                |
| ---------------------------------------- | ----------------------------------------------------------------------- |
| `crates/core/scheduler`                  | 新增 `runner/` 模块，重构 `register.rs`，更新 `error.rs`、`enums/`      |
| `crates/core/entity/src/schedule`        | `schedule_job.rs` 新增字段，新增 `job_run_log.rs`                       |
| `crates/core/migration/src/schedule`     | `schedule_job.rs` 新增字段，新增 `job_run_log.rs`                       |
| `crates/core/migration-sql/src/schedule` | `schedule_job.rs` 新增字段，新增 `job_run_log.rs`                       |
| `service-hub/schedule`                   | `dto/schedule_job.rs` 新增字段，`enums/schedule_job.rs` 新增 `TaskType` |
