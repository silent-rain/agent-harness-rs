//! Agent 顶层 trait 与扩展能力 trait
//!
//! 提供的主要内容：
//! - [`Agent`] - 顶层智能体抽象（所有执行模式的统一接口）
//! - [`AgentOpenAI`] - OpenAI 兼容协议扩展（非流式/流式 + 完整循环/单次推理）

use async_openai::types::chat::{ChatCompletionMessageToolCall, FunctionCall};
use async_trait::async_trait;
use futures::StreamExt;

use crate::{
    error::AgentResult,
    openai::{
        ChatCompletion, ChatCompletionChunk, ChatCompletionMessage, ChatCompletionMessageToolCalls,
        CompletionChoice, CompletionUsage, FinishReason as OpenAIFinishReason, Role,
        raw_stream_adapter::{OpenAIRawStreamAdapter, RawAdapterOutput},
        stream_adapter::{AdapterOutput, OpenAIStreamAdapter},
    },
    session::{
        AgentEventStream, AgentStreamEvent, BoxStream, FinishReason as InternalFinishReason,
        RunOutcome,
    },
    types::runtime::AgentBundle,
};

/// OpenAI 兼容的流式输出类型
pub type OpenAIChunkStream = BoxStream<ChatCompletionChunk>;

/// OpenAI 非流式响应对象类型字面量
const OBJECT_COMPLETION: &str = "chat.completion";

/// 智能体顶层 trait
///
/// 描述"如何执行一次智能体运行"。同一个调用方可挂载不同实现，
/// 实现 LLM 单循环（[`LlmAgent`](crate::runtime::LlmAgent)）、链式
/// （[`ChainAgent`](crate::runtime::ChainAgent)）、并行
/// （[`ParallelAgent`](crate::runtime::ParallelAgent)）等不同编排模式。
///
/// ## 生命周期
///
/// ```text
/// Agent::run(session_id, input)
///   ├─► 内部按需发布事件（RunStarted / ToolCallStarted / ...）
///   ├─► 调用 model / toolset / storage（实现者自行装配）
///   └─► 返回 RunOutcome
/// ```
///
/// ## 调用约定
///
/// `user_id` 与 `agent_id` 不再随调用透传，而是在 Agent 实例化时通过
/// [`AgentDefinition`](crate::types::runtime::AgentDefinition) 一同绑定，
/// 实现者按需从 `agent_config.agent_definition` 读取。
///
/// ## 实现建议
///
/// - **自包含依赖**：实现者持有自己的 model / toolset / publisher，避免运行时穿参
/// - **事件驱动**：所有关键节点通过实现者持有的 publisher 发布事件
/// - **可组合**：高层实现（Chain / Parallel）应接受 `Vec<Arc<dyn Agent>>` 作为成员
///
/// # Examples
///
/// ```rust,no_run
/// use agent_base::runtime::Agent;
/// use agent_base::error::AgentResult;
/// use agent_base::session::RunOutcome;
///
/// struct MyAgent;
///
/// #[async_trait::async_trait]
/// impl Agent for MyAgent {
///     fn name(&self) -> &str { "my_agent" }
///
///     async fn run(
///         &self,
///         _session_id: &str,
///         _input: &str,
///     ) -> AgentResult<RunOutcome> {
///         Ok(RunOutcome::success("done", 1))
///     }
/// }
/// ```
#[async_trait]
pub trait Agent: Send + Sync + 'static {
    /// 智能体名称（用于日志、事件标识与组合场景的分支命名）
    fn name(&self) -> &str;

    /// 智能体所使用的模型
    fn model(&self) -> &str {
        ""
    }

    /// 执行一次智能体运行
    ///
    /// 实现者通过自身持有的 `publisher` 发布事件，函数返回时给出结构化结果。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    ///
    /// # Returns
    ///
    /// 运行结果（最终回答 + 步骤统计）
    async fn run(&self, session_id: &str, input: &str) -> AgentResult<RunOutcome> {
        let stream = self.run_stream(session_id, input)?;
        Ok(RunOutcome::from_stream(stream).await)
    }

    /// 流式执行（默认实现：返回错误）
    ///
    /// 仅 [`LlmAgent`](crate::runtime::LlmAgent) 等承担"事件源"角色的实现需要重写此方法。
    /// 组合型实现（Chain/Parallel）应通过 [`AgentExecutor`](crate::runtime::AgentExecutor)
    /// 包装后再获得流式接口。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    ///
    /// # Returns
    ///
    /// 事件流，消费端可逐事件处理
    fn run_stream(&self, _session_id: &str, _input: &str) -> AgentResult<AgentEventStream> {
        Err(crate::error::Error::Internal(format!(
            "Agent `{}` 不支持流式执行，请通过 AgentExecutor 包装后再使用",
            self.name()
        )))
    }

    /// 执行一次单轮 LLM 推理（不参与 ReAct 循环）
    ///
    /// 实现者通过自身持有的 `publisher` 发布事件，函数返回时给出结构化结果。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    ///
    /// # Returns
    ///
    /// 运行结果（最终回答 + 步骤统计）
    async fn infer(&self, session_id: &str, input: &str) -> AgentResult<RunOutcome> {
        let stream = self.infer_stream(session_id, input)?;
        Ok(RunOutcome::from_stream(stream).await)
    }

    /// 启动单轮 LLM 推理的事件流（默认实现：返回错误）
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    fn infer_stream(&self, _session_id: &str, _input: &str) -> AgentResult<AgentEventStream> {
        Err(crate::error::Error::Internal(format!(
            "Agent `{}` 不支持流式执行，请通过 AgentExecutor 包装后再使用",
            self.name()
        )))
    }
}

/// OpenAI 兼容协议扩展 trait
///
/// 继承 [`Agent`]，提供将内部事件流翻译为 OpenAI Chat Completions API
/// 兼容格式的能力。包含四个维度的接口：
///
/// | 方法 | 模式 | 说明 |
/// |------|------|------|
/// | [`run`](AgentOpenAI::run) | 非流式 + 完整循环 | 聚合事件流为 `ChatCompletion` |
/// | [`run_stream`](AgentOpenAI::run_stream) | 流式 + 完整循环 | 翻译事件流为 `ChatCompletionChunk` 流 |
/// | [`infer`](AgentOpenAI::infer) | 非流式 + 单次推理 | 单次 LLM 推理，不执行工具 |
/// | [`infer_stream`](AgentOpenAI::infer_stream) | 流式 + 单次推理 | 单次 LLM 推理流式输出 |
///
/// ## 默认实现
///
/// 所有四个方法均基于 [`Agent`] trait 的事件流接口提供默认实现：
///
/// - `run` / `run_stream` 基于 [`Agent::run_stream`] + [`OpenAIStreamAdapter`]
/// - `infer` / `infer_stream` 基于 [`Agent::infer_stream`] + [`OpenAIRawStreamAdapter`]
///
/// 任何实现了 `Agent::run_stream` 和 `Agent::infer_stream` 的智能体只需声明
/// `impl AgentOpenAI for MyAgent {}` 即可免费获得完整的 OpenAI 兼容能力。
///
/// ## 命名对齐
///
/// 方法名与 [`Agent`] trait 保持一致（`run` / `run_stream` / `infer` / `infer_stream`），
/// 通过 trait 名称区分语义：`Agent::run` 返回 `RunOutcome`，`AgentOpenAI::run` 返回 `ChatCompletion`。
#[async_trait]
pub trait AgentOpenAI: Agent {
    /// 非流式 OpenAI 兼容执行（完整 ReAct 循环）
    ///
    /// 内部委托 [`Agent::run_stream`]，将内部事件流聚合为
    /// 对齐 OpenAI Chat Completions API 的完整响应。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    ///
    /// # Returns
    ///
    /// OpenAI 标准非流式响应
    async fn run(&self, session_id: &str, input: &str) -> AgentResult<ChatCompletion> {
        let stream = Agent::run_stream(self, session_id, input)?;
        Ok(collect_stream_to_completion(stream, session_id, self.model()).await)
    }

    /// 流式 OpenAI 兼容执行（完整 ReAct 循环）
    ///
    /// 内部委托 [`Agent::run_stream`]，将内部事件流翻译为
    /// 对齐 OpenAI Chat Completions stream 的分片流。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    ///
    /// # Returns
    ///
    /// OpenAI 标准流式分片流
    fn run_stream(&self, session_id: &str, input: &str) -> AgentResult<OpenAIChunkStream> {
        let mut stream = Agent::run_stream(self, session_id, input)?;
        let model = self.model().to_string();
        let session_id = session_id.to_string();

        let output_stream = async_stream::stream! {
            let mut adapter = OpenAIStreamAdapter::new(&session_id, &model);
            while let Some(event) = stream.next().await {
                for output in adapter.translate(event) {
                    match output {
                        AdapterOutput::Chunk(chunk) => yield chunk,
                        AdapterOutput::Done => return,
                        _ => {}
                    }
                }
            }
        };

        Ok(Box::pin(output_stream))
    }

    /// 单轮 LLM 推理非流式调用（OpenAI 兼容）
    ///
    /// 内部委托 [`Agent::infer_stream`]，通过 [`OpenAIRawStreamAdapter`] 将
    /// 内部事件流聚合为对齐 OpenAI Chat Completions API 的完整响应。
    ///
    /// 与 [`Self::run`] 的区别：
    /// - 不走 ReAct 循环，仅执行一次 LLM 推理
    /// - 不自动执行工具调用（即使 LLM 返回了 tool_calls）
    /// - 返回的 `tool_calls` / `content` 反映 LLM 的原始决策
    ///
    /// ## 默认实现
    ///
    /// 基于 [`Agent::infer_stream`] + [`OpenAIRawStreamAdapter`] 提供默认实现，
    /// 任何实现了 `Agent::infer_stream` 的智能体无需覆盖此方法。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    ///
    /// # Returns
    ///
    /// OpenAI 标准非流式响应（`finish_reason` 为 `stop` 或 `tool_calls`）
    async fn infer(&self, session_id: &str, input: &str) -> AgentResult<ChatCompletion> {
        let stream = Agent::infer_stream(self, session_id, input)?;
        Ok(collect_stream_to_completion(stream, session_id, self.model()).await)
    }

    /// 单轮 LLM 推理流式调用（OpenAI 兼容）
    ///
    /// 内部委托 [`Agent::infer_stream`]，通过 [`OpenAIRawStreamAdapter`] 将
    /// 内部事件流翻译为对齐 OpenAI Chat Completions stream 的分片流。
    ///
    /// 与 [`Self::run_stream`] 的区别：
    /// - 不走 ReAct 循环，仅执行一次 LLM 推理
    /// - 不自动执行工具调用
    /// - 流式输出反映 LLM 的原始 token 产出（包括 tool_calls delta）
    ///
    /// ## 默认实现
    ///
    /// 基于 [`Agent::infer_stream`] + [`OpenAIRawStreamAdapter`] 提供默认实现，
    /// 任何实现了 `Agent::infer_stream` 的智能体无需覆盖此方法。
    ///
    /// # Parameters
    ///
    /// * `session_id` - 会话 ID
    /// * `input` - 用户输入文本
    ///
    /// # Returns
    ///
    /// OpenAI 标准流式分片流
    fn infer_stream(&self, session_id: &str, input: &str) -> AgentResult<OpenAIChunkStream> {
        let mut stream = Agent::infer_stream(self, session_id, input)?;
        let model = self.model().to_string();
        let session_id = session_id.to_string();

        let output_stream = async_stream::stream! {
            let mut adapter = OpenAIRawStreamAdapter::new(&session_id, &model);
            while let Some(event) = stream.next().await {
                let mut done = false;
                for output in adapter.translate(event) {
                    match output {
                        RawAdapterOutput::Chunk(chunk) => yield chunk,
                        RawAdapterOutput::Done => {
                            done = true;
                            break;
                        }
                        RawAdapterOutput::Skip => {}
                    }
                }
                if done {
                    break;
                }
            }
        };

        Ok(Box::pin(output_stream))
    }
}

// ─── 辅助函数 ──────────────────────────────────────

/// 将内部事件流聚合为 OpenAI Chat Completions 非流式响应
///
/// `run` 与 `infer` 共用此逻辑，仅事件流来源不同。
///
/// # Parameters
///
/// * `stream` - 内部事件流
/// * `session_id` - 会话 ID（用于生成响应 ID）
/// * `model` - 模型名称
async fn collect_stream_to_completion(
    mut stream: AgentEventStream,
    session_id: &str,
    model: &str,
) -> ChatCompletion {
    let mut content = String::new();
    let mut tool_calls: Vec<ChatCompletionMessageToolCalls> = Vec::new();
    let mut finish_reason = Some(OpenAIFinishReason::Stop);
    let mut usage: Option<rig_core::completion::Usage> = None;

    while let Some(event) = stream.next().await {
        match event {
            AgentStreamEvent::Answer {
                content: answer, ..
            } => {
                content = answer;
            }
            AgentStreamEvent::ToolCallsPlanned { calls, .. } => {
                for call in calls {
                    tool_calls.push(ChatCompletionMessageToolCalls::Function(
                        ChatCompletionMessageToolCall {
                            id: call.call_id,
                            function: FunctionCall {
                                name: call.tool_name,
                                arguments: call.arguments_json,
                            },
                        },
                    ));
                }
            }
            AgentStreamEvent::RunFinished {
                finish_reason: reason,
                usage: run_usage,
                ..
            } => {
                // 内部 FinishReason 映射到 OpenAI FinishReason
                finish_reason = Some(match reason {
                    InternalFinishReason::FinalizeAction => OpenAIFinishReason::Stop,
                    InternalFinishReason::MaxTurns => OpenAIFinishReason::Length,
                    InternalFinishReason::Error => OpenAIFinishReason::Stop,
                    InternalFinishReason::Cancelled => OpenAIFinishReason::Stop,
                });

                usage = run_usage;
                break;
            }
            _ => {}
        }
    }

    ChatCompletion {
        id: format!("chatcmpl-{}", session_id),
        object: OBJECT_COMPLETION.to_string(),
        created: chrono::Utc::now().timestamp() as u32,
        model: model.to_string(),
        choices: vec![CompletionChoice {
            index: 0,
            message: ChatCompletionMessage {
                role: Role::Assistant,
                content: if !tool_calls.is_empty() {
                    None
                } else {
                    Some(content)
                },
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                refusal: None,
                annotations: None,
                #[allow(deprecated)]
                function_call: None,
                audio: None,
            },
            finish_reason,
            logprobs: None,
        }],
        usage: Some(map_usage(usage)),
        service_tier: None,
        #[allow(deprecated)]
        system_fingerprint: None,
    }
}

/// 内部 Usage 映射到 OpenAI CompletionUsage
///
/// 将 rig-core 的 `Usage`（如有）映射为 `async-openai` 的 `CompletionUsage`。
/// 若为 `None` 则返回全零的默认值。
fn map_usage(usage: Option<rig_core::completion::Usage>) -> CompletionUsage {
    match usage {
        Some(u) => CompletionUsage {
            prompt_tokens: u.input_tokens as u32,
            completion_tokens: u.output_tokens as u32,
            total_tokens: u.total_tokens as u32,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        },
        None => CompletionUsage::default(),
    }
}

/// 智能体配置来源
///
/// 每个 source 加载并返回一个 [`AgentBundle`] 片段，由
/// [`AgentBuilder`](crate::runtime::AgentBuilder) 按注册顺序合并。
///
/// ## 合并语义
///
/// - **身份字段**（`agent_definition` / `model_config`）：仅当 source 返回非默认值时覆盖
/// - **能力字段**（rules / skills / mcp / commands / tools / sub_agents）：按顺序追加
///
/// ## 实现要求
///
/// 实现者必须满足 `Send + Sync` 以便在多线程异步场景中共享。
/// 不要求 `Clone`：调用方使用 `Arc<dyn AgentSource>` 引用计数复用。
#[async_trait]
pub trait AgentSource: Send + Sync {
    /// 加载并返回 bundle 片段
    ///
    /// # Returns
    ///
    /// 一个 [`AgentBundle`] 片段，未填充的字段保持默认值。
    async fn load(&self) -> AgentResult<AgentBundle>;
}
