//! 智能体子智能体绑定关系管理

use entity::agent::sub_agent_rel;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询绑定列表 ─────────────────────────────────────────────────────────────

/// 查询智能体子智能体绑定列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetSubAgentRelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 主智能体 ID（必填）
    pub agent_id: i64,
    /// 绑定启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询智能体子智能体绑定列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetSubAgentRelsResp {
    pub data_list: Vec<sub_agent_rel::SubAgentRelWithAgent>,
    pub total: u64,
}

// ─── 新增绑定 ────────────────────────────────────────────────────────────────

/// 新增智能体子智能体绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct AddSubAgentRelReq {
    /// 主智能体 ID
    pub agent_id: i64,
    /// 子智能体 ID
    pub sub_agent_id: i64,
    /// 子智能体别名（在主智能体中的引用名）
    pub alias: Option<String>,
    /// 是否共享上下文记忆
    pub share_context: bool,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新绑定 ────────────────────────────────────────────────────────────────

/// 更新智能体子智能体绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateSubAgentRelReq {
    /// 关系 ID
    pub id: i64,
    /// 子智能体别名（在主智能体中的引用名）
    pub alias: Option<String>,
    /// 是否共享上下文记忆
    pub share_context: bool,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新智能体子智能体绑定启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateSubAgentRelEnabledReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除绑定 ────────────────────────────────────────────────────────────────

/// 删除智能体子智能体绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct DeleteSubAgentRelReq {
    /// 关系 ID
    pub id: i64,
}
