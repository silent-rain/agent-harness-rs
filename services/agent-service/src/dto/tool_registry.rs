//! 工具注册表管理

use serde::{Deserialize, Serialize};
use validator::Validate;

use entity::agent::tool_registry;

// ─── 查询工具列表 ───────────────────────────────────────────────────────────

/// 查询工具列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetToolRegistriesReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 开始时间
    pub start_time: Option<String>,
    /// 结束时间
    pub end_time: Option<String>,
    /// 工具名称模糊搜索
    pub name: Option<String>,
    /// 启用状态过滤
    pub enabled: Option<bool>,
    /// 返回所有数据
    pub all: Option<bool>,
}

/// 查询工具列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetToolRegistriesResp {
    pub data_list: Vec<tool_registry::Model>,
    pub total: u64,
}

// ─── 查询工具详情 ───────────────────────────────────────────────────────────

/// 查询工具详情 请求体
#[derive(Debug, Default, Serialize, Deserialize, Validate)]
pub struct GetToolRegistryReq {
    /// 工具 ID
    pub id: i64,
}

/// 查询工具详情 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetToolRegistryResp {
    #[serde(flatten)]
    pub model: tool_registry::Model,
}

// ─── 创建工具 ───────────────────────────────────────────────────────────────

/// 创建工具 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct CreateToolRegistryReq {
    /// 工具唯一编码
    #[validate(length(min = 1, max = 128, message = "工具编码长度须在 1~128 字符之间"))]
    pub tool_code: String,
    /// 工具名称
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 工具描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 函数 Schema（JSON Schema 格式）
    pub schema_json: Option<String>,
    /// 是否启用
    pub enabled: Option<bool>,
}

/// 创建工具 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateToolRegistryResp {
    /// 新建的工具 ID
    pub id: i64,
}

// ─── 更新工具 ───────────────────────────────────────────────────────────────

/// 更新工具 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateToolRegistryReq {
    /// 工具 ID
    pub id: i64,
    /// 工具唯一编码
    #[validate(length(min = 1, max = 128, message = "工具编码长度须在 1~128 字符之间"))]
    pub tool_code: String,
    /// 工具名称
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 工具描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 函数 Schema（JSON Schema 格式）
    pub schema_json: Option<String>,
}

// ─── 更新启用状态
// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新工具启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateToolRegistryEnabledReq {
    /// 工具 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除工具 ───────────────────────────────────────────────────────────────

/// 删除工具 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteToolRegistryReq {
    /// 工具 ID
    pub id: i64,
}
