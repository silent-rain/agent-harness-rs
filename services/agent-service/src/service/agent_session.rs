//! 智能体会话管理服务
//!
//! 提供会话级别的取消和状态查询能力，通过本地 [`AgentSessionDao`]

use log::error;
use nject::injectable;

use agent_base::message::ConversationMessage;
use err_code::Error;

use crate::{
    dao::{agent_session::AgentSessionDao, session_pending_input::SessionPendingInputDao},
    dto::agent_session::{
        InjectMessageReq, InjectMessageResp, SessionCancelResp, SessionStatusResp,
    },
    enums::agent_session::SessionStatus,
};

/// 智能体会话管理服务
///
/// 封装会话取消与状态查询的业务逻辑，直接通过 [`AgentSessionDao`]
/// 操作数据库，无需构建 Agent 实例。
#[injectable]
pub struct AgentSessionService {
    session_dao: AgentSessionDao,
    pending_input_dao: SessionPendingInputDao,
}

impl AgentSessionService {
    /// 取消指定会话的活跃运行
    ///
    /// 仅当会话当前状态为 `Running` 时才执行取消操作，
    /// 将数据库中的会话状态更新为 [`SessionStatus::Cancelled`]。
    /// Agent 运行循环在流式推理中通过定时轮询检测到状态变更后退出。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前请求用户 ID（用于权限校验）
    /// * `session_id` - 会话 ID
    ///
    /// # Returns
    ///
    /// 取消结果（包含是否成功和提示信息）
    pub async fn cancel(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> anyhow::Result<SessionCancelResp> {
        // 查询会话是否存在
        let model = self.session_dao.get(session_id).await.map_err(|e| {
            error!("查询会话失败, session_id: {}, err: {:#?}", session_id, e);
            Error::DbQueryError("查询会话失败".to_string())
        })?;

        let Some(model) = model else {
            return Ok(SessionCancelResp {
                cancelled: false,
                message: "会话不存在".to_string(),
            });
        };

        // 权限校验：仅允许会话所属用户操作
        if model.user_id != user_id {
            error!(
                "无权操作该会话, session_id: {}, owner: {}, current: {}",
                session_id, model.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("会话不存在或无权访问".to_string()).into());
        }

        let status = SessionStatus::from_raw(model.status);

        // 仅 Running 状态允许取消
        if !status.is_running() {
            return Ok(SessionCancelResp {
                cancelled: false,
                message: format!("会话非运行中状态（当前: {:?}），无法取消", status),
            });
        }

        // 更新状态为 Cancelled
        self.session_dao
            .update_status(session_id, SessionStatus::Cancelled)
            .await
            .map_err(|e| {
                error!(
                    "取消失败：更新会话状态出错, session_id: {}, err: {:#?}",
                    session_id, e
                );
                Error::DbUpdateError("更新会话状态失败".to_string())
            })?;

        Ok(SessionCancelResp {
            cancelled: true,
            message: "已成功取消".to_string(),
        })
    }

    /// 向运行中的会话注入用户消息
    ///
    /// 与 `cancel` 完全对称的服务层接口：
    /// 1. 校验会话存在且为 Running 状态
    /// 2. 权限校验：仅允许会话所属用户操作
    /// 3. 将消息序列化为 `ConversationMessage` JSON 写入 `t_agent_session_pending_input` 队列表
    /// 4. Agent 运行循环在下一轮通过 `drain_pending_input` 自动消费新消息
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前请求用户 ID（用于权限校验）
    /// * `req` - 消息注入请求
    ///
    /// # Returns
    ///
    /// 注入结果（包含是否成功和提示信息）
    pub async fn inject_message(
        &self,
        user_id: &str,
        req: InjectMessageReq,
    ) -> anyhow::Result<InjectMessageResp> {
        // 1. 校验会话状态
        let model = self.session_dao.get(&req.session_id).await.map_err(|e| {
            error!(
                "查询会话失败, session_id: {}, err: {:#?}",
                req.session_id, e
            );
            Error::DbQueryError("查询会话失败".to_string())
        })?;

        let Some(model) = model else {
            return Ok(InjectMessageResp {
                injected: false,
                message: "会话不存在".to_string(),
            });
        };

        // 2. 权限校验：仅允许会话所属用户操作
        if model.user_id != user_id {
            error!(
                "无权操作该会话, session_id: {}, owner: {}, current: {}",
                req.session_id, model.user_id, user_id
            );
            return Err(Error::DbQueryEmptyError("会话不存在或无权访问".to_string()).into());
        }

        // 校验会话状态
        let status = SessionStatus::from_raw(model.status);
        if !status.is_running() {
            return Ok(InjectMessageResp {
                injected: false,
                message: format!("会话非运行中状态（当前: {:?}），无法注入消息", status),
            });
        }

        // 3. 构造 ConversationMessage 并序列化为 JSON
        let conv_msg = ConversationMessage::user(&req.message);
        let content_json = serde_json::to_string(&conv_msg).map_err(|e| {
            error!(
                "序列化消息失败, session_id: {}, err: {:#?}",
                req.session_id, e
            );
            Error::JsonSerialization("序列化消息失败".to_string())
        })?;

        // 4. 写入 pending_input 队列表
        self.pending_input_dao
            .append_pending_input(&req.session_id, req.agent_id, user_id, content_json)
            .await
            .map_err(|e| {
                error!(
                    "消息注入失败：写入注入队列出错, session_id: {}, err: {:#?}",
                    req.session_id, e
                );
                Error::DbAddError("写入注入队列失败".to_string())
            })?;

        Ok(InjectMessageResp {
            injected: true,
            message: "消息已成功注入".to_string(),
        })
    }

    /// 查询指定会话的运行状态
    ///
    /// 从数据库读取会话状态，返回会话是否存在及当前状态。
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前请求用户 ID（用于权限校验）
    /// * `session_id` - 会话 ID
    ///
    /// # Returns
    ///
    /// 状态查询结果（包含是否存在、状态名称和状态码）
    pub async fn status(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> anyhow::Result<SessionStatusResp> {
        let model = self.session_dao.get(session_id).await.map_err(|e| {
            error!("查询会话失败, session_id: {}, err: {:#?}", session_id, e);
            Error::DbQueryError("查询会话失败".to_string())
        })?;

        match model {
            Some(m) => {
                // 权限校验：仅允许会话所属用户查询
                if m.user_id != user_id {
                    error!(
                        "无权操作该会话, session_id: {}, owner: {}, current: {}",
                        session_id, m.user_id, user_id
                    );
                    return Err(Error::DbQueryEmptyError("会话不存在或无权访问".to_string()).into());
                }
                let status = SessionStatus::from_raw(m.status);
                Ok(SessionStatusResp {
                    exists: true,
                    status: Some(format!("{:?}", status)),
                    status_code: Some(m.status),
                })
            }
            None => Ok(SessionStatusResp {
                exists: false,
                status: None,
                status_code: None,
            }),
        }
    }
}
