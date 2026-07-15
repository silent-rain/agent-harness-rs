//! MCP 服务注册表管理

use serde::{Deserialize, Serialize};
use validator::Validate;

use entity::agent::mcp_service_registry;

// ─── 查询注册表列表 ───────────────────────────────────────────────────────────

/// 查询 MCP 服务注册表列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetMcpServiceRegistriesReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 开始时间
    pub start_time: Option<String>,
    /// 结束时间
    pub end_time: Option<String>,
    /// 名称模糊搜索
    pub name: Option<String>,
    /// 启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询 MCP 服务注册表列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetMcpServiceRegistriesResp {
    pub data_list: Vec<mcp_service_registry::Model>,
    pub total: u64,
}

// ─── 查询注册表详情 ───────────────────────────────────────────────────────────

/// 查询 MCP 服务注册表详情 请求体
#[derive(Debug, Default, Serialize, Deserialize, Validate)]
pub struct GetMcpServiceRegistryReq {
    /// 注册表 ID
    pub id: i64,
}

// ─── 创建注册表 ───────────────────────────────────────────────────────────────

/// 创建 MCP 服务注册表 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct CreateMcpServiceRegistryReq {
    /// 服务名称（同一用户下唯一）
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 服务描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// MCP 服务地址（stdio 类型为空）
    pub mcp_service_url: Option<String>,
    /// 传输类型: stdio, sse, streamable-http
    #[validate(length(min = 1, message = "传输类型不能为空"))]
    pub transport_type: String,
    /// 允许使用的工具列表（JSON 数组，空=全部可用）
    pub enabled_tools: Option<String>,
    /// MCP 服务配置（JSON）
    pub config_json: Option<String>,
    /// 是否启用
    pub enabled: Option<bool>,
}

/// 创建 MCP 服务注册表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMcpServiceRegistryResp {
    /// 新建的注册表 ID
    pub id: i64,
}

// ─── 更新注册表 ───────────────────────────────────────────────────────────────

/// 更新 MCP 服务注册表 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMcpServiceRegistryReq {
    /// 注册表 ID
    pub id: i64,
    /// 服务名称
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 服务描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// MCP 服务地址（stdio 类型为空）
    pub mcp_service_url: Option<String>,
    /// 传输类型: stdio, sse, streamable-http
    #[validate(length(min = 1, message = "传输类型不能为空"))]
    pub transport_type: String,
    /// 允许使用的工具列表（JSON 数组，空=全部可用）
    pub enabled_tools: Option<String>,
    /// MCP 服务配置（JSON）
    pub config_json: Option<String>,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新 MCP 服务注册表启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMcpServiceRegistryEnabledReq {
    /// 注册表 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除注册表 ───────────────────────────────────────────────────────────────

/// 删除 MCP 服务注册表 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteMcpServiceRegistryReq {
    /// 注册表 ID
    pub id: i64,
}
