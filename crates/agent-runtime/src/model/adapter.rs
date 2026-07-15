//! LLM 模型适配器
//!
//! 提供 object-safe 的 [`ModelAdapter`] trait，作为 rig-core `CompletionModel`
//! 的类型擦除包装。通过 blanket impl，任何实现了 `CompletionModel` 的类型
//! 自动获得 `ModelAdapter` 实现，用户无需手动实现此 trait。
//!
//! 提供的主要内容：
//! - [`ModelAdapter`]: object-safe 的 LLM 模型调用 trait（同时支持非流式与 token 流式）
//! - blanket impl: 自动为所有 `rig::CompletionModel` 实现 `ModelAdapter`

use async_trait::async_trait;
use futures::StreamExt;
use rig_core::completion::{CompletionModel, CompletionRequest, CompletionResponse};

use crate::{
    error::{AgentResult, Error},
    model::streaming::{BoxedCompletionStream, StreamChunk, conv},
};

/// Object-safe 的 LLM 模型调用 trait
///
/// 对 rig-core `CompletionModel` 的类型擦除包装，使得 `Agent` 可以
/// 通过 `Arc<dyn ModelAdapter>` 持有任意 provider 的模型实例。
///
/// **用户无需手动实现此 trait**，所有实现了 `rig::CompletionModel` 的类型
/// 会通过 blanket impl 自动获得 `ModelAdapter` 实现。
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use agent_base::model::ModelAdapter;
///
/// // 任何 rig CompletionModel 自动实现 ModelAdapter
/// // let model = openai::Client::from_env().completion_model("gpt-4o");
/// // let adapter: Arc<dyn ModelAdapter> = Arc::new(model);
/// ```
#[async_trait]
pub trait ModelAdapter: Send + Sync + 'static {
    /// 调用 LLM 完成一次推理（非流式）
    ///
    /// # Parameters
    ///
    /// * `request` - rig-core 的 CompletionRequest
    ///
    /// # Returns
    ///
    /// 推理响应（类型擦除后 raw_response 为 `()`）
    async fn completion(&self, request: CompletionRequest) -> AgentResult<CompletionResponse<()>>;

    /// 调用 LLM 进行 token 级流式推理
    ///
    /// 返回一个产出 [`StreamChunk`] 的异步流。流终止时会产生
    /// 一个 `StreamChunk::Done` 事件。
    ///
    /// **此方法没有默认实现**——先前提供的 "先 await 完整响应、再切片成 chunk"
    /// 的回退实现是伪流式语义（所有 chunk 在同一时刻一次性涌出），
    /// 容易掩盖实现方未真正支持 streaming 的事实。所有实现者必须显式选择：
    ///
    /// - 真实 provider（rig `CompletionModel`）通过下文的 blanket impl 自动获得真流式实现
    /// - 测试 mock 应使用 `futures::stream::iter` 直接产出预设 [`StreamChunk`] 序列
    ///   （或在 crate 内部复用 `crate::model::mock::completion_to_chunks`）
    ///
    /// # Parameters
    ///
    /// * `request` - rig-core 的 CompletionRequest
    ///
    /// # Returns
    ///
    /// 装箱的 [`BoxedCompletionStream`]
    async fn stream_completion(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<BoxedCompletionStream>;
}

/// 为所有 `CompletionModel` 自动实现 `ModelAdapter`
///
/// 通过此 blanket impl，用户只需传入任何 rig model 实例（再用 `Arc::new` 装箱），
/// 即可自动获得 `ModelAdapter` 实现，无需额外适配代码。
#[async_trait]
impl<M> ModelAdapter for M
where
    M: CompletionModel + Send + Sync + 'static,
{
    async fn completion(&self, request: CompletionRequest) -> AgentResult<CompletionResponse<()>> {
        let response = CompletionModel::completion(self, request)
            .await
            .map_err(|e| Error::ModelCall(e.to_string()))?;

        Ok(CompletionResponse {
            choice: response.choice,
            usage: response.usage,
            raw_response: (),
            message_id: response.message_id,
        })
    }

    async fn stream_completion(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<BoxedCompletionStream> {
        let mut response = CompletionModel::stream(self, request)
            .await
            .map_err(|e| Error::ModelCall(e.to_string()))?;

        // 把每个 StreamedAssistantContent 映射为 StreamChunk，流结束时输出 Done。
        let stream = async_stream::try_stream! {
            while let Some(item) = response.next().await {
                match item {
                    Ok(content) => {
                        if let Some(chunk) = conv::map_streamed_content::<M::StreamingResponse>(content) {
                            yield chunk;
                        }
                    }
                    Err(e) => Err(Error::ModelCall(e.to_string()))?,
                }
            }
            yield StreamChunk::Done { usage: None };
        };

        Ok(Box::pin(stream))
    }
}
