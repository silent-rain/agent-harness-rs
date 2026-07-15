//! 智能体 A2A 服务绑定关系管理

use entity::agent::a2a_service_rel::A2aServiceRelWithRegistry;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询绑定列表 ─────────────────────────────────────────────────────────────

/// 查询智能体 A2A 服务绑定列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetA2aServiceRelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 智能体 ID（必填）
    pub agent_id: i64,
    /// 绑定启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询智能体 A2A 服务绑定列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetA2aServiceRelsResp {
    pub data_list: Vec<A2aServiceRelWithRegistry>,
    pub total: u64,
}

// ─── 批量同步绑定 ─────────────────────────────────────────────────────────────

/// 批量同步智能体 A2A 服务绑定 请求体
///
/// 传入 agent_id 和目标 registry_ids 列表，系统自动计算差异：
/// - 新增：目标中有但当前没有的绑定
/// - 删除：当前有但目标中没有的绑定
/// - 保持：交集部分不变
#[derive(Debug, Deserialize, Validate)]
pub struct BatchSaveA2aServiceRelsReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// 目标 A2A 服务注册表 ID 列表
    pub registry_ids: Vec<i64>,
}

/// 批量同步智能体 A2A 服务绑定 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchSaveA2aServiceRelsResp {
    /// 新增的绑定数量
    pub added: usize,
    /// 删除的绑定数量
    pub removed: usize,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新智能体 A2A 服务绑定启用状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateA2aServiceRelEnabledReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}
