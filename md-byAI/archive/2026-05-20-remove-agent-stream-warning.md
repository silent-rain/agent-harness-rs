# 删除 `AgentStreamEvent::Warning`,统一由 `RunFinished` 表达终止信号

> 日期:2026-05-20
> 范围:`agent-runtime` / `agent-service`
> 性质:**Breaking change**(无向后兼容)

## 1. 功能说明

按业界主流流式 Agent 协议(OpenAI Responses API、Anthropic Messages、Vercel AI SDK、AG-UI)的事件模型设计,**流式事件中不应该出现"中间态告警"通道**。所有"非理想但可解释"的终止应通过 `RunFinished.finish_reason` + `error_message` 统一表达,而不是单独发一个 `Warning` 事件。

理由:`Warning` 没有可消费的语义 —— 客户端无法据此采取行动,只能"显示一段红字",而这种 UI 渲染应该由 `RunFinished` 的终止三元组(`success` / `finish_reason` / `error_message`)统一驱动。

本次改造彻底移除 `AgentStreamEvent::Warning` 变体,并把原本通过 `Warning` 暴露的"达到 max_turns"等运行结果信息收敛到 `RunFinished.error_message`。

## 2. 新增内容

无新增类型。改造后的事件协议保留以下与终止相关的事件:

| 事件          | 角色                                                         |
| ------------- | ------------------------------------------------------------ |
| `RunFinished` | **唯一终止信号**;携带 `finish_reason` 与可选 `error_message` |
| `Error`       | 不可恢复错误的独立通知(仍保留,与 `RunFinished` 配合使用)     |

`FinishReason` 枚举不变:`FinalizeAction` / `MaxTurns` / `Error` / `Cancelled`。

## 3. 修改内容

### 3.1 `crates/core/agent-runtime/src/session/event.rs`

- 删除 `AgentStreamEvent::Warning { run_id, message }` 变体
- 删除 `event_type()` 中的 `Self::Warning => "warning"` 分支
- 删除 `run_id()` 中的 `Self::Warning { run_id, .. }` 分支

### 3.2 `crates/core/agent-runtime/src/runtime/agent/llm_agent.rs`

- `drive_react_loop` 返回签名由 `(String, FinishReason, u32)` 扩展为 `(String, FinishReason, u32, Option<String>)`,第四个分量为 `error_message`,与 `FinishReason` 就近对齐生成
- `MaxTurns` 分支不再发布 `AgentStreamEvent::Warning`,只保留 `warn!` 日志,并把"已达到最大推理轮次 N"作为 `error_message` 返回
- `run_stream_inner` 中 `Err` 分支:把 `err.to_string()` 透传到 `error_message`,与 `AgentStreamEvent::Error` 同源
- 删除 `RunFinished` 构造前那段硬编码的 `"运行结束但未产出最终回答"` 兜底文案
- 同步更新 `emit_persisted` 的 doc 注释,移除 `Warning` 字样

`error_message` 与 `FinishReason` 的对齐关系:

| `finish_reason`  | `error_message`                                             |
| ---------------- | ----------------------------------------------------------- |
| `FinalizeAction` | `None`                                                      |
| `MaxTurns`       | `Some("已达到最大推理轮次 N")`                              |
| `Error`          | `Some(<原始错误>)`(由 `Err` 分支注入,与 `Error` 事件同文案) |
| `Cancelled`      | 预留(当前未达进入路径)                                      |

### 3.3 `service-hub/agent-service/src/dto/agent_run.rs`

- 删除 SSE `event_type` 映射中的 `AgentStreamEvent::Warning { .. } => "warning"` 分支

### 3.4 `crates/core/agent-runtime/src/openai/stream_adapter.rs`

- 删除 OpenAI 适配器中的 `AgentStreamEvent::Warning { message, .. } => extension("warning", ...)` 分支

## 4. 注意事项

1. **Breaking change**:任何外部下游若曾匹配 `AgentStreamEvent::Warning`,需迁移为消费 `RunFinished.error_message`
2. SSE 协议层不再出现 `event: warning`;`nucleus.warning` 扩展事件名也已下线
3. `RunFinished.error_message` 现在是终止原因的**唯一文案出口**,前端 UI 应基于此字段(配合 `finish_reason`)统一渲染
4. `Error` 事件与 `RunFinished { finish_reason: Error, error_message: Some(...) }` 同源同文案,客户端可任选其一消费,推荐统一基于 `RunFinished` 渲染最终态、`Error` 仅用于"提前红屏告警"

## 5. 使用示例

### 示例 1:消费终止事件,正确区分 MaxTurns 与 Error

```rust
use agent_runtime::session::{AgentStreamEvent, FinishReason};
use futures::StreamExt;

async fn consume(stream: impl futures::Stream<Item = AgentStreamEvent> + Unpin) {
    futures::pin_mut!(stream);
    while let Some(event) = stream.next().await {
        if let AgentStreamEvent::RunFinished {
            success,
            finish_reason,
            error_message,
            ..
        } = event
        {
            match finish_reason {
                FinishReason::FinalizeAction => println!("正常结束"),
                FinishReason::MaxTurns => {
                    // 文案形如:"已达到最大推理轮次 10"
                    eprintln!("达到上限: {}", error_message.unwrap_or_default());
                }
                FinishReason::Error => {
                    eprintln!("运行失败: {}", error_message.unwrap_or_default());
                }
                FinishReason::Cancelled => println!("已取消"),
            }
            assert!(success == matches!(finish_reason, FinishReason::FinalizeAction));
            break;
        }
    }
}
```

### 示例 2:SSE 客户端不再监听 `warning` 事件

```ts
// ❌ 改造前:需要单独监听 warning 通道
// eventSource.addEventListener("warning", e => { ... });

// ✅ 改造后:仅监听 run_finished 即可拿到所有终止信息
eventSource.addEventListener("run_finished", (e) => {
  const data = JSON.parse(e.data);
  if (!data.success) {
    showBanner(data.error_message ?? "运行未成功完成");
  }
});
```

### 示例 3:OpenAI 兼容协议下的终止映射

```rust
use agent_runtime::openai::OpenAIStreamAdapter;
use agent_runtime::session::{AgentStreamEvent, FinishReason};

let mut adapter = OpenAIStreamAdapter::new("run-1", "gpt-4");

// MaxTurns -> OpenAI finish_reason: "length"
let outputs = adapter.translate(AgentStreamEvent::RunFinished {
    run_id: "run-1".to_string(),
    success: false,
    total_steps: 10,
    finish_reason: FinishReason::MaxTurns,
    error_message: Some("已达到最大推理轮次 10".to_string()),
    duration_ms: 1234,
});
// outputs[0] = Chunk { finish_reason: Length }
// outputs[1] = Done
```

## 6. 兼容性说明

| 项                                                    | 状态             |
| ----------------------------------------------------- | ---------------- |
| `AgentStreamEvent::Warning` 变体                      | **已删除**       |
| `AgentStreamEvent::event_type()` 返回 `"warning"`     | **已删除**       |
| SSE 事件 `event: warning`                             | **已下线**       |
| OpenAI 扩展事件 `nucleus.warning`                     | **已下线**       |
| `RunFinished.error_message` 在 `MaxTurns` 时为 `None` | 改为携带具体文案 |
| `RunFinished.error_message` 在 `Error` 时硬编码兜底   | 改为透传真实错误 |

涉及组件/模块:

- `agent-runtime::session::event` — 事件协议定义
- `agent-runtime::runtime::agent::llm_agent` — ReAct 主循环与终止信号生成
- `agent-runtime::openai::stream_adapter` — OpenAI 流式协议适配器
- `agent-service::dto::agent_run` — SSE 服务层事件分类
