//! 构建 OpenAI 兼容的 [`ModelAdapter`]。

use std::sync::Arc;

use rig_core::{
    Model,
    providers::openai::{self, OpenAIConfig},
};

use crate::{AgentResult, ModelAdapter, types::runtime::ModelConfig};

/// 根据 [`ModelConfig`] 构建 OpenAI 兼容的 [`ModelAdapter`]。
///
/// 统一走 Chat Completions 协议（`OpenAIConfig::chat`），以最大化与
/// OpenAI 兼容服务（OpenAI / DeepSeek / Qwen / 本地 vLLM 等）的互通性。
/// rig-core 0.45 的 HTTP transport 由 `rig-reqwest` 提供，默认使用进程级
/// 共享客户端（共享连接池）。
///
/// # Parameters
///
/// * `binding` - 模型绑定配置（api_key / base_url / model）
///
/// # Returns
///
/// 类型擦除后的模型适配器
pub fn create_model(binding: &ModelConfig) -> AgentResult<Arc<dyn ModelAdapter>> {
    let config = OpenAIConfig::with_key(&openai::wire::OPENAI, binding.api_key.as_str())
        .with_base_url(binding.base_url.as_str());

    let wire = config.chat(binding.model.as_str());
    let model = Model::new(wire, rig_reqwest::shared());

    Ok(Arc::new(model.erase()))
}
