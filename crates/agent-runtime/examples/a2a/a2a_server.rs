//! A2A Server 示例
//!
//! 演示如何将本地 Agent 一键暴露为 A2A v1.0 服务。
//!
//! 模型配置从 workspace 根目录的 `.env` 文件读取，可参考 `.env.example`。
//!
//! # 环境变量
//!
//! - `OPENAI_API_KEY`（必填）
//! - `OPENAI_BASE_URL`（可选）
//! - `OPENAI_MODEL`（可选，默认 `gpt-4o`）
//!
//! # 运行
//!
//! ```sh
//! cargo run --example a2a_server --features a2a -p agent-runtime
//! ```
//!
//! # 测试（另开终端）
//!
//! 查看 AgentCard：
//! ```sh
//! curl http://localhost:3000/.well-known/agent-card.json | jq .
//! ```
//!
//! 非流式调用：
//! ```sh
//! curl -X POST http://localhost:3000/v1/rpc \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "jsonrpc": "2.0",
//!     "id": 1,
//!     "method": "message/send",
//!     "params": {
//!       "message": {
//!         "messageId": "msg-1",
//!         "role": "ROLE_USER",
//!         "parts": [{"text": "Hello, agent!"}]
//!       }
//!     }
//!   }'
//! ```
//!
//! 流式调用（SSE）：
//! ```sh
//! curl -N -X POST http://localhost:3000/v1/rpc \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "jsonrpc": "2.0",
//!     "id": 1,
//!     "method": "message/stream",
//!     "params": {
//!       "message": {
//!         "messageId": "msg-2",
//!         "role": "ROLE_USER",
//!         "parts": [{"text": "Tell me a joke"}]
//!       }
//!     }
//!   }'
//! ```

use std::env;
use std::sync::Arc;

use agent_base::{
    a2a::A2aServer,
    error::AgentResult,
    runtime::AgentBuilder,
    types::runtime::{AgentBundle, AgentConfig, AgentDefinition, ModelConfig},
};
use dotenv::dotenv;
use tracing::info;
use tracing_subscriber::EnvFilter;

const ENV_API_KEY: &str = "OPENAI_API_KEY";
const ENV_BASE_URL: &str = "OPENAI_BASE_URL";
const ENV_MODEL: &str = "OPENAI_MODEL";
const ENV_PROVIDER: &str = "OPENAI_PROVIDER";

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_PROVIDER: &str = "openai";

fn load_model_config() -> AgentResult<ModelConfig> {
    let api_key = env::var(ENV_API_KEY).map_err(|_| {
        agent_base::Error::ModelCall(format!(
            "missing env `{ENV_API_KEY}`: please configure it in `.env`"
        ))
    })?;

    Ok(ModelConfig {
        provider: env::var(ENV_PROVIDER).unwrap_or_else(|_| DEFAULT_PROVIDER.to_string()),
        model: env::var(ENV_MODEL).unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
        base_url: env::var(ENV_BASE_URL).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
        api_key,
        temperature: Some(0.7),
        max_tokens: None,
    })
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    info!("Building agent...");

    let config = AgentConfig {
        agent_definition: AgentDefinition {
            id: 1,
            name: "demo-agent".to_string(),
            description: "A demo agent exposed via A2A protocol".to_string(),
            system_prompt: "You are a helpful assistant.".to_string(),
            ..Default::default()
        },
        model_config: load_model_config()?,
    };

    // 构造 AgentBundle（用于生成 AgentCard）
    let bundle = AgentBundle {
        agent_definition: config.agent_definition.clone(),
        model_config: config.model_config.clone(),
        ..Default::default()
    };

    let agent = Arc::new(AgentBuilder::new(config)?.build().await?);
    info!("Starting A2A Server on 0.0.0.0:3000...");
    info!("AgentCard: http://localhost:3000/.well-known/agent-card.json");
    info!("JSON-RPC:  POST http://localhost:3000/v1/rpc");
    info!("REST:      POST http://localhost:3000/v1/message:send");
    info!("Stream:    POST http://localhost:3000/v1/message:stream");

    // 一行启动 A2A 服务
    A2aServer::new(agent, bundle).serve("0.0.0.0:3000").await?;

    Ok(())
}
