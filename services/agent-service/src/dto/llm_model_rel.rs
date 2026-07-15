//! 智能体 LLM 模型绑定关系管理

use entity::agent::llm_model_rel;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询绑定列表 ─────────────────────────────────────────────────────────────

/// 查询智能体 LLM 模型绑定列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetLlmModelRelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 智能体 ID（必填）
    pub agent_id: i64,
    /// 绑定启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询智能体 LLM 模型绑定列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetLlmModelRelsResp {
    pub data_list: Vec<llm_model_rel::Model>,
    pub total: u64,
}

// ─── 批量同步绑定 ─────────────────────────────────────────────────────────────

/// 批量同步智能体 LLM 模型绑定 请求体
///
/// 传入 agent_id 和目标 model_ids 列表，系统自动计算差异：
/// - 新增：目标中有但当前没有的绑定
/// - 删除：当前有但目标中没有的绑定
/// - 保持：交集部分不变
#[derive(Debug, Deserialize, Validate)]
pub struct BatchSaveLlmModelRelsReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// 目标 LLM 模型 ID 列表
    pub model_ids: Vec<i64>,
}

/// 批量同步智能体 LLM 模型绑定 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchSaveLlmModelRelsResp {
    /// 新增的绑定数量
    pub added: usize,
    /// 删除的绑定数量
    pub removed: usize,
}

// ─── 更新绑定 ─────────────────────────────────────────────────────────────────

/// 更新智能体 LLM 模型绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateLlmModelRelReq {
    /// 关系 ID
    pub id: i64,
    /// 温度参数
    pub temperature: Option<f64>,
    /// 最大输出 token 数
    pub max_tokens: Option<u64>,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新智能体 LLM 模型绑定启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateLlmModelRelEnabledReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}
