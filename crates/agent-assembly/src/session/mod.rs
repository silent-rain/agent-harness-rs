//! 会话持久化数据访问层
//!
//! 提供 `t_agent_session` / `t_agent_session_event` / `t_agent_session_state`
//! / `t_agent_session_user_state` / `t_agent_session_agent_state` /
//! `t_agent_session_pending_input` 六张核心表的 DAO 封装，
//! 以及基于这些 DAO 组装的 [`DbSessionService`]，作为
//! [`agent_runtime::session::SessionService`] 的数据库实现。
//!
//! 模块组织：
//!
//! - [`SessionDao`] —— 会话主表 CRUD
//! - [`SessionEventDao`] —— 会话事件流追加 / 顺序读取
//! - [`SessionStateDao`] —— 三层上下文（Session / User / Agent）按序追加 / 列表读取
//! - [`PendingInputDao`] —— 会话注入消息队列（写入 / 消费）
//! - [`SessionStatus`] —— 会话状态枚举（消除魔数）
//! - [`DbSessionService`] —— `SessionService` trait 的数据库实现

pub mod db_session_service;
pub mod pending_input_dao;
pub mod session_dao;
pub mod session_event_dao;
pub mod session_state_dao;

pub use db_session_service::DbSessionService;
pub use pending_input_dao::{PendingInputDao, PendingInputEntry};
pub use session_dao::{NewSession, SessionDao, SessionUpdate};
pub use session_event_dao::{NewSessionEvent, SessionEventDao};
pub use session_state_dao::{SessionStateDao, SessionStateEntry};
