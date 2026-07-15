//! MCP Streamable HTTP 传输示例
//!
//! 演示如何通过 **Streamable HTTP** 传输类型连接 MCP 服务。
//!
//! Streamable HTTP 是 MCP 协议推荐的现代传输方式，相比 SSE 具有以下优势：
//! - 支持双向流式通信（请求和响应均可流式传输）
//! - 更好的负载均衡和代理兼容性
//! - 支持连接复用，减少握手开销
//! - 适合高并发、低延迟的生产环境
//!
//! 适用场景：
//! - 生产环境的远程 MCP 服务
//! - 需要高性能流式工具调用的场景
//! - 与现代 HTTP/2 基础设施集成
//!
//! # 前提
//!
//! 需要一个支持 Streamable HTTP 传输的 MCP 服务。
//! 可以使用以下方式启动本地测试服务：
//!
//! ```sh
//! # 使用支持 streamable-http 的 MCP 服务器
//! npx -y @modelcontextprotocol/server-everything --port 8080
//! ```
//!
//! # 运行
//!
//! ```sh
//! OPENAI_API_KEY=sk-xxx MCP_HTTP_URL=http://localhost:8080/mcp \
//!   cargo run --example mcp_streamable_http --features mcp -p agent-base
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
/// MCP Streamable HTTP 服务地址环境变量名。
const ENV_MCP_HTTP_URL: &str = "MCP_HTTP_URL";

// === 默认值 ===

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_PROVIDER: &str = "openai";
/// 默认 Streamable HTTP 服务地址（本地测试）。
const DEFAULT_MCP_HTTP_URL: &str = "http://localhost:8080/mcp";

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

/// 创建智能体配置（高性能工具助手）。
fn create_agent_config() -> AgentResult<AgentConfig> {
    Ok(AgentConfig {
        agent_definition: AgentDefinition {
            id: 3,
            user_id: "demo".to_string(),
            name: "高性能工具助手".to_string(),
            description: "通过 MCP Streamable HTTP 传输连接高性能工具服务的智能体".to_string(),
            welcome_message: Some(
                "你好！我通过 Streamable HTTP 连接到高性能 MCP 服务，可以快速调用远端工具。"
                    .to_string(),
            ),
            system_prompt: concat!(
                "你是一个高性能工具助手，通过 MCP Streamable HTTP 协议连接到远程工具服务。\n",
                "你可以使用提供的工具高效地完成用户请求的各种任务。\n",
                "请始终以中文回复用户，并在使用工具时简要说明操作步骤。"
            )
            .to_string(),
            max_delegation_depth: 1,
        },
        model_config: load_model_config()?,
    })
}

/// 构建基础 Streamable HTTP MCP 服务配置。
///
/// Streamable HTTP 传输通过单一 HTTP 端点进行双向流式通信，
/// 服务地址从环境变量 `MCP_HTTP_URL` 读取，默认为本地测试地址。
fn create_http_mcp_config() -> McpServerConfig {
    let url = env::var(ENV_MCP_HTTP_URL).unwrap_or_else(|_| DEFAULT_MCP_HTTP_URL.to_string());

    McpServerConfig {
        service_name: "streamable-http-service".to_string(),
        // 允许所有工具
        allowed_tools: vec!["*".to_string()],
        server_config: McpServiceEntry {
            // Streamable HTTP 传输类型
            transport_type: Some("streamable_http".to_string()),
            url: Some(url),
            // 连接超时：60 秒（流式传输可能耗时较长）
            timeout: Some(60_000),
            ..Default::default()
        },
    }
}

/// 构建带认证和工具白名单的 Streamable HTTP MCP 服务配置。
///
/// 适用于生产环境中需要 Bearer Token 认证的 MCP 服务，
/// 同时通过 `allowed_tools` 精确控制可用工具范围。
fn create_http_mcp_config_production(service_url: &str, bearer_token: &str) -> McpServerConfig {
    McpServerConfig {
        service_name: "production-mcp-service".to_string(),
        // 精确指定允许的工具（生产环境推荐明确白名单）
        allowed_tools: vec![
            "web_search".to_string(),
            "code_execute".to_string(),
            "data_query".to_string(),
        ],
        server_config: McpServiceEntry {
            transport_type: Some("streamable_http".to_string()),
            url: Some(service_url.to_string()),
            timeout: Some(120_000),
            // 生产环境认证头
            headers: std::collections::HashMap::from([
                (
                    "Authorization".to_string(),
                    format!("Bearer {bearer_token}"),
                ),
                ("X-Agent-Version".to_string(), "1.0.0".to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
            ]),
            ..Default::default()
        },
    }
}

/// 场景 1：通过 AgentBuilder 便捷 API 使用 Streamable HTTP MCP 服务。
///
/// 最简洁的用法：调用 `add_mcps` 注入配置，
/// `build()` 阶段自动完成连接与工具注入。
async fn scenario_builder_api() -> AgentResult<()> {
    println!("\n=== 场景 1：AgentBuilder 便捷 API（Streamable HTTP）===");

    let agent = AgentBuilder::new(create_agent_config()?)?
        .add_mcps(vec![create_http_mcp_config()])
        .build()
        .await?;

    let result = agent
        .run(
            "http-session-1",
            "你现在有哪些可用的工具？请列出工具名称和功能描述。",
        )
        .await?;

    println!("回答: {}", result.final_answer);
    Ok(())
}

/// 场景 2：手动构建 McpRegistry，连接后查看服务详情。
///
/// 演示如何在注入 AgentBuilder 前，先通过 McpRegistry 检查服务状态
/// 和获取完整的工具列表（包含未过白名单过滤的全量工具）。
async fn scenario_manual_registry() -> AgentResult<()> {
    println!("\n=== 场景 2：手动构建 McpRegistry（Streamable HTTP）===");

    let mut registry = McpRegistry::new();
    registry.add_config(create_http_mcp_config());

    if let Err(e) = registry.connect_all().await {
        println!("Streamable HTTP 连接失败（请确认服务已启动）: {e}");
        println!("提示: 可通过 MCP_HTTP_URL 环境变量指定服务地址");
        return Ok(());
    }

    println!("已连接服务数: {}", registry.connection_count());
    println!("已连接服务列表: {:?}", registry.service_names());

    // 查看服务描述（通过 MCP 握手获取的元数据）
    if let Ok(desc) = registry.get_service_description("streamable-http-service") {
        println!("\n服务元数据:");
        println!("  名称: {}", desc.server_name);
        println!("  版本: {}", desc.server_version);
        if let Some(title) = &desc.server_title {
            println!("  标题: {title}");
        }
        if let Some(description) = &desc.server_description {
            println!("  描述: {description}");
        }
        println!("  协议版本: {}", desc.protocol_version);
    }

    // 查看白名单过滤后的可用工具
    let enabled_tools = registry.list_all_enabled_tools().await?;
    println!("\n白名单过滤后可用工具 ({} 个):", enabled_tools.len());
    for (_, tool) in &enabled_tools {
        println!(
            "  ✓ {} — {}",
            tool.name,
            tool.description.as_deref().unwrap_or("无描述")
        );
    }

    // 查看服务全量工具（不经过白名单过滤）
    if let Ok((_, all_tools)) = registry
        .get_service_all_tools("streamable-http-service")
        .await
    {
        println!("\n服务全量工具 ({} 个):", all_tools.len());
        for tool in &all_tools {
            println!(
                "  - {} — {}",
                tool.name,
                tool.description.as_deref().unwrap_or("无描述")
            );
        }
    }

    let agent = AgentBuilder::new(create_agent_config()?)?
        .with_mcp_registry(registry)
        .build()
        .await?;

    let result = agent
        .run("http-session-2", "请用工具帮我完成一个任务。")
        .await?;

    println!("\n回答: {}", result.final_answer);
    Ok(())
}

/// 场景 3：生产环境配置——带认证头、精确工具白名单、多服务混合。
///
/// 演示生产环境中的最佳实践：
/// - 使用 Bearer Token 认证
/// - 精确指定工具白名单（最小权限原则）
/// - 同时连接多个 Streamable HTTP 服务
async fn scenario_production_config() -> AgentResult<()> {
    println!("\n=== 场景 3：生产环境配置（认证 + 白名单 + 多服务）===");

    let service_url =
        env::var(ENV_MCP_HTTP_URL).unwrap_or_else(|_| DEFAULT_MCP_HTTP_URL.to_string());
    let bearer_token =
        env::var("MCP_BEARER_TOKEN").unwrap_or_else(|_| "demo-production-token".to_string());

    // 主服务：带认证和精确白名单
    let primary_config = create_http_mcp_config_production(&service_url, &bearer_token);

    // 辅助服务：另一个 Streamable HTTP 端点（如内部工具服务）
    let secondary_url =
        env::var("MCP_SECONDARY_URL").unwrap_or_else(|_| "http://localhost:8082/mcp".to_string());
    let secondary_config = McpServerConfig {
        service_name: "internal-tools".to_string(),
        allowed_tools: vec!["log_query".to_string(), "metrics_fetch".to_string()],
        server_config: McpServiceEntry {
            transport_type: Some("streamable_http".to_string()),
            url: Some(secondary_url),
            timeout: Some(30_000),
            headers: std::collections::HashMap::from([(
                "X-Internal-Token".to_string(),
                "internal-secret".to_string(),
            )]),
            ..Default::default()
        },
    };

    let agent = AgentBuilder::new(create_agent_config()?)?
        // 批量注入多个 Streamable HTTP 服务
        .add_mcps(vec![primary_config, secondary_config])
        .build()
        .await?;

    let result = agent
        .run(
            "http-session-3",
            "你现在连接了哪些 MCP 服务？每个服务有哪些可用工具？",
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

    let http_url = env::var(ENV_MCP_HTTP_URL).unwrap_or_else(|_| DEFAULT_MCP_HTTP_URL.to_string());

    println!("=== MCP Streamable HTTP 传输示例 ===");
    println!("传输类型: Streamable HTTP（双向流式）");
    println!("服务地址: {http_url}");
    println!("提示: 可通过 MCP_HTTP_URL 环境变量覆盖服务地址");

    // 场景 1：AgentBuilder 便捷 API
    if let Err(e) = scenario_builder_api().await {
        println!("场景 1 失败: {e}");
    }

    // 场景 2：手动构建 McpRegistry + 服务详情查看
    if let Err(e) = scenario_manual_registry().await {
        println!("场景 2 失败: {e}");
    }

    // 场景 3：生产环境配置
    if let Err(e) = scenario_production_config().await {
        println!("场景 3 失败: {e}");
    }

    println!("\n=== 示例完成 ===");
    Ok(())
}
