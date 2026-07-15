//! 智能体 MCP 服务绑定关系管理

use entity::agent::mcp_service_rel::McpServiceRelWithRegistry;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询绑定列表 ─────────────────────────────────────────────────────────────

/// 查询智能体 MCP 服务绑定列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetMcpServiceRelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 智能体 ID（必填）
    pub agent_id: i64,
    /// 绑定启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询智能体 MCP 服务绑定列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetMcpServiceRelsResp {
    pub data_list: Vec<McpServiceRelWithRegistry>,
    pub total: u64,
}

// ─── 新增绑定 ────────────────────────────────────────────────────────────────

/// 新增智能体 MCP 服务绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct AddMcpServiceRelReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// MCP 服务注册表 ID
    pub registry_id: i64,
    /// 允许使用的工具列表覆盖（JSON 数组，空=使用注册表默认值）
    pub enabled_tools: String,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新绑定 ────────────────────────────────────────────────────────────────

/// 更新智能体 MCP 服务绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMcpServiceRelReq {
    /// 关系 ID
    pub id: i64,
    /// 允许使用的工具列表覆盖（JSON 数组，空=使用注册表默认值）
    pub enabled_tools: String,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新智能体 MCP 服务绑定启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMcpServiceRelEnabledReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除绑定 ────────────────────────────────────────────────────────────────

/// 删除智能体 MCP 服务绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct DeleteMcpServiceRelReq {
    /// 关系 ID
    pub id: i64,
}
