//! 技能安装包管理 DTO

use serde::{Deserialize, Serialize};
use validator::Validate;

use entity::agent::skill_package;

use crate::enums::{SkillPackageStatus, SkillSourceType};

// ─── 查询安装包列表 ───────────────────────────────────────────────────────────

/// 查询技能安装包列表 请求体
#[derive(Default, Deserialize, Validate)]
pub struct GetSkillPackagesReq {
    /// 当前分页
    pub page: u64,
    /// 页面大小
    pub page_size: u64,
    /// 技能编码模糊搜索
    pub skill_code: Option<String>,
    /// 名称模糊搜索
    pub name: Option<String>,
    /// 状态过滤
    pub status: Option<SkillPackageStatus>,
}

/// 查询技能安装包列表 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetSkillPackagesResp {
    pub data_list: Vec<skill_package::Model>,
    pub total: u64,
}

// ─── 查询安装包详情 ───────────────────────────────────────────────────────────

/// 查询技能安装包详情 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct GetSkillPackageReq {
    /// 安装包 ID
    pub id: i64,
}

/// 查询技能安装包详情 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct GetSkillPackageResp {
    #[serde(flatten)]
    pub model: skill_package::Model,
}

// ─── 创建安装包 ───────────────────────────────────────────────────────────────

/// 创建技能安装包 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct CreateSkillPackageReq {
    /// 技能唯一编码
    #[validate(length(min = 1, max = 128, message = "技能编码长度须在 1~128 字符之间"))]
    pub skill_code: String,
    /// 技能展示名
    #[validate(length(min = 1, max = 128, message = "名称长度须在 1~128 字符之间"))]
    pub name: String,
    /// 技能描述
    #[validate(length(min = 1, message = "描述不能为空"))]
    pub description: String,
    /// 版本号
    #[validate(length(min = 1, max = 64, message = "版本号长度须在 1~64 字符之间"))]
    pub version: String,
    /// 来源类型
    pub source_type: SkillSourceType,
    /// 原始安装来源标识
    pub source_url: String,
}

/// 创建技能安装包 响应体
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSkillPackageResp {
    /// 新建的安装包 ID
    pub id: i64,
}

// ─── 更新安装包状态 ───────────────────────────────────────────────────────────

/// 更新技能安装包状态 请求体
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateSkillPackageStatusReq {
    /// 安装包 ID
    pub id: i64,
    /// 目标状态
    pub status: SkillPackageStatus,
}

// ─── 删除安装包 ───────────────────────────────────────────────────────────────

/// 删除技能安装包 请求体
#[derive(Debug, Default, Deserialize, Validate)]
pub struct DeleteSkillPackageReq {
    /// 安装包 ID
    pub id: i64,
}
