//! LLM 模型适配器
//!
//! 提供 object-safe 的 [`ModelAdapter`] trait，作为 rig-core 0.45
//! [`DynModel<Completion>`]（类型擦除后的补全模型）的运行时包装。
//! 通过 blanket impl，任何 `DynModel<Completion>` 自动获得 `ModelAdapter`
//! 实现，用户无需手动实现此 trait。
//!
//! 提供的主要内容：
//! - [`ModelAdapter`]: object-safe 的 LLM 模型调用 trait（同时支持非流式与 token 流式）
//! - blanket impl: 自动为所有 `rig_core::DynModel<Completion>` 实现 `ModelAdapter`

use async_trait::async_trait;
use futures::StreamExt;
use rig_core::{
    completion::{CompletionRequest, CompletionResponse},
    driver::DynModel,
    operation::Completion,
};

use crate::{
    error::{AgentResult, Error},
    model::streaming::{BoxedCompletionStream, StreamChunk, conv},
};

/// Object-safe 的 LLM 模型调用 trait
///
/// 对 rig-core `DynModel<Completion>` 的类型擦除包装，使得 `Agent` 可以
/// 通过 `Arc<dyn ModelAdapter>` 持有任意 provider 的模型实例。
///
/// **用户无需手动实现此 trait**，rig-core 的补全模型（`DynModel<Completion>`）
/// 会通过 blanket impl 自动获得 `ModelAdapter` 实现。
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use agent_runtime::model::ModelAdapter;
///
/// // 任何 rig 补全模型自动实现 ModelAdapter
/// // let model = create_model(&config)?;
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
    /// 推理响应（provider 原始载荷保存在 `raw` 字段，不做类型约束）
    async fn completion(&self, request: CompletionRequest) -> AgentResult<CompletionResponse>;

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

/// 为所有 `DynModel<Completion>` 自动实现 `ModelAdapter`
///
/// 通过此 blanket impl，用户只需把 provider 模型 `erase()` 成
/// `DynModel<Completion>`（再用 `Arc::new` 装箱），即可自动获得
/// `ModelAdapter` 实现，无需额外适配代码。
#[async_trait]
impl ModelAdapter for DynModel<Completion> {
    async fn completion(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        DynModel::call(self, request)
            .await
            .map_err(|e| Error::ModelCall(e.to_string()))
    }

    async fn stream_completion(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<BoxedCompletionStream> {
        let mut response =
            DynModel::stream(self, request).map_err(|e| Error::ModelCall(e.to_string()))?;

        // 把每个流式事件映射为 StreamChunk，流结束时输出带 usage 的 Done。
        let stream = async_stream::try_stream! {
            while let Some(item) = response.next().await {
                let item = item.map_err(|e| Error::ModelCall(e.to_string()))?;
                if let Some(chunk) = conv::map_stream_event(&item) {
                    yield chunk;
                }
            }
            let usage = response.partial().usage;
            yield StreamChunk::Done { usage: Some(usage) };
        };

        Ok(Box::pin(stream))
    }
}
