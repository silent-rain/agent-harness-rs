//! MCP stdio 传输示例
//!
//! 演示如何通过 **stdio** 传输类型连接 MCP 服务。
//!
//! stdio 传输会在本地启动一个子进程（如 `npx`、`uvx`、`node` 等），
//! 通过标准输入/输出与 MCP 服务通信，适用于：
//! - 本地 Node.js / Python MCP 服务包
//! - 无需网络端口的本地工具集成
//! - 开发调试阶段的快速验证
//!
//! # 前提
//!
//! 本示例使用 `npx` 启动 `@modelcontextprotocol/server-filesystem` 作为演示服务，
//! 运行前请确保已安装 Node.js 和 npm：
//!
//! ```sh
//! node --version   # >= 18
//! npx --version
//! ```
//!
//! # 运行
//!
//! ```sh
//! OPENAI_API_KEY=sk-xxx cargo run --example mcp_stdio --features mcp -p agent-runtime
//! ```

use std::env;

use agent_runtime::{
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

// === 默认值 ===

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_PROVIDER: &str = "openai";

/// 从环境变量加载模型配置。
fn load_model_config() -> AgentResult<ModelConfig> {
    let api_key = env::var(ENV_API_KEY).map_err(|_| {
        agent_runtime::Error::ModelCall(format!(
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

/// 创建智能体配置（文件系统助手）。
fn create_agent_config() -> AgentResult<AgentConfig> {
    Ok(AgentConfig {
        agent_definition: AgentDefinition {
            id: 1,
            user_id: "demo".to_string(),
            name: "文件系统助手".to_string(),
            description: "通过 MCP stdio 传输连接本地文件系统服务的智能体".to_string(),
            welcome_message: Some(
                "你好！我可以通过 MCP 文件系统服务帮你读取和管理文件。".to_string(),
            ),
            system_prompt: concat!(
                "你是一个文件系统助手，通过 MCP 协议连接到本地文件系统服务。\n",
                "你可以使用提供的工具来读取文件、列出目录内容等操作。\n",
                "请始终以中文回复用户。"
            )
            .to_string(),
            max_delegation_depth: 1,
        },
        model_config: load_model_config()?,
    })
}

/// 构建 stdio 传输的 MCP 服务配置。
///
/// 使用 `npx` 启动 `@modelcontextprotocol/server-filesystem`，
/// 挂载当前工作目录（`.`）作为可访问的文件系统根路径。
fn create_stdio_mcp_config() -> McpServerConfig {
    McpServerConfig {
        service_name: "filesystem".to_string(),
        // 允许所有工具（"*" 通配符）
        allowed_tools: vec!["*".to_string()],
        server_config: McpServiceEntry {
            // stdio 传输：通过子进程启动 MCP 服务
            transport_type: Some("stdio".to_string()),
            // 可执行命令：npx（Node.js 包执行器）
            command: Some("npx".to_string()),
            // 参数：-y 自动确认安装，包名，挂载路径
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                ".".to_string(),
            ],
            ..Default::default()
        },
    }
}

/// 场景 1：通过 AgentBuilder 便捷 API 使用 stdio MCP 服务。
///
/// 最简洁的用法：调用 `add_mcps` 注入配置，
/// `build()` 阶段自动完成连接与工具注入。
async fn scenario_builder_api() -> AgentResult<()> {
    println!("\n=== 场景 1：AgentBuilder 便捷 API ===");

    let agent = AgentBuilder::new(create_agent_config()?)?
        .add_mcps(vec![create_stdio_mcp_config()])
        .build()
        .await?;

    let result = agent
        .run("stdio-session-1", "列出当前目录下有哪些文件？")
        .await?;

    println!("回答: {}", result.final_answer);
    Ok(())
}

/// 场景 2：手动构建 McpRegistry 后注入 AgentBuilder。
///
/// 适用于需要在注入前检查连接状态、获取工具列表等高级场景。
async fn scenario_manual_registry() -> AgentResult<()> {
    println!("\n=== 场景 2：手动构建 McpRegistry ===");

    // 手动构建并连接 MCP 注册表
    let mut registry = McpRegistry::new();
    registry.add_config(create_stdio_mcp_config());

    // 连接失败时忽略（示例环境可能未安装 npx）
    if let Err(e) = registry.connect_all().await {
        println!("MCP 连接失败（跳过）: {e}");
        return Ok(());
    }

    println!("已连接服务: {:?}", registry.service_names());

    // 查看可用工具列表（已按配置的启用范围过滤）
    for service_name in registry.service_names() {
        let tools = registry.tools_of(service_name).await?;
        println!("[{service_name}] 可用工具 {} 个:", tools.len());
        for tool in &tools {
            println!(
                "  - {} : {}",
                tool.name,
                tool.description.as_deref().unwrap_or("")
            );
        }
    }

    // 注入已连接的注册表
    let agent = AgentBuilder::new(create_agent_config()?)?
        .with_mcp_registry(registry)
        .build()
        .await?;

    let result = agent
        .run("stdio-session-2", "当前目录下有 Cargo.toml 文件吗？")
        .await?;

    println!("回答: {}", result.final_answer);
    Ok(())
}

/// 场景 3：批量注入多个 stdio MCP 服务配置。
///
/// 演示同时挂载多个本地 MCP 服务（如文件系统 + 自定义脚本工具）。
async fn scenario_multiple_stdio() -> AgentResult<()> {
    println!("\n=== 场景 3：批量注入多个 stdio 服务 ===");

    // 第二个服务：使用 uvx 启动 Python MCP 服务（示例）
    let python_mcp = McpServerConfig {
        service_name: "python-tools".to_string(),
        allowed_tools: vec!["*".to_string()],
        server_config: McpServiceEntry {
            transport_type: Some("stdio".to_string()),
            command: Some("uvx".to_string()),
            args: vec!["mcp-server-fetch".to_string()],
            // 可通过 env 注入环境变量
            env: std::collections::HashMap::from([("PYTHONPATH".to_string(), ".".to_string())]),
            ..Default::default()
        },
    };

    let agent = AgentBuilder::new(create_agent_config()?)?
        // 批量注入多个 stdio 服务
        .add_mcps(vec![create_stdio_mcp_config(), python_mcp])
        .build()
        .await?;

    let result = agent
        .run("stdio-session-3", "你现在有哪些可用的工具？")
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

    println!("=== MCP stdio 传输示例 ===");
    println!("传输类型: stdio（子进程）");
    println!("服务: @modelcontextprotocol/server-filesystem (via npx)");

    // 场景 1：AgentBuilder 便捷 API
    if let Err(e) = scenario_builder_api().await {
        println!("场景 1 失败（可能未安装 npx）: {e}");
    }

    // 场景 2：手动构建 McpRegistry
    if let Err(e) = scenario_manual_registry().await {
        println!("场景 2 失败: {e}");
    }

    // 场景 3：批量注入多个 stdio 服务
    if let Err(e) = scenario_multiple_stdio().await {
        println!("场景 3 失败（可能未安装 uvx）: {e}");
    }

    println!("\n=== 示例完成 ===");
    Ok(())
}
