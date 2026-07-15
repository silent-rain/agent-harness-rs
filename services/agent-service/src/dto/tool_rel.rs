//! 智能体工具绑定关系管理

use entity::agent::tool_rel;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询绑定列表 ─────────────────────────────────────────────────────────────

/// 查询智能体工具绑定列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetToolRelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 智能体 ID（必填）
    pub agent_id: i64,
    /// 绑定启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询智能体工具绑定列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetToolRelsResp {
    pub data_list: Vec<tool_rel::Model>,
    pub total: u64,
}

// ─── 批量同步绑定 ───────────────────────────────────────────────────────────

/// 批量同步智能体工具绑定 请求体
///
/// 传入 agent_id 和目标 tool_ids 列表，系统自动计算差异：
/// - 新增：目标中有但当前没有的绑定
/// - 删除：当前有但目标中没有的绑定
/// - 保持：交集部分不变
#[derive(Debug, Deserialize, Validate)]
pub struct BatchSaveToolRelsReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// 目标工具 ID 列表
    pub tool_ids: Vec<i64>,
}

/// 批量同步智能体工具绑定 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchSaveToolRelsResp {
    /// 新增的绑定数量
    pub added: usize,
    /// 删除的绑定数量
    pub removed: usize,
}

// ─── 更新绑定 ───────────────────────────────────────────────────────────────

/// 更新智能体工具绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateToolRelReq {
    /// 关系 ID
    pub id: i64,
    /// 优先级（数值越小越优先）
    pub priority: i32,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新智能体工具绑定启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateToolRelEnabledReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}
