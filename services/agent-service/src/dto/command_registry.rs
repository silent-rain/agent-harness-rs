//! 命令注册表管理

use serde::{Deserialize, Serialize};
use validator::Validate;

use entity::agent::command_registry;

// ─── 查询命令列表 ───────────────────────────────────────────────────────────

/// 查询命令列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetCommandRegistriesReq {
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

/// 查询命令列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetCommandRegistriesResp {
    pub data_list: Vec<command_registry::Model>,
    pub total: u64,
}

// ─── 查询命令详情 ───────────────────────────────────────────────────────────

/// 查询命令详情 请求体
#[derive(Debug, Default, Serialize, Deserialize, Validate)]
pub struct GetCommandRegistryReq {
    /// 命令 ID
    pub id: i64,
}

/// 查询命令详情 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetCommandRegistryResp {
    #[serde(flatten)]
    pub model: command_registry::Model,
}

// ─── 创建命令 ───────────────────────────────────────────────────────────────

/// 创建命令 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct CreateCommandRegistryReq {
    /// 命令名称
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 命令描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 命令提示词正文（完整的 Markdown 内容）
    #[validate(length(min = 1, message = "命令内容不能为空"))]
    pub content: String,
    /// 是否启用
    pub enabled: Option<bool>,
}

/// 创建命令 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCommandRegistryResp {
    /// 新建的命令 ID
    pub id: i64,
}

// ─── 更新命令 ───────────────────────────────────────────────────────────────

/// 更新命令 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCommandRegistryReq {
    /// 命令 ID
    pub id: i64,
    /// 命令描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 命令提示词正文（完整的 Markdown 内容）
    #[validate(length(min = 1, message = "命令内容不能为空"))]
    pub content: String,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新命令启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCommandRegistryEnabledReq {
    /// 命令 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除命令 ───────────────────────────────────────────────────────────────

/// 删除命令 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteCommandRegistryReq {
    /// 命令 ID
    pub id: i64,
}
