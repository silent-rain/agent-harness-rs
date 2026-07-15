//! 智能体配置管理

use serde::{Deserialize, Serialize};
use validator::Validate;

use entity::agent::agent_config;

// ─── 查询智能体列表 ───────────────────────────────────────────────────────────

/// 查询智能体列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetAgentConfigsReq {
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
}

/// 查询智能体列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetAgentConfigsResp {
    pub data_list: Vec<agent_config::Model>,
    pub total: u64,
}

// ─── 查询智能体详情 ───────────────────────────────────────────────────────────

/// 查询智能体详情 请求体
#[derive(Debug, Default, Serialize, Deserialize, Validate)]
pub struct GetAgentConfigReq {
    /// 智能体 ID
    pub id: i64,
}

/// 查询智能体详情 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetAgentConfigResp {
    #[serde(flatten)]
    pub model: agent_config::Model,
}

// ─── 创建智能体 ───────────────────────────────────────────────────────────────

/// 创建智能体 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct CreateAgentConfigReq {
    /// 智能体名称
    #[validate(length(min = 1, max = 100, message = "名称长度须在 1~100 字符之间"))]
    pub name: String,
    /// 智能体描述
    #[validate(length(max = 500, message = "描述不超过 500 字符"))]
    pub description: String,
    /// 欢迎语
    pub welcome_message: Option<String>,
    /// 系统 Prompt
    pub system_prompt: String,
    /// 最大子智能体递归委派深度（默认 3，范围 1~10）
    #[validate(range(min = 1, max = 10, message = "委派深度须在 1~10 之间"))]
    pub max_delegation_depth: Option<i32>,
    /// 最大推理轮次
    pub max_turns: Option<i32>,
}

/// 创建智能体 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAgentConfigResp {
    /// 新建的智能体 ID
    pub id: i64,
}

// ─── 更新智能体 ───────────────────────────────────────────────────────────────

/// 更新智能体 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateAgentConfigReq {
    /// 智能体 ID
    pub id: i64,
    /// 智能体名称
    #[validate(length(min = 1, max = 100, message = "名称长度须在 1~100 字符之间"))]
    pub name: String,
    /// 智能体描述
    #[validate(length(max = 500, message = "描述不超过 500 字符"))]
    pub description: String,
    /// 欢迎语
    pub welcome_message: Option<String>,
    /// 系统 Prompt
    pub system_prompt: String,
    /// 最大子智能体递归委派深度（范围 1~10）
    #[validate(range(min = 1, max = 10, message = "委派深度须在 1~10 之间"))]
    pub max_delegation_depth: Option<i32>,
    /// 最大推理轮次
    pub max_turns: Option<i32>,
}

// ─── 删除智能体
// ─── 删除智能体 ───────────────────────────────────────────────────────────────

/// 删除智能体 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteAgentConfigReq {
    /// 智能体 ID
    pub id: i64,
}
