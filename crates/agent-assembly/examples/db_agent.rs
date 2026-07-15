//! 基于数据库的智能体示例（agent-assembly）
//!
//! 演示通过 [`DbAgentBuilder`] 一键装配数据库驱动的智能体：
//! 只需提供 `(database_url, agent_id, user_id)`，即可自动完成
//!
//! - 智能体定义、模型绑定、规则、命令、技能、MCP、子智能体的加载
//! - [`DbSessionService`] 的装配与上下文持久化
//! - 事件流入库（`t_agent_session_event`）
//!
//! 模型 API Key 与 base_url 直接从 `t_agent_llm_model` 取，不再依赖 `OPENAI_*` 环境变量。
//!
//! # 环境变量
//!
//! - `DATABASE_URL`（必填）—— SeaORM 兼容连接串，例如：
//!   - `mysql://user:pass@127.0.0.1:3306/agent_db`
//!   - `postgres://user:pass@127.0.0.1:5432/agent_db?currentSchema=public`
//!   - `sqlite://path/to/db.sqlite?mode=rwc`
//! - `AGENT_ID`（必填，i64）—— `t_agent_config.id`
//! - `USER_ID`（必填，i64）—— `t_agent_config.user_id`，用于权限隔离
//! - `USER_INPUT`（可选，默认见 [`DEFAULT_USER_INPUT`]）—— 用户输入文本（可被 `--prompt` 覆盖）
//! - `SESSION_ID`（可选，默认 `db-agent-session-1`）—— 会话标识（可被 `--session-id` 覆盖）
//!
//! # 命令行参数（优先级高于同名环境变量）
//!
//! - `--prompt <TEXT>`    —— 用户输入的提示词
//! - `--session-id <ID>` —— 会话标识
//!
//! 运行：
//! ```bash
//! # 使用环境变量
//! cargo run -p agent-assembly --example db_agent
//!
//! # 使用命令行参数
//! cargo run -p agent-assembly --example db_agent -- --prompt "你好" --session-id "my-session"
//! ```
//!
//! [`DbSessionService`]: agent_assembly::DbSessionService

use std::{env, sync::Arc};

use clap::Parser;
use dotenv::dotenv;
use sea_orm::DatabaseConnection;

use agent_assembly::{AgentSummary, DbAgentSource, DbSessionService, Error, PlatformResult};
use agent_runtime::{
    RuntimeServices,
    model::create_model,
    runtime::{Agent, AgentBuilder},
};
use database::ConnectionBuilder;

// === 命令行参数 ===

/// 数据库驱动的智能体示例。
///
/// 优先级：命令行参数 > 环境变量 > 默认值。
#[derive(Parser, Debug)]
#[command(name = "db_agent", about = "数据库驱动的智能体示例")]
struct Cli {
    /// 用户输入的提示词（覆盖 USER_INPUT 环境变量）
    #[arg(long, value_name = "TEXT")]
    prompt: Option<String>,

    /// 会话标识（覆盖 SESSION_ID 环境变量）
    #[arg(long, value_name = "ID")]
    session_id: Option<String>,
}

// === 环境变量名常量（消除魔数式字符串） ===

/// 数据库连接串环境变量名（必填）。
const ENV_DATABASE_URL: &str = "DATABASE_URL";
/// 智能体 ID 环境变量名（必填，i64）。
const ENV_AGENT_ID: &str = "AGENT_ID";
/// 用户 ID 环境变量名（必填，i64）。
const ENV_USER_ID: &str = "USER_ID";
/// 用户输入环境变量名（可选）。
const ENV_USER_INPUT: &str = "USER_INPUT";
/// 会话 ID 环境变量名（可选）。
const ENV_SESSION_ID: &str = "SESSION_ID";

// === 默认值 ===

/// 默认用户输入。
const DEFAULT_USER_INPUT: &str = "你好，请简要介绍一下你自己。然后描述一个女孩在河边的摄影的提示词，最后使用 translator-sub-agent 子智能体进行翻译";
/// 默认会话标识。
const DEFAULT_SESSION_ID: &str = "db-agent-session-1";

/// 从环境变量读取必填字符串。
fn require_env(key: &str) -> PlatformResult<String> {
    env::var(key).map_err(|_| {
        Error::Internal(format!(
            "missing env `{key}`: please configure it in `.env` (see `.env.example`)"
        ))
    })
}

/// 从环境变量读取并解析必填整数。
fn require_env_parsed<T: std::str::FromStr>(key: &str) -> PlatformResult<T>
where
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let raw = require_env(key)?;
    raw.parse::<T>()
        .map_err(|e| Error::Internal(format!("env `{key}` is not a valid number: {e}")))
}

/// 建立 SeaORM 数据库连接。
async fn connect_database(database_url: String) -> PlatformResult<DatabaseConnection> {
    ConnectionBuilder::new(database_url)
        .build()
        .await
        .map_err(|e| Error::Internal(format!("connect database failed: {e}")))
}

/// 装配带 [`DbSessionService`] 的 [`RuntimeServices`]
///
/// 会话服务完全无状态，可在不同会话间复用；
pub fn build_db_runtime_services(db: DatabaseConnection) -> RuntimeServices {
    let session_service = Arc::new(DbSessionService::new(Arc::new(db)));
    RuntimeServices {
        session: session_service,
        ..RuntimeServices::noop()
    }
}

/// 运行数据库驱动的智能体示例。
///
/// 通过 [`DbAgentBuilder`] 一次性完成：
///
/// 1. 加载智能体定义、模型绑定、规则、命令、技能、MCP、子智能体
/// 2. 装配会话事件与上下文入库（`DbSessionService`）
/// 3. 通过 `SessionPersistingPublisher` 把事件流写入 `t_agent_session_event`
async fn run_db_agent(cli: Cli) -> PlatformResult<String> {
    // 1. 解析参数（命令行参数 > 环境变量 > 默认值）
    let database_url = require_env(ENV_DATABASE_URL)?;
    let agent_id: i64 = require_env_parsed(ENV_AGENT_ID)?;
    let user_id: i64 = require_env_parsed(ENV_USER_ID)?;
    let user_input = cli
        .prompt
        .or_else(|| env::var(ENV_USER_INPUT).ok())
        .unwrap_or_else(|| DEFAULT_USER_INPUT.to_string());
    let session_id = cli
        .session_id
        .or_else(|| env::var(ENV_SESSION_ID).ok())
        .unwrap_or_else(|| DEFAULT_SESSION_ID.to_string());

    // 2. 建立数据库连接
    let db = connect_database(database_url).await?;

    // 3. 装配 LlmAgent + 能力摘要（自动加载 rules / commands / skills / mcp / sub_agents）
    //    会话主表由 `LlmAgent::run_stream` 入口在运行时自动幂等 ensure，
    //    装配阶段不再绑定具体 `session_id`。
    let bundle = DbAgentSource::new(db.clone(), agent_id, user_id)
        .load_bundle()
        .await
        .map_err(|e| Error::Internal(format!("load agent bundle failed: {e}")))?;
    let summary = AgentSummary::from_bundle(&bundle);
    let model = create_model(&bundle.model_config)?;
    let services = build_db_runtime_services(db.clone());

    let source = DbAgentSource::new(db, agent_id, user_id);
    let agent = AgentBuilder::new_with_model(bundle.to_agent_config(), model)
        .add_source(source)
        .with_services(services)
        .build()
        .await?;

    println!(
        "✓ 已加载智能体: id={}, name={}, model={}@{}",
        summary.agent_id, summary.name, summary.model, summary.provider,
    );
    println!(
        "✓ 能力摘要: rules={}, commands={}, skills={}, mcp={}, sub_agents={}",
        summary.rules, summary.commands, summary.skills, summary.mcp, summary.sub_agents,
    );

    // 4. 执行一次推理（事件自动入库 t_agent_session_event）
    let outcome = agent.run(&session_id, &user_input).await?;
    Ok(outcome.final_answer)
}

#[tokio::main]
async fn main() -> PlatformResult<()> {
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

    let cli = Cli::parse();

    println!("=== 数据库驱动的智能体示例 (agent-assembly) ===");
    let answer = run_db_agent(cli).await?;
    println!("\n--- 最终回答 ---\n{answer}");
    println!("\n=== 示例完成 ===");
    Ok(())
}
