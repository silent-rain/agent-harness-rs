//! 技能安装包相关枚举

use serde::Deserialize;

/// 技能安装包状态
///
/// 对应数据库 `t_agent_skill_package.status` 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[repr(i16)]
pub enum SkillPackageStatus {
    /// 已安装，可被 Registry 加载
    Installed = 0,
    /// 已禁用（保留 zip 但不参与加载）
    Disabled = 1,
    /// 损坏（解压/校验失败）
    Broken = 2,
}

impl From<SkillPackageStatus> for i16 {
    fn from(s: SkillPackageStatus) -> Self {
        s as i16
    }
}

impl TryFrom<i16> for SkillPackageStatus {
    type Error = i16;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Installed),
            1 => Ok(Self::Disabled),
            2 => Ok(Self::Broken),
            other => Err(other),
        }
    }
}

/// 技能来源类型
///
/// 对应数据库 `t_agent_skill_rel.source_type` 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[repr(i16)]
pub enum SkillSourceType {
    /// 内置（随发行版分发）
    Builtin = 0,
    /// 智能体自动安装（通过指令从远端安装）
    AgentInstalled = 1,
    /// 用户手动放入 skills 目录
    UserManual = 2,
}

impl From<SkillSourceType> for i16 {
    fn from(s: SkillSourceType) -> Self {
        s as i16
    }
}

impl TryFrom<i16> for SkillSourceType {
    type Error = i16;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Builtin),
            1 => Ok(Self::AgentInstalled),
            2 => Ok(Self::UserManual),
            other => Err(other),
        }
    }
}
