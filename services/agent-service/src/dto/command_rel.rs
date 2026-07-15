//! 智能体命令绑定关系管理

use entity::agent::command_rel::CommandRelWithCommand;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询绑定列表（含命令详情） ─────────────────────────────────────────────

/// 查询智能体命令绑定列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetCommandRelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 智能体 ID（必填）
    pub agent_id: i64,
    /// 绑定启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询智能体命令绑定列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetCommandRelsResp {
    pub data_list: Vec<CommandRelWithCommand>,
    pub total: u64,
}

// ─── 批量同步绑定 ───────────────────────────────────────────────────────────

/// 批量同步智能体命令绑定 请求体
///
/// 传入 agent_id 和目标 command_ids 列表，系统自动计算差异：
/// - 新增：目标中有但当前没有的绑定
/// - 删除：当前有但目标中没有的绑定
/// - 保持：交集部分不变
#[derive(Debug, Deserialize, Validate)]
pub struct BatchSaveCommandRelsReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// 目标命令 ID 列表
    pub command_ids: Vec<i64>,
}

/// 批量同步智能体命令绑定 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchSaveCommandRelsResp {
    /// 新增的绑定数量
    pub added: usize,
    /// 删除的绑定数量
    pub removed: usize,
}

// ─── 更新绑定 ───────────────────────────────────────────────────────────────

/// 更新智能体命令绑定 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCommandRelReq {
    /// 关系 ID
    pub id: i64,
    /// 排序优先级覆盖
    pub priority: i32,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新智能体命令绑定启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCommandRelEnabledReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}
