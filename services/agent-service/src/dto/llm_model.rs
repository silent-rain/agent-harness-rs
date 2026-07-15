//! LLM 模型配置管理 DTO

use entity::agent::llm_model;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询列表 ─────────────────────────────────────────────────────────────────

/// 查询 LLM 模型列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetLlmModelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 供应商过滤，如 openai / qwen / glm
    pub provider: Option<String>,
    /// 模型编码过滤
    pub model_code: Option<String>,
    /// 启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询 LLM 模型列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetLlmModelsResp {
    pub data_list: Vec<llm_model::Model>,
    pub total: u64,
}

// ─── 查询详情 ─────────────────────────────────────────────────────────────────

/// 查询 LLM 模型详情 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct GetLlmModelReq {
    /// 模型 ID
    pub id: i64,
}

/// 查询 LLM 模型详情 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetLlmModelResp {
    #[serde(flatten)]
    pub model: llm_model::Model,
}

// ─── 创建模型 ─────────────────────────────────────────────────────────────────

/// 创建 LLM 模型 请求体
#[derive(Deserialize, Validate)]
pub struct CreateLlmModelReq {
    /// 供应商，如 openai / qwen / glm
    #[validate(length(min = 1, message = "供应商不能为空"))]
    pub provider: String,
    /// 模型编码，如 gpt-4o / qwen-plus
    #[validate(length(min = 1, message = "模型编码不能为空"))]
    pub model_code: String,
    /// 模型显示名称
    #[validate(length(min = 1, message = "模型名称不能为空"))]
    pub model_name: String,
    /// API 基础地址
    #[validate(length(min = 1, message = "API 基础地址不能为空"))]
    pub base_url: String,
    /// API Key
    #[validate(length(min = 1, message = "API Key 不能为空"))]
    pub api_key: String,
    /// 是否启用
    pub enabled: bool,
}

/// 创建 LLM 模型 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateLlmModelResp {
    /// 新建记录 ID
    pub id: i64,
}

// ─── 更新模型 ─────────────────────────────────────────────────────────────────

/// 更新 LLM 模型 请求体
#[derive(Deserialize, Validate)]
pub struct UpdateLlmModelReq {
    /// 模型 ID
    pub id: i64,
    /// API 基础地址
    #[validate(length(min = 1, message = "API 基础地址不能为空"))]
    pub base_url: String,
    /// API Key
    #[validate(length(min = 1, message = "API Key 不能为空"))]
    pub api_key: String,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新 LLM 模型启用状态 请求体
#[derive(Deserialize, Validate)]
pub struct UpdateLlmModelEnabledReq {
    /// 模型 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除模型 ─────────────────────────────────────────────────────────────────

/// 删除 LLM 模型 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteLlmModelReq {
    /// 模型 ID
    pub id: i64,
}
