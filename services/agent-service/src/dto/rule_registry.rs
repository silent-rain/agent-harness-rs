//! 规则注册表管理

use serde::{Deserialize, Serialize};
use validator::Validate;

use entity::agent::rule_registry;

// ─── 查询规则列表 ───────────────────────────────────────────────────────────

/// 查询规则列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetRuleRegistriesReq {
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

/// 查询规则列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetRuleRegistriesResp {
    pub data_list: Vec<rule_registry::Model>,
    pub total: u64,
}

// ─── 查询规则详情 ───────────────────────────────────────────────────────────

/// 查询规则详情 请求体
#[derive(Debug, Default, Serialize, Deserialize, Validate)]
pub struct GetRuleRegistryReq {
    /// 规则 ID
    pub id: i64,
}

/// 查询规则详情 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetRuleRegistryResp {
    #[serde(flatten)]
    pub model: rule_registry::Model,
}

// ─── 创建规则 ───────────────────────────────────────────────────────────────

/// 创建规则 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct CreateRuleRegistryReq {
    /// 规则名称
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 规则描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 规则内容（注入 System Prompt 的文本）
    #[validate(length(min = 1, message = "规则内容不能为空"))]
    pub content: String,
    /// 是否启用
    pub enabled: Option<bool>,
}

/// 创建规则 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRuleRegistryResp {
    /// 新建的规则 ID
    pub id: i64,
}

// ─── 更新规则 ───────────────────────────────────────────────────────────────

/// 更新规则 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateRuleRegistryReq {
    /// 规则 ID
    pub id: i64,
    /// 规则名称
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 规则描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 规则内容（注入 System Prompt 的文本）
    #[validate(length(min = 1, message = "规则内容不能为空"))]
    pub content: String,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新规则启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateRuleRegistryEnabledReq {
    /// 规则 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除规则 ───────────────────────────────────────────────────────────────

/// 删除规则 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteRuleRegistryReq {
    /// 规则 ID
    pub id: i64,
}
