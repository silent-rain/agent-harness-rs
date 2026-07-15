//! 智能体运行服务
//!
//! 负责从平台装配 `Agent` 并执行运行，
//! 支持多种运行模式：
//!
//! - **标准运行**（`run` / `run_stream`）：使用 `AgentOpenAI` 协议，返回 OpenAI 兼容格式
//! - **调试运行**（`debug_run` / `debug_run_stream`）：使用 `Agent` 内部事件流，输出详细调试信息
//! - **配置运行**（`run_with_config` / `run_stream_with_config`）：agent_config 与 model 从请求中传入

use std::{convert::Infallible, sync::Arc};

use axum::response::sse::Event;
use futures::StreamExt;
use nject::injectable;
use uuid::Uuid;

use agent_assembly::{DbAgentSource, DbSessionService};
use agent_base::{
    Agent, AgentBuilder, AgentOpenAI, LlmAgent, RuntimeServices, runtime::AgentSource,
    session::RunOutcome, tool::simple_tools::all_builtin_tools, types::runtime::AgentConfig,
};
use inject::PrimaryDb;

use crate::dto::agent_run::{AgentConfigRunReq, AgentDebugRunReq, AgentRunReq, SseEventData};

/// 智能体运行服务
///
/// 封装从配置装配到运行的完整流程。
/// 模型在构造时会被一次性转换为内部适配后的对象，调用方可以直接传入 rig model。
#[injectable]
pub struct AgentRunService {
    pub db: PrimaryDb,
}

impl AgentRunService {
    // ─── 标准运行（AgentOpenAI 协议） ────────────────────────────────────────

    /// 标准非流式运行（OpenAI 兼容）
    ///
    /// 使用 `AgentOpenAI::run` 推理，返回 OpenAI Chat Completions 格式响应。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 调用用户 ID
    /// * `req` - 运行请求
    ///
    /// # Returns
    ///
    /// OpenAI 标准非流式响应
    pub async fn run(
        &self,
        user_id: i64,
        req: AgentRunReq,
    ) -> anyhow::Result<agent_base::openai::ChatCompletion> {
        let session_id = req
            .session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let agent = self.build_agent(req.agent_id, user_id).await?;
        let result = AgentOpenAI::run(&agent, &session_id, &req.input).await?;
        Ok(result)
    }

    /// 标准流式运行（OpenAI 兼容）
    ///
    /// 使用 `AgentOpenAI::run_stream` 推理，返回 SSE Event 流。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 调用用户 ID
    /// * `req` - 运行请求
    ///
    /// # Returns
    ///
    /// SSE 事件流
    pub async fn run_stream(
        &self,
        user_id: i64,
        req: AgentRunReq,
    ) -> anyhow::Result<impl futures::Stream<Item = Result<Event, Infallible>> + use<>> {
        let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        let agent = self.build_agent(req.agent_id, user_id).await?;
        let mut stream = AgentOpenAI::run_stream(&agent, &session_id, &req.input)?;

        let sse_stream = async_stream::stream! {
            while let Some(chunk) = stream.next().await {
                let data = serde_json::to_string(&chunk).unwrap_or_default();
                yield Ok(Event::default().data(data));
            }
        };

        Ok(sse_stream)
    }

    // ─── 调试运行（Agent 内部事件流） ────────────────────────────────────────

    /// 调试非流式运行
    ///
    /// 使用 `Agent::run` 推理，返回内部 `RunOutcome`，包含详细的步骤信息。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 调用用户 ID
    /// * `req` - 调试运行请求
    ///
    /// # Returns
    ///
    /// 内部运行结果（最终回答 + 步骤统计）
    pub async fn debug_run(
        &self,
        user_id: i64,
        req: AgentDebugRunReq,
    ) -> anyhow::Result<RunOutcome> {
        let session_id = req
            .session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let agent = self.build_agent(req.agent_id, user_id).await?;
        let result = Agent::run(&agent, &session_id, &req.input).await?;
        Ok(result)
    }

    /// 调试流式运行
    ///
    /// 将调试运行的内部事件流转换为 SSE Event 流。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 调用用户 ID
    /// * `req` - 调试运行请求
    ///
    /// # Returns
    ///
    /// SSE 事件流
    pub async fn debug_run_stream(
        &self,
        user_id: i64,
        req: AgentDebugRunReq,
    ) -> anyhow::Result<impl futures::Stream<Item = Result<Event, Infallible>> + use<>> {
        let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        let agent = self.build_agent(req.agent_id, user_id).await?;
        let mut stream = Agent::run_stream(&agent, &session_id, &req.input)?;

        // 将内部事件流转换为 SSE Event 流
        let sse_stream = async_stream::stream! {
             while let Some(event) = stream.next().await {
                   let sse_data = SseEventData::from_agent_event(&event);
                 let data = serde_json::to_string(&sse_data.data).unwrap_or_default();
                let sse_event = Event::default().event(&sse_data.event).data(data);
                yield Ok(sse_event);
            }
        };

        Ok(sse_stream)
    }

    // ─── 通用配置运行（AgentOpenAI 协议，配置从请求传入） ────────────────────

    /// 通用非流式运行（OpenAI 兼容，配置从请求传入）
    ///
    /// 使用 `AgentOpenAI::run` 推理，`agent_config` 与 `model_config` 直接从请求中获取，
    /// 不从数据库加载智能体配置。
    ///
    /// # Parameters
    ///
    /// * `req` - 通用配置运行请求（含 agent_config、model_config）
    ///
    /// # Returns
    ///
    /// OpenAI 标准非流式响应
    pub async fn run_with_config(
        &self,
        req: AgentConfigRunReq,
    ) -> anyhow::Result<agent_base::openai::ChatCompletion> {
        let session_id = req
            .session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let agent = self
            .build_agent_from_config(AgentConfig {
                agent_definition: req.agent_definition,
                model_config: req.model_config,
            })
            .await?;
        let result = AgentOpenAI::run(&agent, &session_id, &req.input).await?;
        Ok(result)
    }

    /// 通用流式运行（OpenAI 兼容，配置从请求传入）
    ///
    /// 使用 `AgentOpenAI::run_stream` 推理，`agent_config` 与 `model_config` 直接从请求中获取，
    /// 不从数据库加载智能体配置。
    ///
    /// # Parameters
    ///
    /// * `req` - 通用配置运行请求（含 agent_config、model_config）
    ///
    /// # Returns
    ///
    /// SSE 事件流
    pub async fn run_stream_with_config(
        &self,
        req: AgentConfigRunReq,
    ) -> anyhow::Result<impl futures::Stream<Item = Result<Event, Infallible>> + use<>> {
        let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        let agent = self
            .build_agent_from_config(AgentConfig {
                agent_definition: req.agent_definition,
                model_config: req.model_config,
            })
            .await?;
        let mut stream = AgentOpenAI::run_stream(&agent, &session_id, &req.input)?;

        let sse_stream = async_stream::stream! {
            while let Some(chunk) = stream.next().await {
                let data = serde_json::to_string(&chunk).unwrap_or_default();
                yield Ok(Event::default().data(data));
            }
        };

        Ok(sse_stream)
    }

    // ─── 内部方法 ─────────────────────────────────────────────────────────────

    /// 从数据库构建 Agent
    ///
    /// 通过 [`DbAgentSource`] 一次性加载完整的智能体配置
    /// （含规则、命令、技能、MCP、工具、子智能体），然后用 [`AgentBuilder`]
    /// 组装为可运行的 [`LlmAgent`]。
    async fn build_agent(&self, agent_id: i64, user_id: i64) -> anyhow::Result<LlmAgent> {
        let source = DbAgentSource::new(self.db.db().clone(), agent_id, user_id);
        let bundle = source.load().await?;

        let session_service = Arc::new(DbSessionService::new(Arc::new(self.db.db().clone())));
        let services = RuntimeServices {
            session: session_service,
            ..RuntimeServices::noop()
        };

        let agent = AgentBuilder::new(bundle.to_agent_config())?
            .with_services(services)
            .add_source(source)
            .add_tools(all_builtin_tools())
            .build()
            .await?;

        Ok(agent)
    }

    /// 从请求配置构建 Agent
    ///
    /// 直接使用传入的 `agent_config` 与 `model_config` 构建 [`LlmAgent`]，
    /// 不从数据库加载，适用于通用配置运行接口。
    ///
    /// # Parameters
    ///
    /// * `agent_config` - 智能体配置（身份信息）
    /// * `model_config` - 模型配置（供应商、模型编码、API Key 等）
    async fn build_agent_from_config(&self, agent_config: AgentConfig) -> anyhow::Result<LlmAgent> {
        let session_service = Arc::new(DbSessionService::new(Arc::new(self.db.db().clone())));
        let services = RuntimeServices {
            session: session_service,
            ..RuntimeServices::noop()
        };

        let agent = AgentBuilder::new(agent_config)?
            .with_services(services)
            .add_tools(all_builtin_tools())
            .build()
            .await?;

        Ok(agent)
    }
}
