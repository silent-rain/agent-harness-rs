//! 构建 OpenAI 兼容的 [`ModelAdapter`]。

use std::sync::Arc;

use rig_core::{client::CompletionClient, providers::openai};

use crate::{AgentResult, Error, ModelAdapter, types::runtime::ModelConfig};

/// 根据 [`ModelBindingConfig`] 构建 OpenAI 兼容的 [`ModelAdapter`]。
///
/// 使用 `completions_api()` 切换到 Chat Completions 协议，以最大化与
/// OpenAI 兼容服务（OpenAI / DeepSeek / Qwen / 本地 vLLM 等）的互通性。
pub fn create_model(binding: &ModelConfig) -> AgentResult<Arc<dyn ModelAdapter>> {
    let client = openai::Client::builder()
        .api_key(binding.api_key.as_str())
        .base_url(binding.base_url.as_str())
        .build()
        .map_err(|e| Error::ModelCall(format!("build openai client failed: {e}")))?
        .completions_api();

    let model = client.completion_model(binding.model.as_str());
    Ok(Arc::new(model))
}
