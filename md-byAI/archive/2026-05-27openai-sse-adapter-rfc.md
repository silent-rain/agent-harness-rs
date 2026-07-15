# OpenAI 兼容输出层改造 RFC（方案 C，一步到位、不向后兼容）

> Status: **DRAFT**
> Author: AI Pair (待 review)
> Last Updated: 2026-05-15
> Scope: `agent-base` 流式输出协议 + `agent-service` HTTP 暴露层

## 1. 功能说明

### 1.1 改造目标

把 nucleus-rs Agent 对外暴露的两个调用入口（同步 / 流式）**一次性**改造为 **OpenAI Chat Completions API 兼容协议**：

| 入口                     | OpenAI 标准等价物                                 |
| ------------------------ | ------------------------------------------------- |
| `POST /agent/run`        | `POST /v1/chat/completions`（`stream=false`）     |
| `POST /agent/run/stream` | `POST /v1/chat/completions`（`stream=true`，SSE） |

调用方可直接使用 OpenAI Python/JS/Go SDK 消费。

### 1.2 不向后兼容的取舍

- **删除** `AgentStreamEvent` 作为对外协议的角色（保留为内部事件总线）
- **删除** 自定义 `SseEventData::from_agent_event` 转换层
- **删除** `AnswerDelta` 在 `dispatch_finalize` 里的"伪流"用法（实际是整段一次性发送）
- 调试/运维事件（`ToolCallStarted` / `RagStarted` / `StepStarted` 等）以 OpenAI **扩展字段**或**自定义 SSE event name**承载，不混入 `data` 主流

### 1.3 核心收益

| 维度       | 改造前                                       | 改造后                                              |
| ---------- | -------------------------------------------- | --------------------------------------------------- |
| 流式 token | `dispatch_finalize` 整段一次性 `AnswerDelta` | 真正逐 token `delta.content`                        |
| 协议       | 自定义 19 种事件枚举                         | OpenAI 标准 `ChatCompletionChunk`                   |
| SDK 复用   | 必须自写客户端解析                           | OpenAI 官方 SDK 直接可用                            |
| Tool 调用  | 自定义 `ToolCallStarted/Finished`            | OpenAI `delta.tool_calls[].function.arguments` 流   |
| 终止信号   | `RunFinished` + `FinishReason` 自定义枚举    | `finish_reason: "stop" \| "tool_calls" \| "length"` |

---

## 2. 协议设计

### 2.1 同步响应（`stream=false`）

完全对齐 OpenAI `ChatCompletion`：

```jsonc
{
  "id": "chatcmpl-<run_id>",
  "object": "chat.completion",
  "created": 1715750000,
  "model": "<agent_config.model_config.model_code>",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "最终答案文本"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 0,
    "completion_tokens": 0,
    "total_tokens": 0
  },
  // ── nucleus 扩展字段（命名前缀 nucleus_）────
  "nucleus_run_id": "uuid-...",
  "nucleus_session_id": "uuid-...",
  "nucleus_total_steps": 5
}
```

> 字段说明：`finish_reason` 仅取 OpenAI 合法值；`nucleus_*` 扩展字段仅在 nucleus 自身需要时使用，OpenAI SDK 会忽略。

### 2.2 流式响应（`stream=true`）

`Content-Type: text/event-stream`，每行一个 `data: <json>` chunk，**不带自定义 SSE `event:` 字段**（OpenAI 标准做法）。

#### 2.2.1 文本流 chunk

```jsonc
data: {"id":"chatcmpl-<run_id>","object":"chat.completion.chunk","created":1715750000,"model":"...","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-<run_id>","object":"chat.completion.chunk","created":1715750000,"model":"...","choices":[{"index":0,"delta":{"content":"你"},"finish_reason":null}]}

data: {"id":"chatcmpl-<run_id>","object":"chat.completion.chunk","created":1715750000,"model":"...","choices":[{"index":0,"delta":{"content":"好"},"finish_reason":null}]}

data: {"id":"chatcmpl-<run_id>","object":"chat.completion.chunk","created":1715750000,"model":"...","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

#### 2.2.2 工具调用 chunk（OpenAI 标准 tool_calls delta）

```jsonc
data: {"id":"chatcmpl-...","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"search_web","arguments":""}}]},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"q\":\""}}]},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"rust\"}"}}]},"finish_reason":null}]}
```

#### 2.2.3 推理（Reasoning）chunk —— OpenAI o1 风格

```jsonc
data: {"id":"...","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"reasoning_content":"思考片段..."},"finish_reason":null}]}
```

> `reasoning_content` 是 DeepSeek/通义/月之暗面等通用扩展，OpenAI 官方 o 系列用的是 `reasoning`。本项目统一使用 `reasoning_content`，与国内主流模型对齐。

#### 2.2.4 nucleus 扩展事件（运维/调试）

OpenAI 标准 chunk **不携带**步骤、工具完成、RAG 这些信息。我们以**自定义 SSE event name**承载扩展事件，**默认开启**（前端 OpenAI SDK 会自动忽略带 `event:` 前缀的非 `message` 事件）：

```text
event: nucleus.tool_call.finished
data: {"step":3,"toolname":"search_web","call_id":"call_abc","success":true,"duration_ms":120}

event: nucleus.step.started
data: {"step":4}

event: nucleus.rag.finished
data: {"step":2,"hit_count":5}
```

调用方若不关心，无需处理；OpenAI SDK 也只读默认 `data` 行，不会报错。

### 2.3 错误响应

OpenAI 错误格式（同步 + 流式末尾 chunk）：

```jsonc
{
  "error": {
    "type": "agent_run_failed",
    "code": "no_final_answer",
    "message": "Agent did not produce a final answer within max turns"
  }
}
```

流式中错误时：发送一条 `error` 自定义 event，再发 `data: [DONE]`。

---

## 3. 模块改造

### 3.1 新增模块

```text
crates/core/agent-base/src/
└── openai/                           # ← 新增
    ├── mod.rs                        # 仅 pub mod + pub use
    ├── chunk.rs                      # ChatCompletionChunk / ChoiceDelta / ToolCallDelta
    ├── completion.rs                 # ChatCompletion（非流式）
    ├── error.rs                      # OpenAIError
    └── stream_adapter.rs             # AgentStreamEvent → ChatCompletionChunk 转换器

service-hub/agent-service/src/
└── openai/                           # ← 新增（只承载 HTTP 层 DTO 与路由）
    ├── mod.rs
    ├── dto.rs                        # ChatCompletionRequest / ChatMessage / ToolDef
    ├── controller.rs                 # POST /v1/chat/completions
    └── sse.rs                        # SSE 写出：data: {} / [DONE] / nucleus.* event
```

### 3.2 修改的现有模块

| 文件                                        | 改动                                                                                        |
| ------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `model_caller.rs`                           | `TextDelta` 真正发布到 publisher（新事件 `OpenAIChunk`），`ReasoningDelta` 透传 OpenAI 形态 |
| `decision_dispatcher.rs::dispatch_finalize` | **删除** `AnswerDelta` 发布，仅保留入库与记忆                                               |
| `event.rs::AgentStreamEvent`                | 新增 `ChatCompletionChunk { run_id, chunk }` 变体；`AnswerDelta` 标记为 deprecated 并删除   |
| `service/agent_run.rs`                      | `run` / `run_stream` 改为返回 OpenAI 类型                                                   |
| `controller/agent_run.rs`                   | 路由迁移到 `POST /v1/chat/completions`                                                      |
| `dto/agent_run.rs::SseEventData`            | **删除**（被 `openai/sse.rs` 取代）                                                         |

### 3.3 删除的模块/类型

- `SseEventData` 及其 `from_agent_event`
- `AgentStreamEvent::AnswerDelta`（在 `dispatch_finalize` 的伪流用法）
- `FinishReason` 不删（仍在内部用），但在对外协议处映射为 OpenAI 字符串

---

## 4. 核心类型定义

### 4.1 `crates/core/agent-base/src/openai/chunk.rs`

```rust
//! OpenAI Chat Completions 流式 chunk 数据模型
//!
//! 完全对齐 OpenAI Chat Completions API（2024-11 规范）。
//!
//! 提供的主要内容：
//! - [`ChatCompletionChunk`]: 单条流式分片
//! - [`ChunkChoice`]: chunk 中的 choice 项
//! - [`ChoiceDelta`]: 增量内容
//! - [`ChunkToolCallDelta`]: 工具调用增量
//! - [`FinishReason`]: 完成原因（OpenAI 字符串枚举）

use serde::{Deserialize, Serialize};

/// OpenAI 流式分片
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    /// 唯一 ID（建议格式 `chatcmpl-<run_id>`）
    pub id: String,
    /// 固定为 `"chat.completion.chunk"`
    pub object: &'static str,
    /// Unix 时间戳（秒）
    pub created: i64,
    /// 模型代码
    pub model: String,
    /// 一般只含 1 个 choice（n=1）
    pub choices: Vec<ChunkChoice>,
    /// 仅最后一条 chunk 在 `stream_options.include_usage=true` 时携带
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// chunk 内单个 choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    /// choice 序号
    pub index: u32,
    /// 增量内容
    pub delta: ChoiceDelta,
    /// 完成原因（仅最后一条 chunk 非 null）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
}

/// 增量内容
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChoiceDelta {
    /// 仅首条 chunk 出现
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'static str>,
    /// 文本增量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// 推理过程增量（DeepSeek 风格）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// 工具调用增量（数组下标稳定，跨 chunk 拼接）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChunkToolCallDelta>>,
}

/// 工具调用增量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkToolCallDelta {
    /// 同一工具调用所有 chunk 共享同一 index
    pub index: u32,
    /// provider 分配的 ID（仅首条 chunk 携带）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 类型，固定 `"function"`
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub call_type: Option<&'static str>,
    /// 函数增量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ChunkFunctionDelta>,
}

/// 函数增量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkFunctionDelta {
    /// 函数名（仅首条 chunk）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 参数 JSON 字符串增量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// 完成原因（OpenAI 字符串枚举）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// 正常完成
    Stop,
    /// 还有 tool_calls 未消费（理论上 agent 内部已消费完，仅表达"本轮以工具调用结束"）
    ToolCalls,
    /// 达到 max_tokens / 推理最大轮次
    Length,
    /// 内容审查阻断
    ContentFilter,
}

/// Token 使用量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}
```

### 4.2 `openai/stream_adapter.rs` —— 内部事件 → OpenAI chunk

这是**核心适配器**，负责把 `AgentStreamEvent` 翻译为 `ChatCompletionChunk` + 扩展事件。

```rust
//! 将 nucleus 内部 `AgentStreamEvent` 适配为 OpenAI 流式协议
//!
//! 输出双通道：
//! - **主流**：`ChatCompletionChunk`，对齐 OpenAI Chat Completions stream
//! - **扩展事件**：`NucleusExtensionEvent`，承载 step/tool_call/rag 等运维信息
//!
//! 适配器持有少量本地状态（tool_calls 索引映射、是否已发首条 role chunk 等），
//! 不可跨 run 复用，每个 run 一次性创建。

use std::collections::HashMap;

use crate::openai::chunk::{ChatCompletionChunk, /* ... */};
use crate::session::AgentStreamEvent;

/// 单个 run 的流式协议适配器
pub struct OpenAIStreamAdapter {
    /// chunk.id 前缀（一般为 `chatcmpl-<run_id>`）
    chat_id: String,
    /// 模型代码
    model: String,
    /// 是否已发出首条带 role 的 chunk
    role_emitted: bool,
    /// internal_call_id → OpenAI tool_calls 数组下标（跨 chunk 稳定）
    tool_call_indexes: HashMap<String, u32>,
    /// 下一个 tool_call 分配的下标
    next_tool_index: u32,
}

/// 适配器输出
pub enum AdapterOutput {
    /// OpenAI 标准 chunk（写入 data: 行）
    Chunk(ChatCompletionChunk),
    /// nucleus 扩展事件（写入 event: nucleus.* + data: 行）
    Extension {
        event_name: &'static str,
        payload: serde_json::Value,
    },
    /// 流结束（写入 data: [DONE]）
    Done,
    /// 此事件不需要对外输出（如 RunStarted 仅用于初始化）
    Skip,
}

impl OpenAIStreamAdapter {
    pub fn new(run_id: &str, model: String) -> Self { /* ... */ }

    /// 将一个内部事件翻译成 0..=2 个 OpenAI 输出
    pub fn translate(&mut self, event: AgentStreamEvent) -> Vec<AdapterOutput> {
        match event {
            AgentStreamEvent::RunStarted { .. } => vec![AdapterOutput::Skip],

            // text token 真流
            AgentStreamEvent::TextDelta { delta, .. } => {
                let mut out = Vec::with_capacity(2);
                if !self.role_emitted {
                    out.push(AdapterOutput::Chunk(self.role_chunk()));
                    self.role_emitted = true;
                }
                out.push(AdapterOutput::Chunk(self.content_chunk(delta)));
                out
            }

            AgentStreamEvent::ReasoningDelta { delta, .. } =>
                vec![AdapterOutput::Chunk(self.reasoning_chunk(delta))],

            // 工具调用 delta（可选：当 ModelCaller 决定让外界感知 tool_call 流时启用）
            // 默认场景：agent 自治执行工具，外界不可见 tool_call —— 走扩展事件即可

            AgentStreamEvent::ToolCallStarted { toolname, call_id, arguments_json, step, .. } =>
                vec![AdapterOutput::Extension {
                    event_name: "nucleus.tool_call.started",
                    payload: serde_json::json!({
                        "step": step, "toolname": toolname,
                        "call_id": call_id, "arguments": arguments_json,
                    }),
                }],

            AgentStreamEvent::ToolCallFinished { toolname, call_id, success, duration_ms, .. } =>
                vec![AdapterOutput::Extension {
                    event_name: "nucleus.tool_call.finished",
                    payload: serde_json::json!({
                        "toolname": toolname, "call_id": call_id,
                        "success": success, "duration_ms": duration_ms,
                    }),
                }],

            AgentStreamEvent::StepStarted { step, .. } =>
                vec![AdapterOutput::Extension {
                    event_name: "nucleus.step.started",
                    payload: serde_json::json!({ "step": step }),
                }],

            AgentStreamEvent::RagStarted { .. } | AgentStreamEvent::RagFinished { .. } =>
                vec![/* 同上，event_name = nucleus.rag.* */],

            AgentStreamEvent::RunFinished { finish_reason, .. } => {
                vec![
                    AdapterOutput::Chunk(self.finish_chunk(finish_reason)),
                    AdapterOutput::Done,
                ]
            }

            AgentStreamEvent::Error { code, message, .. } =>
                vec![AdapterOutput::Extension {
                    event_name: "error",
                    payload: serde_json::json!({
                        "error": { "type": "agent_run_failed", "code": code, "message": message }
                    }),
                }, AdapterOutput::Done],

            // 其余调试事件（PlannerFunctionCallStarted 等）默认 Skip
            _ => vec![AdapterOutput::Skip],
        }
    }

    fn role_chunk(&self) -> ChatCompletionChunk { /* delta.role = "assistant" */ }
    fn content_chunk(&self, delta: String) -> ChatCompletionChunk { /* delta.content */ }
    fn reasoning_chunk(&self, delta: String) -> ChatCompletionChunk { /* delta.reasoning_content */ }
    fn finish_chunk(&self, reason: super::super::session::FinishReason) -> ChatCompletionChunk {
        /* finish_reason 映射：FinalizeAction → Stop, MaxTurns → Length, Error/Cancelled → Stop */
    }
}
```

### 4.3 新增内部事件 `AgentStreamEvent::TextDelta`

为了让 `ModelCaller` 把 LLM token **真流** publish 出去，新增：

```rust
// agent-base/src/session/event.rs
pub enum AgentStreamEvent {
    // ... 原有变体 ...

    /// LLM 文本 token 增量（真流）
    ///
    /// 对应 OpenAI `delta.content` 字段。在每次 `ModelCaller` 收到 provider
    /// 的 `StreamChunk::TextDelta` 时立即发布。
    TextDelta { run_id: String, delta: String },

    // 删除：AnswerDelta（被 TextDelta 取代）
}
```

> `TextDelta` 与现有 `AnswerDelta` 的语义差别：前者是 LLM 流式 token，后者是 finalize 后的整段答案。改造后只保留前者。

### 4.4 ModelCaller 改造

```rust
// model_caller.rs:184 当前
StreamChunk::TextDelta(delta) => {
    if !delta.is_empty() { text_buffer.push_str(&delta); }   // ← 仅塞 buffer
}

// 改造后
StreamChunk::TextDelta(delta) => {
    if !delta.is_empty() {
        // 1. 真流发布
        self.publisher
            .publish(AgentStreamEvent::TextDelta {
                run_id: run_id.to_string(),
                delta: delta.clone(),
            })
            .await?;
        // 2. 同时聚合（用于 parse_decision 判定无 tool_call 时的 finalize）
        text_buffer.push_str(&delta);
    }
}
```

### 4.5 DecisionDispatcher::dispatch_finalize 改造

```rust
// 改造后：删除 AnswerDelta publish，因为内容在 ModelCaller 里已经流出
pub async fn dispatch_finalize(
    &self, run_id: &str, step: u32, answer: &str, session_id: &str,
) -> AgentResult<()> {
    info!(run_id = %run_id, step = step, "Agent 产出最终回答");

    // 仅落库 + 写记忆，不再 publish 任何"伪流"
    let answer_event = AgentStreamEvent::TextDelta {
        run_id: run_id.to_string(),
        delta: answer.to_string(),
    };
    let _ = self.services.session.append_event(session_id, &answer_event).await;

    let assistant_memory = MemoryEntry::assistant_answer(answer.to_string());
    let _ = self.services.session.record_memory(session_id, run_id, &assistant_memory).await;

    Ok(())
}
```

> 说明：当 `ModelCaller` 实时流出 token 后，`finalize` 拿到的 `answer` 就是 token 聚合结果，再次 publish 会导致前端拿到双倍内容。

### 4.6 HTTP 层改造

```rust
// service-hub/agent-service/src/openai/controller.rs
pub async fn chat_completions(
    ctx: Context,
    Extension(service): Extension<Arc<AgentRunService>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Response, ResponseErr> {
    if req.stream.unwrap_or(false) {
        let sse = service.chat_completions_stream(ctx.get_user_id(), req).await?;
        Ok(Sse::new(sse).into_response())
    } else {
        let resp = service.chat_completions(ctx.get_user_id(), req).await?;
        Ok(Json(resp).into_response())
    }
}
```

`AgentRunService` 内部：

```rust
pub async fn chat_completions_stream(
    &self, user_id: i32, req: ChatCompletionRequest,
) -> anyhow::Result<impl Stream<Item = Result<Event, Infallible>>> {
    let agent = self.build_runtime(req.agent_id, user_id).await?;
    let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let user_input = extract_last_user_message(&req.messages)?;

    let mut event_stream = agent.run_stream(&session_id, user_id, &user_input)?;
    let mut adapter = OpenAIStreamAdapter::new(/* run_id slot */, req.model.clone());

    let stream = async_stream::stream! {
        while let Some(event) = event_stream.next().await {
            for out in adapter.translate(event) {
                match out {
                    AdapterOutput::Chunk(c) => yield Ok(Event::default().data(serde_json::to_string(&c)?)),
                    AdapterOutput::Extension { event_name, payload } =>
                        yield Ok(Event::default().event(event_name).data(payload.to_string())),
                    AdapterOutput::Done => {
                        yield Ok(Event::default().data("[DONE]"));
                        break;
                    }
                    AdapterOutput::Skip => continue,
                }
            }
        }
    };
    Ok(stream)
}

pub async fn chat_completions(
    &self, user_id: i32, req: ChatCompletionRequest,
) -> anyhow::Result<ChatCompletion> {
    // 内部仍用流式聚合，最后产出非流式响应
    let mut event_stream = /* run_stream(...) */;
    let mut content = String::new();
    let mut run_id = None;
    let mut finish = FinishReason::Stop;
    while let Some(ev) = event_stream.next().await {
        match ev {
            AgentStreamEvent::RunStarted { run_id: rid, .. } => run_id = Some(rid),
            AgentStreamEvent::TextDelta { delta, .. } => content.push_str(&delta),
            AgentStreamEvent::RunFinished { finish_reason, .. } => {
                finish = map_finish(finish_reason);
                break;
            }
            AgentStreamEvent::Error { code, message, .. } =>
                return Err(anyhow!("{code}: {message}")),
            _ => {}
        }
    }
    Ok(ChatCompletion { id: format!("chatcmpl-{}", run_id.unwrap()), /* ... */ })
}
```

---

## 5. 路由变更

```diff
- POST /agent/run            // 旧：自定义 AgentRunResp
- POST /agent/run/stream     // 旧：自定义 SSE event 流
- POST /agent/debug/run      // 旧：debug 模式 SSE
+ POST /v1/chat/completions  // 新：OpenAI 兼容（stream 由 body 字段控制）
+ POST /v1/chat/completions/debug  // 可选：保留 debug，扩展事件全开
```

---

## 6. 实施步骤（建议顺序）

1. **Step 1**：在 `agent-base` 新增 `openai/` 模块（chunk.rs / completion.rs / stream_adapter.rs）
2. **Step 2**：扩展 `AgentStreamEvent::TextDelta`，删除 `AnswerDelta` 变体
3. **Step 3**：改造 `ModelCaller::call`（publish TextDelta）
4. **Step 4**：改造 `dispatch_finalize`（不再 publish）
5. **Step 5**：在 `agent-service` 新增 `openai/` 模块（dto/controller/sse）
6. **Step 6**：删除 `SseEventData` 与旧路由
7. **Step 7**：更新所有单元测试与集成测试
8. **Step 8**：用 OpenAI Python SDK 做 E2E 验证

每一步都可独立编译通过（除了 Step 2 删除 `AnswerDelta` 时需要同步改 dispatcher 与 service.run）。

---

## 7. 注意事项

1. **`finish_reason` 映射**

   | 内部 `FinishReason` | OpenAI                        |
   | ------------------- | ----------------------------- |
   | `FinalizeAction`    | `stop`                        |
   | `MaxTurns`          | `length`                      |
   | `Error`             | `stop`（或 `error` 扩展事件） |
   | `Cancelled`         | `stop`                        |

2. **`tool_calls` 是否对外可见**

   当前架构下 agent **自治执行**所有工具，外部 OpenAI 客户端不会消费 `tool_calls`。因此默认**不**把 `delta.tool_calls` 写入主流，避免客户端误以为需要回灌 `tool` role 消息。

   如果将来要做"OpenAI 工具代理模式"（agent 不执行，把 tool_call 抛给客户端），再启用 `tool_calls` 主流。

3. **历史会话拼接**

   OpenAI 请求体 `messages: [{role, content}]` 与 nucleus 单条 `user_input` 不一致。建议：
   - 取 `messages` 中**最后一条** `role: user` 作为 `user_input`
   - 其余消息由 nucleus session 自身管理（基于 `session_id` 持久化）
   - 或者新增一个"transient session"模式，把 `messages` 完整丢给 `MessageContext`

   推荐前者（与 session 持久化一致）。

4. **多 choice 不支持**

   nucleus 一次只产出一个答案，`n` 字段只接受 `1`，否则 400。

5. **删除带来的破坏性影响**
   - 现有前端 / 客户端如果直接消费过 `event: answer_delta`，需要切到 OpenAI `data: {...}` 格式
   - 改造期内可短暂保留 `/agent/run/stream` 路由作为只读别名（仅做 SSE 透传），但 RFC 默认**不保留**

6. **测试要求**
   - 单元测试：`OpenAIStreamAdapter` 对所有 `AgentStreamEvent` 变体的转换
   - 集成测试：使用 `async-openai` crate 作为客户端串通同步 + 流式两条路径
   - 兼容性测试：OpenAI Python SDK `client.chat.completions.create(stream=True)`

---

## 8. 使用示例

### 8.1 同步调用（OpenAI Python SDK）

```python
from openai import OpenAI

client = OpenAI(base_url="https://nucleus.example.com/v1", api_key="...")

resp = client.chat.completions.create(
    model="agent_42",  # 实际是 nucleus agent_id 或别名
    messages=[{"role": "user", "content": "帮我搜索 Rust 异步运行时对比"}],
    extra_body={"agent_id": 42, "session_id": "sess-001"},
)
print(resp.choices[0].message.content)
```

### 8.2 流式调用（打字机效果）

```python
stream = client.chat.completions.create(
    model="agent_42",
    messages=[{"role": "user", "content": "你好"}],
    stream=True,
    extra_body={"agent_id": 42},
)

for chunk in stream:
    delta = chunk.choices[0].delta
    if delta.content:
        print(delta.content, end="", flush=True)
```

### 8.3 流式 + 监听 nucleus 扩展事件（curl）

```bash
curl -N -X POST https://nucleus.example.com/v1/chat/completions \
  -H "Authorization: Bearer ..." \
  -H "Content-Type: application/json" \
  -d '{
    "model": "agent_42",
    "messages": [{"role":"user","content":"分析这个仓库"}],
    "stream": true,
    "agent_id": 42
  }'

# 输出（节选）
event: nucleus.step.started
data: {"step":1}

data: {"id":"chatcmpl-...","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-...","choices":[{"index":0,"delta":{"content":"我"},"finish_reason":null}]}

data: {"id":"chatcmpl-...","choices":[{"index":0,"delta":{"content":"先"},"finish_reason":null}]}

event: nucleus.tool_call.finished
data: {"toolname":"list_dir","call_id":"call_1","success":true,"duration_ms":15}

data: {"id":"chatcmpl-...","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

---

## 9. 兼容性说明

- **不向后兼容**：旧 `/agent/run` / `/agent/run/stream` 路由及 `AgentStreamEvent` JSON 输出格式将被删除
- **rig-core 兼容**：本改造不依赖 rig 升级，只在 nucleus 适配层完成转换
- **OpenAI 协议版本**：对齐 2024-11 规范（`reasoning_content` / `tool_calls index` / `[DONE]`）
- **国内大模型互通**：`reasoning_content` 字段命名与 DeepSeek / 通义千问一致，前端可复用

---

## 10. 涉及的组件/模块清单

| 组件                                           | 文件                      | 改动类型                                   |
| ---------------------------------------------- | ------------------------- | ------------------------------------------ |
| `agent-base::openai`                           | 新模块                    | NEW                                        |
| `agent-base::session::event`                   | `event.rs`                | MODIFY（增 `TextDelta`，删 `AnswerDelta`） |
| `agent-base::runtime::llm::ModelCaller`        | `model_caller.rs`         | MODIFY（publish `TextDelta`）              |
| `agent-base::runtime::llm::DecisionDispatcher` | `decision_dispatcher.rs`  | MODIFY（`dispatch_finalize` 不再 publish） |
| `agent-service::openai`                        | 新模块                    | NEW                                        |
| `agent-service::dto::agent_run::SseEventData`  | `dto/agent_run.rs`        | DELETE                                     |
| `agent-service::service::agent_run::run`       | `service/agent_run.rs`    | MODIFY（事件匹配 `TextDelta`）             |
| `agent-service::controller::agent_run`         | `controller/agent_run.rs` | DELETE / MIGRATE                           |
| `agent-service` 路由表                         | `router.rs`               | MODIFY                                     |
