//! 基础智能体流式示例（仅 Rule）
//!
//! 与 [`basic_agent`] 示例配置完全一致（Qwen-Image 2512 叙事提示词助手），
//! 区别在于通过 [`Agent::run_stream`] 实时消费 [`AgentStreamEvent`]，
//! 在控制台逐 token / 逐事件展示推理过程，最终拼接出完整回答。
//!
//! 模型配置从 workspace 根目录的 `.env` 文件读取，可参考 `.env.example`。
//!
//! # 环境变量
//!
//! - `OPENAI_API_KEY`（必填）
//! - `OPENAI_BASE_URL`（可选，默认 `https://api.openai.com/v1`）
//! - `OPENAI_MODEL`（可选，默认 `gpt-4o`）
//! - `OPENAI_PROVIDER`（可选，默认 `openai`）
//! - `OPENAI_TEMPERATURE`（可选，默认 `0.7`）
//!
//! 运行：
//! ```bash
//! cargo run -p agent-base --example basic_agent_streaming
//! ```

use std::{
    env,
    io::{self, Write},
};

use dotenv::dotenv;
use futures::StreamExt;

use agent_base::{
    error::AgentResult,
    runtime::{AgentBuilder, AgentOpenAI, OpenAIChunkStream},
    types::{
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

/// 示例使用的会话标识。
const SESSION_ID: &str = "basic-streaming-session-1";
/// 示例使用的用户输入。
const USER_INPUT: &str = "一个女孩在河边散步";

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

/// 创建基础智能体配置（Qwen-Image 2512 叙事提示词助手）。
///
/// 与 `basic_agent` 示例保持一致，便于对比同一智能体在阻塞与流式两种运行模式下的行为差异。
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

/// 消费 [`OpenAIChunkStream`]（`AgentOpenAI::run_stream` 返回的 OpenAI 兼容分片流）
/// 并实时打印，返回拼接后的最终回答。
///
/// # 分片语义
///
/// 每个 [`ChatCompletionChunk`](agent_base::openai::ChatCompletionChunk) 对应一次
/// OpenAI Chat Completions stream 分片，字段含义：
///
/// - `choices[0].delta.content`：本分片携带的文本增量（`TextDelta` 对应的 token）
/// - `choices[0].finish_reason`：非 `None` 时表示本次运行结束，值为 `stop` / `length` 等
///
/// # Parameters
///
/// * `stream` - 由 [`AgentOpenAI::run_stream`] 返回的 OpenAI 兼容分片流。
///
/// # Returns
///
/// 通过累积各分片 `delta.content` 拼接得到的最终回答文本。
async fn consume_stream(mut stream: OpenAIChunkStream) -> String {
    let mut final_answer = String::new();
    let mut stdout = io::stdout();

    while let Some(chunk) = stream.next().await {
        let Some(choice) = chunk.choices.into_iter().next() else {
            continue;
        };

        // 文本增量：逐 token 打印并累积
        if let Some(content) = choice.delta.content {
            final_answer.push_str(&content);
            print!("{content}");
            let _ = stdout.flush();
        }

        // finish_reason 非空表示本次运行结束
        if let Some(reason) = choice.finish_reason {
            println!("\n✅ 运行完成: finish_reason={reason:?}");
            break;
        }
    }

    final_answer
}

/// 运行基础智能体流式示例（仅 Rule）。
async fn run_basic_agent_streaming() -> AgentResult<String> {
    let agent_config = create_basic_agent_config()?;

    let agent = AgentBuilder::new(agent_config)?
        .add_rules(create_basic_rules())
        .build()
        .await?;

    // 使用 AgentOpenAI::run_stream，返回 OpenAI 兼容的 ChatCompletionChunk 分片流
    let stream = AgentOpenAI::run_stream(&agent, SESSION_ID, USER_INPUT)?;
    Ok(consume_stream(stream).await)
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    // 加载 workspace 根目录下的 `.env`（若不存在则忽略，使用进程已有环境变量）。
    dotenv().ok();

    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_max_level(tracing::Level::INFO)
        .with_level(true)
        .with_file(true)
        .with_line_number(true)
        .with_target(false)
        .init();

    println!("=== 基础智能体流式示例 ===");
    let final_answer = run_basic_agent_streaming().await?;
    println!("\n--- 最终回答 ---\n{final_answer}");
    println!("\n=== 示例完成 ===");
    Ok(())
}
