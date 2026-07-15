//! A2A 服务注册表管理

use serde::{Deserialize, Serialize};
use validator::Validate;

use entity::agent::a2a_service_registry;

// ─── 查询注册表列表 ───────────────────────────────────────────────────────────

/// 查询 A2A 服务注册表列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetA2aServiceRegistriesReq {
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

/// 查询 A2A 服务注册表列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetA2aServiceRegistriesResp {
    pub data_list: Vec<a2a_service_registry::Model>,
    pub total: u64,
}

// ─── 查询注册表详情 ───────────────────────────────────────────────────────────

/// 查询 A2A 服务注册表详情 请求体
#[derive(Debug, Default, Serialize, Deserialize, Validate)]
pub struct GetA2aServiceRegistryReq {
    /// 注册表 ID
    pub id: i64,
}

// ─── 创建注册表 ───────────────────────────────────────────────────────────────

/// 创建 A2A 服务注册表 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct CreateA2aServiceRegistryReq {
    /// 服务名称（同一用户下唯一，LLM 可见）
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 服务描述（LLM 可见）
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 远程 A2A 服务基础 URL
    #[validate(length(min = 1, message = "服务地址不能为空"))]
    pub base_url: String,
    /// 连接超时（秒），NULL 使用默认值 60
    pub timeout_secs: Option<i32>,
    /// A2A 服务配置（JSON）
    pub config_json: Option<String>,
    /// 是否启用
    pub enabled: Option<bool>,
}

/// 创建 A2A 服务注册表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateA2aServiceRegistryResp {
    /// 新建的注册表 ID
    pub id: i64,
}

// ─── 更新注册表 ───────────────────────────────────────────────────────────────

/// 更新 A2A 服务注册表 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateA2aServiceRegistryReq {
    /// 注册表 ID
    pub id: i64,
    /// 服务名称
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 服务描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 远程 A2A 服务基础 URL
    #[validate(length(min = 1, message = "服务地址不能为空"))]
    pub base_url: String,
    /// 连接超时（秒）
    pub timeout_secs: Option<i32>,
    /// A2A 服务配置（JSON）
    pub config_json: Option<String>,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新 A2A 服务注册表启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateA2aServiceRegistryEnabledReq {
    /// 注册表 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除注册表 ───────────────────────────────────────────────────────────────

/// 删除 A2A 服务注册表 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteA2aServiceRegistryReq {
    /// 注册表 ID
    pub id: i64,
}
