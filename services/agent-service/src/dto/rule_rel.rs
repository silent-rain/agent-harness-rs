//! 智能体规则绑定关系管理

use entity::agent::rule_rel::RuleRelWithRule;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询绑定列表（含规则详情） ─────────────────────────────────────────────

/// 查询智能体规则绑定列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetRuleRelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 智能体 ID（必填）
    pub agent_id: i64,
    /// 绑定启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询智能体规则绑定列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetRuleRelsResp {
    pub data_list: Vec<RuleRelWithRule>,
    pub total: u64,
}

// ─── 批量同步绑定 ───────────────────────────────────────────────────────────

/// 批量同步智能体规则绑定 请求体
///
/// 传入 agent_id 和目标 rule_ids 列表，系统自动计算差异：
/// - 新增：目标中有但当前没有的绑定
/// - 删除：当前有但目标中没有的绑定
/// - 保持：交集部分不变
#[derive(Debug, Deserialize, Validate)]
pub struct BatchSaveRuleRelsReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// 目标规则 ID 列表
    pub rule_ids: Vec<i64>,
}

/// 批量同步智能体规则绑定 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchSaveRuleRelsResp {
    /// 新增的绑定数量
    pub added: usize,
    /// 删除的绑定数量
    pub removed: usize,
}

// ─── 更新绑定 ───────────────────────────────────────────────────────────────

/// 更新智能体规则绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateRuleRelReq {
    /// 关系 ID
    pub id: i64,
    /// 注入优先级覆盖
    pub priority: i32,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新智能体规则绑定启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateRuleRelEnabledReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}
