//! A2A Client 示例
//!
//! 演示如何使用 `A2AAgent` 像调用本地 Agent 一样调用远程 A2A 服务。
//!
//! # 前提
//!
//! 先启动 A2A Server（另开终端）：
//! ```sh
//! cargo run --example a2a_server --features a2a -p agent-runtime
//! ```
//!
//! # 运行
//!
//! ```sh
//! cargo run --example a2a_client --features a2a -p agent-runtime
//! ```

use dotenv::dotenv;
use futures::StreamExt;
use tracing::info;
use tracing_subscriber::EnvFilter;

use agent_runtime::{
    a2a::A2AAgent, error::AgentResult, runtime::Agent, types::agent_meta::AgentMeta,
};

#[tokio::main]
async fn main() -> AgentResult<()> {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    info!("Connecting to remote A2A Agent at http://localhost:3000...");

    // 一行连接远程 A2A Agent（自动发现 AgentCard）
    let remote = A2AAgent::connect("http://localhost:3000")
        .await?
        .with_meta(AgentMeta::a2a_agent(
            "parent_run_id-1".into(),
            42,
            "parent_session_id-1".into(),
        ));

    info!(
        "Connected to: {} — {}",
        remote.name(),
        remote.agent_card().description
    );
    info!(
        "Skills: {:?}",
        remote
            .agent_card()
            .skills
            .iter()
            .map(|s| &s.name)
            .collect::<Vec<_>>()
    );

    // ─── 场景 1：非流式调用 ───────────────────────────────────────────────
    info!("\n=== Scenario 1: Non-streaming call ===");
    let outcome = remote.run("session-1", "Hello, agent!").await?;
    info!("Answer: {}", outcome.final_answer);
    info!(
        "Success: {}, Steps: {}",
        outcome.success, outcome.total_steps
    );

    // ─── 场景 2：流式调用 ─────────────────────────────────────────────────
    info!("\n=== Scenario 2: Streaming call ===");
    let mut stream = remote.run_stream("session-2", "Tell me a joke")?;

    while let Some(event) = stream.next().await {
        use agent_runtime::session::AgentStreamEvent;
        match event {
            AgentStreamEvent::TextDelta { delta, .. } => {
                print!("{delta}");
            }
            AgentStreamEvent::Answer { content, .. } => {
                info!("\nFinal answer: {content}");
            }
            AgentStreamEvent::RunFinished { finish_reason, .. } => {
                info!("Run finished: {finish_reason:?}");
            }
            _ => {}
        }
    }

    // ─── 场景 3：多轮对话（同一 session_id）─────────────────────────────
    info!("\n=== Scenario 3: Multi-turn conversation ===");
    let session_id = "multi-turn-session";

    let r1 = remote.run(session_id, "My name is Alice.").await?;
    info!("Turn 1: {}", r1.final_answer);

    let r2 = remote.run(session_id, "What is my name?").await?;
    info!("Turn 2: {}", r2.final_answer);

    Ok(())
}
