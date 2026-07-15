//! 完整装配的智能体示例（Rule + Skill + Tool + MCP + Command）
//!
//! 展示如何同时装配所有子系统：
//! - **Rule**: 通过 `LocalRuleLoader` + `add_rules` 注入规则
//! - **Skill**: 通过 `LocalSkillLoader` + `add_skills` 注入技能
//! - **Tool**: 通过 `AgentBuilder::add_tool()` 注册内置工具
//! - **MCP**: 通过 `McpRegistry` 连接外部 MCP 服务
//! - **Command**: 通过 `LocalCommandLoader` + `add_commands` 加载命令模板
//!
//! 运行：
//! ```bash
//! cargo run -p agent-base --example full_agent
//! ```

use std::env;

use agent_base::{
    command::LocalCommandLoader,
    error::AgentResult,
    mcp::McpRegistry,
    rule::LocalRuleLoader,
    runtime::{Agent, AgentBuilder},
    skill::LocalSkillLoader,
    tool::{ListDirTool, ReadFileTool, RunCommandTool, SearchFilesTool},
    types::{
        mcp::{McpServerConfig, McpServiceEntry},
        rule::{RuleConfig, RuleSource},
        runtime::{AgentConfig, AgentDefinition, ModelConfig},
    },
};

// === 环境变量名常量（消除魔数式字符串） ===

/// API Key 环境变量名（必填）。
const ENV_API_KEY: &str = "OPENAI_API_KEY";
/// API 基础地址环境变量名。
const ENV_BASE_URL: &str = "OPENAI_BASE_URL";
/// 模型编码环境变量名。
const ENV_MODEL: &str = "OPENAI_MODEL";
/// 供应商标识环境变量名。
const ENV_PROVIDER: &str = "OPENAI_PROVIDER";
/// 采样温度环境变量名。
const ENV_TEMPERATURE: &str = "OPENAI_TEMPERATURE";

// === 默认值（OpenAI 官方端点） ===

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_PROVIDER: &str = "openai";
const DEFAULT_TEMPERATURE: f64 = 0.7;

/// 从环境变量加载 [`ModelBindingConfig`]。
///
/// 调用前应已通过 [`dotenv::dotenv`] 将 `.env` 文件加载到进程环境。
///
/// # Returns
///
/// 装配好的 [`ModelBindingConfig`]；当 `OPENAI_API_KEY` 缺失时返回错误，
/// 引导用户检查 `.env` 配置。
fn load_model_binding_from_env() -> AgentResult<ModelConfig> {
    let api_key = env::var(ENV_API_KEY).map_err(|_| {
        agent_base::Error::ModelCall(format!(
            "missing env `{ENV_API_KEY}`: please configure it in `.env` (see `.env.example`)"
        ))
    })?;

    let temperature = env::var(ENV_TEMPERATURE)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(DEFAULT_TEMPERATURE);

    Ok(ModelConfig {
        provider: env::var(ENV_PROVIDER).unwrap_or_else(|_| DEFAULT_PROVIDER.to_string()),
        model: env::var(ENV_MODEL).unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
        base_url: env::var(ENV_BASE_URL).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
        api_key,
        temperature: Some(temperature),
        max_tokens: None,
    })
}

/// 创建基础智能体配置（绘画提示词助手）。
///
/// `model_config` 字段从环境变量读取，便于在不修改示例代码的前提下切换真实模型。
fn create_basic_agent_config() -> AgentResult<AgentConfig> {
    Ok(AgentConfig {
        agent_definition: AgentDefinition {
            id: 1,
            user_id: "1".to_string(),
            name: "Qwen-Image 叙事提示词助手".to_string(),
            description: "面向 Qwen-Image 2512 模型的叙事型提示词助手，将用户创意扩写为富有画面感的中文叙事提示词".to_string(),
            welcome_message: Some(
                "你好！我是 Qwen-Image 2512 叙事提示词助手，告诉我你想画的画面，我会为你生成富有故事感的中文叙事提示词。"
                    .to_string(),
            ),
            system_prompt: concat!(
                "你是一名专业的 Qwen-Image 2512 提示词工程师，精通该模型对中文长文本与叙事化描述的理解能力。\n",
                "你的任务是：根据用户提供的创意描述，生成一份高质量、富有画面感的中文叙事提示词，全程仅使用中文输出。\n\n",
                "请严格遵循以下规则：\n",
                "1. 输出语言：全程使用中文，禁止夹杂英文关键词或英文短语，让 Qwen-Image 2512 直接理解中文叙事。\n",
                "2. 叙事风格：采用流畅自然的叙事性长句描述，而非英文模型常见的逗号分隔关键词堆砌；像在讲述一幅画的故事。\n",
                "3. 描写顺序：主体人物或对象 → 外貌神态与动作 → 场景环境与时间 → 光线与氛围 → 构图与视角 → 画面风格与质感。\n",
                "4. 细节刻画：注重材质、光影、情绪、空间层次的具象化描写，让画面具有故事张力，但避免冗长堆砌形容词。\n",
                "5. 画质表达：用中文自然融入「细节丰富」「电影质感」「高清画质」等画质描述，避免生硬罗列。\n",
                "6. 输出格式：先用一段不超过 50 字的中文说明创作思路，再输出一段完整的中文叙事提示词正文。"
            )
            .to_string(),
            max_delegation_depth: 3,
        },
        model_config: load_model_binding_from_env()?,
    })
}

/// 创建示例规则配置。
fn create_basic_rules() -> Vec<RuleConfig> {
    vec![RuleConfig {
        name: "基础规则".to_string(),
        description: String::new(),
        content: "始终礼貌回应用户".to_string(),
        source: RuleSource::Injected,
        source_hint: None,
    }]
}

/// 创建示例 MCP 服务配置。
fn create_basic_mcp_configs() -> Vec<McpServerConfig> {
    vec![McpServerConfig {
        service_name: "example-mcp-service".to_string(),
        allowed_tools: vec!["search".to_string(), "fetch".to_string()],
        server_config: McpServiceEntry {
            url: Some("http://localhost:8080/mcp".to_string()),
            transport_type: Some("streamable_http".to_string()),
            ..Default::default()
        },
    }]
}

/// 运行完整装配的智能体示例。
async fn run_full_agent() -> AgentResult<String> {
    let agent_config = create_basic_agent_config()?;

    // === 1. Rule: load via LocalRuleLoader and combine with extra rules ===
    let mut rules = LocalRuleLoader::default().load().await?;
    rules.extend(create_basic_rules());

    // === 2. Skill: load via LocalSkillLoader ===
    let skills = LocalSkillLoader::new(".codebuddy/skills").load().await?;

    // === 3. Tool: registered directly on the builder below ===

    // === 4. MCP: build mcp registry and connect services ===
    let mcp_configs = create_basic_mcp_configs();
    let mut mcp_registry = McpRegistry::new().with_configs(mcp_configs);
    // NOTE: example MCP service is not actually running, ignore failures.
    mcp_registry.connect_all().await.ok();

    // === 5. Command: load via LocalCommandLoader ===
    let commands = LocalCommandLoader::new(".codebuddy/commands")
        .load()
        .await?;

    // === Assemble all subsystems into Agent ===
    let agent = AgentBuilder::new(agent_config)?
        .add_rules(rules)
        .add_skills(skills)
        .add_tool(ReadFileTool)
        .add_tool(ListDirTool)
        .add_tool(SearchFilesTool)
        .add_tool(RunCommandTool)
        .with_mcp_registry(mcp_registry)
        .add_commands(commands)
        .build()
        .await?;

    Ok(agent
        .run("test-session-full", "你好，世界！")
        .await?
        .final_answer)
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_max_level(tracing::Level::DEBUG)
        .with_level(true)
        .with_file(true)
        .with_line_number(true)
        .with_target(false)
        .init();

    println!("=== 完整装配智能体示例 ===");
    let result = run_full_agent().await?;
    println!("完整模式运行结果: {}", result);
    println!("=== 示例完成 ===");
    Ok(())
}
