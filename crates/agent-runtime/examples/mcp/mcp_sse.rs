//! MCP SSE 传输示例
//!
//! 演示如何通过 **SSE（Server-Sent Events）** 传输类型连接远程 MCP 服务。
//!
//! SSE 传输通过 HTTP 长连接接收服务端推送事件，适用于：
//! - 已部署的远程 MCP 服务（云端或局域网）
//! - 需要持久连接的实时工具服务
//! - 与现有 HTTP 基础设施集成
//!
//! # 前提
//!
//! 需要一个运行中的 SSE MCP 服务。可以使用以下命令在本地启动测试服务：
//!
//! ```sh
//! # 方式一：使用 mcp-proxy 将 stdio 服务转为 SSE
//! npx -y @modelcontextprotocol/server-filesystem . --port 8081
//!
//! # 方式二：使用任意支持 SSE 传输的 MCP 服务
//! ```
//!
//! # 运行
//!
//! ```sh
//! OPENAI_API_KEY=sk-xxx MCP_SSE_URL=http://localhost:8081/sse \
//!   cargo run --example mcp_sse --features mcp -p agent-runtime
//! ```

use std::env;

use agent_base::{
    error::AgentResult,
    mcp::McpRegistry,
    runtime::{Agent, AgentBuilder},
    types::{
        mcp::{McpServerConfig, McpServiceEntry},
        runtime::{AgentConfig, AgentDefinition, ModelConfig},
    },
};
use dotenv::dotenv;

// === 环境变量名常量 ===

/// API Key 环境变量名（必填）。
const ENV_API_KEY: &str = "OPENAI_API_KEY";
/// API 基础地址环境变量名。
const ENV_BASE_URL: &str = "OPENAI_BASE_URL";
/// 模型编码环境变量名。
const ENV_MODEL: &str = "OPENAI_MODEL";
/// 供应商标识环境变量名。
const ENV_PROVIDER: &str = "OPENAI_PROVIDER";
/// MCP SSE 服务地址环境变量名。
const ENV_MCP_SSE_URL: &str = "MCP_SSE_URL";

// === 默认值 ===

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_PROVIDER: &str = "openai";
/// 默认 SSE 服务地址（本地测试）。
const DEFAULT_MCP_SSE_URL: &str = "http://localhost:8081/sse";

/// 从环境变量加载模型配置。
fn load_model_config() -> AgentResult<ModelConfig> {
    let api_key = env::var(ENV_API_KEY).map_err(|_| {
        agent_base::Error::ModelCall(format!(
            "missing env `{ENV_API_KEY}`: please set it in `.env` or shell environment"
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

/// 创建智能体配置（远程工具助手）。
fn create_agent_config() -> AgentResult<AgentConfig> {
    Ok(AgentConfig {
        agent_definition: AgentDefinition {
            id: 2,
            user_id: "demo".to_string(),
            name: "远程工具助手".to_string(),
            description: "通过 MCP SSE 传输连接远程工具服务的智能体".to_string(),
            welcome_message: Some(
                "你好！我通过 SSE 连接到远程 MCP 服务，可以使用远端工具帮你完成任务。".to_string(),
            ),
            system_prompt: concat!(
                "你是一个远程工具助手，通过 MCP SSE 协议连接到远程工具服务。\n",
                "你可以使用提供的工具来完成用户请求的各种任务。\n",
                "请始终以中文回复用户，并在使用工具时说明你正在做什么。"
            )
            .to_string(),
            max_delegation_depth: 1,
        },
        model_config: load_model_config()?,
    })
}

/// 构建 SSE 传输的 MCP 服务配置。
///
/// SSE 传输通过 HTTP 长连接与远程 MCP 服务通信。
/// 服务地址从环境变量 `MCP_SSE_URL` 读取，默认为本地测试地址。
fn create_sse_mcp_config() -> McpServerConfig {
    let url = env::var(ENV_MCP_SSE_URL).unwrap_or_else(|_| DEFAULT_MCP_SSE_URL.to_string());

    McpServerConfig {
        service_name: "remote-sse-service".to_string(),
        // 允许所有工具
        allowed_tools: vec!["*".to_string()],
        server_config: McpServiceEntry {
            // SSE 传输：通过 HTTP 长连接接收服务端推送
            transport_type: Some("sse".to_string()),
            url: Some(url),
            // 连接超时：30 秒
            timeout: Some(30_000),
            ..Default::default()
        },
    }
}

/// 构建带自定义请求头的 SSE MCP 服务配置。
///
/// 适用于需要 API Key 认证或自定义请求头的远程 MCP 服务。
fn create_sse_mcp_config_with_auth(api_key: &str) -> McpServerConfig {
    let url = env::var(ENV_MCP_SSE_URL).unwrap_or_else(|_| DEFAULT_MCP_SSE_URL.to_string());

    McpServerConfig {
        service_name: "authenticated-sse-service".to_string(),
        allowed_tools: vec!["search".to_string(), "fetch".to_string()],
        server_config: McpServiceEntry {
            transport_type: Some("sse".to_string()),
            url: Some(url),
            timeout: Some(30_000),
            // 自定义 HTTP 请求头（用于认证）
            headers: std::collections::HashMap::from([
                ("Authorization".to_string(), format!("Bearer {api_key}")),
                ("X-Client-Version".to_string(), "1.0.0".to_string()),
            ]),
            ..Default::default()
        },
    }
}

/// 场景 1：通过 AgentBuilder 便捷 API 使用 SSE MCP 服务。
///
/// 最简洁的用法：调用 `add_mcps` 注入 SSE 配置，
/// `build()` 阶段自动完成连接与工具注入。
async fn scenario_builder_api() -> AgentResult<()> {
    println!("\n=== 场景 1：AgentBuilder 便捷 API（SSE）===");

    let agent = AgentBuilder::new(create_agent_config()?)?
        .add_mcps(vec![create_sse_mcp_config()])
        .build()
        .await?;

    let result = agent
        .run("sse-session-1", "你现在有哪些可用的工具？请列出来。")
        .await?;

    println!("回答: {}", result.final_answer);
    Ok(())
}

/// 场景 2：手动构建 McpRegistry，连接前检查服务状态。
///
/// 适用于需要在注入前验证连接、查看服务描述信息的场景。
async fn scenario_manual_registry() -> AgentResult<()> {
    println!("\n=== 场景 2：手动构建 McpRegistry（SSE）===");

    let mut registry = McpRegistry::new();
    registry.add_config(create_sse_mcp_config());

    // 连接失败时优雅降级
    if let Err(e) = registry.connect_all().await {
        println!("SSE 连接失败（请确认服务已启动）: {e}");
        println!("提示: 可通过 MCP_SSE_URL 环境变量指定服务地址");
        return Ok(());
    }

    println!("已连接服务数: {}", registry.connection_count());

    // 查看服务描述信息（通过 MCP 握手获取）
    if let Ok(desc) = registry.get_service_description("remote-sse-service") {
        println!("服务名称: {}", desc.server_name);
        println!("服务版本: {}", desc.server_version);
        if let Some(instructions) = &desc.instructions {
            println!("使用说明: {instructions}");
        }
    }

    // 查看可用工具
    let tools = registry.list_all_enabled_tools().await?;
    println!("可用工具数量: {}", tools.len());
    for (_, tool) in &tools {
        println!(
            "  - {} : {}",
            tool.name,
            tool.description.as_deref().unwrap_or("无描述")
        );
    }

    let agent = AgentBuilder::new(create_agent_config()?)?
        .with_mcp_registry(registry)
        .build()
        .await?;

    let result = agent
        .run("sse-session-2", "帮我用工具完成一个简单的任务。")
        .await?;

    println!("回答: {}", result.final_answer);
    Ok(())
}

/// 场景 3：带认证头的 SSE 服务 + 工具白名单过滤。
///
/// 演示如何连接需要 API Key 认证的远程 MCP 服务，
/// 并通过 `allowed_tools` 白名单限制可用工具范围。
async fn scenario_authenticated_with_whitelist() -> AgentResult<()> {
    println!("\n=== 场景 3：带认证头的 SSE 服务 + 工具白名单 ===");

    // 从环境变量读取远程服务的 API Key（示例）
    let remote_api_key = env::var("REMOTE_MCP_API_KEY").unwrap_or_else(|_| "demo-key".to_string());

    let agent = AgentBuilder::new(create_agent_config()?)?
        .add_mcps(vec![create_sse_mcp_config_with_auth(&remote_api_key)])
        .build()
        .await?;

    let result = agent
        .run(
            "sse-session-3",
            "你当前可以使用哪些工具？（白名单限制为 search 和 fetch）",
        )
        .await?;

    println!("回答: {}", result.final_answer);
    Ok(())
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    dotenv().ok();

    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let sse_url = env::var(ENV_MCP_SSE_URL).unwrap_or_else(|_| DEFAULT_MCP_SSE_URL.to_string());

    println!("=== MCP SSE 传输示例 ===");
    println!("传输类型: SSE（Server-Sent Events）");
    println!("服务地址: {sse_url}");
    println!("提示: 可通过 MCP_SSE_URL 环境变量覆盖服务地址");

    // 场景 1：AgentBuilder 便捷 API
    if let Err(e) = scenario_builder_api().await {
        println!("场景 1 失败: {e}");
    }

    // 场景 2：手动构建 McpRegistry
    if let Err(e) = scenario_manual_registry().await {
        println!("场景 2 失败: {e}");
    }

    // 场景 3：带认证头 + 工具白名单
    if let Err(e) = scenario_authenticated_with_whitelist().await {
        println!("场景 3 失败: {e}");
    }

    println!("\n=== 示例完成 ===");
    Ok(())
}
