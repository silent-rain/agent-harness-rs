//! 智能体技能绑定关系管理

use entity::agent::skill_rel;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ─── 查询绑定列表 ─────────────────────────────────────────────────────────────

/// 查询智能体技能绑定列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetSkillRelsReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 智能体 ID（必填）
    pub agent_id: i64,
    /// 绑定启用状态过滤
    pub enabled: Option<bool>,
}

/// 查询智能体技能绑定列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetSkillRelsResp {
    pub data_list: Vec<skill_rel::Model>,
    pub total: u64,
}

// ─── 查询绑定详情 ─────────────────────────────────────────────────────────────

/// 查询智能体技能绑定详情 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct GetSkillRelReq {
    /// 关系 ID
    pub id: i64,
}

/// 查询智能体技能绑定详情 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetSkillRelResp {
    #[serde(flatten)]
    pub model: skill_rel::Model,
}

// ─── 创建绑定 ─────────────────────────────────────────────────────────────────

/// 创建智能体技能绑定 请求体
///
/// 通过 `skill_package_id` 关联 `t_agent_skill_package`，
/// 元数据（name/description/skill_code）从包表获取，无需重复传入。
#[derive(Deserialize, Validate)]
pub struct CreateSkillRelReq {
    /// 智能体 ID
    pub agent_id: i64,
    /// 关联的技能安装包 ID（外键 → t_agent_skill_package.id）
    pub skill_package_id: i64,
    /// 是否启用
    pub enabled: bool,
}

/// 创建智能体技能绑定 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSkillRelResp {
    /// 新建记录 ID
    pub id: i64,
}

// ─── 更新绑定 ─────────────────────────────────────────────────────────────────

/// 更新智能体技能绑定 请求体
#[derive(Deserialize, Validate)]
pub struct UpdateSkillRelReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 更新启用状态 ─────────────────────────────────────────────────────────────

/// 更新智能体技能绑定启用状态 请求体
#[derive(Deserialize, Validate)]
pub struct UpdateSkillRelEnabledReq {
    /// 关系 ID
    pub id: i64,
    /// 是否启用
    pub enabled: bool,
}

// ─── 删除绑定 ─────────────────────────────────────────────────────────────────

/// 删除智能体技能绑定 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteSkillRelReq {
    /// 关系 ID
    pub id: i64,
}
