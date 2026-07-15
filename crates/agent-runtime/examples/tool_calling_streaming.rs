//! 工具调用流式示例（聚焦 Tool 子系统）
//!
//! 与 [`full_agent_streaming`] 共享完整子系统装配（Rule / Skill / Tool / MCP / Command），
//! 但通过"代码助手"系统提示词与"明确指令型"用户输入，引导 LLM 主动调用内置工具
//! ([`ReadFileTool`] / [`ListDirTool`] / [`SearchFilesTool`] / [`RunCommandTool`])，
//! 让你在控制台清晰看到 [`AgentStreamEvent::ToolCallStarted`] /
//! [`AgentStreamEvent::ToolCallFinished`] 的完整生命周期。
//!
//! 模型配置从 workspace 根目录的 `.env` 文件读取，可参考 `.env.example`。
//!
//! # 环境变量
//!
//! - `OPENAI_API_KEY`（必填）
//! - `OPENAI_BASE_URL`（可选，默认 `https://api.openai.com/v1`）
//! - `OPENAI_MODEL`（可选，默认 `gpt-4o`，建议使用支持 tool calling 的模型）
//! - `OPENAI_PROVIDER`（可选，默认 `openai`）
//! - `OPENAI_TEMPERATURE`（可选，默认 `0.2`，工具调用场景建议低温度）
//!
//! 运行：
//! ```bash
//! cargo run -p agent-runtime --example tool_calling_streaming
//! ```

use std::{
    env,
    io::{self, Write},
};

use agent_runtime::{
    error::AgentResult,
    runtime::{Agent, AgentBuilder, AgentEventStream},
    session::AgentStreamEvent,
    tool::{ListDirTool, ReadFileTool, RunCommandTool, SearchFilesTool},
    types::runtime::{AgentConfig, AgentDefinition, ModelConfig},
};
use dotenv::dotenv;
use futures::StreamExt;

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

// === 默认值 ===

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_PROVIDER: &str = "openai";
/// 工具调用场景建议使用较低温度，提升参数构造稳定性。
const DEFAULT_TEMPERATURE: f64 = 0.2;

// === 运行参数常量 ===

/// 示例使用的会话标识。
const SESSION_ID: &str = "tool-calling-streaming-session-1";
/// 示例使用的用户输入：明确要求模型调用工具完成多步任务。
const USER_INPUT: &str = concat!(
    "请帮我完成以下任务，必须通过调用提供的工具完成，不要凭空猜测：\n",
    "1. 使用 list_dir 列出 `crates/core/agent-runtime/examples` 目录下的全部文件；\n",
    "2. 从中找到 `full_agent.rs`，使用 read_file 读取它的前 30 行；\n",
    "3. 用 search_files 在 `crates/core/agent-runtime/src/tool` 目录下查找包含关键字 `ReadFileTool` 的文件；\n",
    "4. 完成后用一段简短中文总结你看到的内容（不要复述全部代码）。"
);

/// 代码助手系统提示词：明确告知可用工具与调用契约。
const SYSTEM_PROMPT: &str = concat!(
    "你是一名严谨的代码助手，工作在 Rust 项目仓库中。\n",
    "你**必须**通过调用以下工具来完成用户任务，禁止凭空编造文件内容或目录结构：\n",
    "- `read_file`：读取文件内容，参数 `path`（必填）、`start_line`、`line_count`（可选）。\n",
    "- `list_dir`：列出目录下的条目，参数 `path`（必填）。\n",
    "- `search_files`：在指定目录递归按名称模糊查找文件，参数 `path` 与 `pattern`（必填）。\n",
    "- `run_command`：执行 shell 命令，参数 `command`（必填）。仅在用户明确要求执行命令时使用。\n\n",
    "工作原则：\n",
    "1. 一次只调用一个工具，根据返回结果决定下一步。\n",
    "2. 路径优先使用相对工作区根的相对路径（例如 `crates/core/agent-runtime/examples`）。\n",
    "3. 全部工具调用完成后，用一段中文小结向用户复述你做了什么、看到了什么。"
);

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
        agent_runtime::Error::ModelCall(format!(
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

/// 创建"代码助手"智能体配置。
fn create_tool_agent_config() -> AgentResult<AgentConfig> {
    Ok(AgentConfig {
        agent_definition: AgentDefinition {
            id: 2,
            user_id: "1".to_string(),
            name: "Rust 仓库代码助手".to_string(),
            description: "通过调用文件 / 目录 / 搜索 / 命令工具来回答仓库相关问题的代码助手"
                .to_string(),
            welcome_message: Some(
                "你好！告诉我你想了解仓库中的什么信息，我会通过调用工具去实际查看。".to_string(),
            ),
            system_prompt: SYSTEM_PROMPT.to_string(),
            max_delegation_depth: 3,
        },
        model_config: load_model_binding_from_env()?,
    })
}

/// 截断字符串到指定字符数（按 char 计数，避免 UTF-8 边界问题），用于事件日志预览。
///
/// # Parameters
///
/// * `s` - 原始字符串
/// * `max_chars` - 最大保留字符数
///
/// # Returns
///
/// 若原串字符数超过 `max_chars`，则截断并追加 `...`；否则返回原串副本。
fn truncate_for_log(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

/// 用于参数 / 结果预览的最大字符数。
const PREVIEW_MAX_CHARS: usize = 200;

/// 消费 [`AgentEventStream`] 并实时打印事件，重点突出工具调用全生命周期。
///
/// # 事件语义
///
/// - [`AgentStreamEvent::ToolCallStarted`]：携带 `toolname` / `call_id` / `arguments_json`，
///   是观察"模型决定调用哪个工具、传了什么参数"的关键事件。
/// - [`AgentStreamEvent::ToolCallFinished`]：携带 `success` / `result` / `error_message` /
///   `duration_ms`，描述工具实际执行结果。
/// - [`AgentStreamEvent::TextDelta`]：LLM 原始 token 流；最终的中文小结由它逐 token 推送。
/// - [`AgentStreamEvent::Answer`]：整段终答镜像，每次运行至多一条；本示例不再累积，
///   仅作为结束分隔信号，避免与 `TextDelta` 累积出现一倍冗余。
///
/// # Parameters
///
/// * `stream` - 由 [`Agent::run_stream`] 返回的事件流。
///
/// # Returns
///
/// 通过累积 [`AgentStreamEvent::TextDelta`] 拼接得到的最终回答文本。
async fn consume_stream(mut stream: AgentEventStream) -> String {
    let mut final_answer = String::new();
    let mut stdout = io::stdout();

    while let Some(event) = stream.next().await {
        match event {
            AgentStreamEvent::RunStarted {
                run_id, agent_name, ..
            } => {
                println!("🚀 运行开始: run_id={run_id}, agent={agent_name}");
            }
            AgentStreamEvent::StepStarted { step, .. } => {
                println!("\n📝 步骤 {step} 开始");
            }
            AgentStreamEvent::ReasoningDelta { delta, .. } => {
                print!("🧠 {delta}");
                let _ = stdout.flush();
            }
            AgentStreamEvent::TextDelta { delta, .. } => {
                final_answer.push_str(&delta);
                print!("{delta}");
                let _ = stdout.flush();
            }
            AgentStreamEvent::Answer { .. } => {
                println!("\n— 终段镜像已就位 —");
            }
            AgentStreamEvent::ToolCallStarted {
                toolname,
                call_id,
                arguments_json,
                ..
            } => {
                println!(
                    "🔧 工具调用开始: tool={toolname}, call_id={call_id}, args={}",
                    truncate_for_log(&arguments_json, PREVIEW_MAX_CHARS)
                );
            }
            AgentStreamEvent::ToolCallFinished {
                toolname,
                call_id,
                success,
                result,
                error_message,
                duration_ms,
                ..
            } => {
                if success {
                    let preview = result
                        .as_deref()
                        .map(|r| truncate_for_log(r, PREVIEW_MAX_CHARS))
                        .unwrap_or_else(|| "<empty>".to_string());
                    println!(
                        "✅ 工具调用成功: tool={toolname}, call_id={call_id}, duration_ms={duration_ms}, result={preview}"
                    );
                } else {
                    let err = error_message.unwrap_or_else(|| "<unknown>".to_string());
                    println!(
                        "❌ 工具调用失败: tool={toolname}, call_id={call_id}, duration_ms={duration_ms}, error={err}"
                    );
                }
            }
            AgentStreamEvent::RunFinished {
                total_steps,
                finish_reason,
                ..
            } => {
                println!(
                    "\n🏁 运行完成: total_steps={total_steps}, finish_reason={finish_reason:?}"
                );
                break;
            }
            _ => {}
        }
    }

    final_answer
}

/// 运行工具调用流式示例。
///
/// 仅注册 4 个内置工具，不挂载 Skill / MCP / Command，让事件流聚焦在工具调用本身。
async fn run_tool_calling_streaming() -> AgentResult<String> {
    let agent_config = create_tool_agent_config()?;

    let agent = AgentBuilder::new(agent_config)?
        .add_tool(ReadFileTool)
        .add_tool(ListDirTool)
        .add_tool(SearchFilesTool)
        .add_tool(RunCommandTool)
        .build()
        .await?;

    let stream = agent.run_stream(SESSION_ID, USER_INPUT)?;
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

    println!("=== 工具调用流式示例（聚焦 Tool 子系统） ===");
    println!("用户输入：\n{USER_INPUT}\n");
    let final_answer = run_tool_calling_streaming().await?;
    println!("\n--- 最终回答 ---\n{final_answer}");
    println!("\n=== 示例完成 ===");
    Ok(())
}
