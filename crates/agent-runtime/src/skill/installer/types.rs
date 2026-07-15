//! Skill 安装包数据契约
//!
//! 定义安装通道与持久化层之间共享的领域模型：
//! - [`SkillPackage`]：完整的物理包（含 zip 字节流，写入数据库）
//! - [`SkillPackageMeta`]：轻量元数据（不含 zip 字节，用于列表查询）
//! - [`SkillPackageStatus`]：包状态枚举（消除魔数）

use serde::{Deserialize, Serialize};

/// Skill 安装包状态
///
/// 与 `entity::agent::skill_package::Model::status` 对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// 完整的 Skill 安装包（含 zip 字节流）
///
/// 用于安装写入与按 `skill_code` 加载，体积可能较大。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackage {
    /// 技能唯一编码
    pub skill_code: String,
    /// 技能展示名
    pub name: String,
    /// 技能描述
    pub description: String,
    /// 版本号
    pub version: String,
    /// 来源类型（与 `SkillSourceType` 对齐）
    pub source_type: i16,
    /// 原始安装来源标识
    pub source_url: String,
    /// 包的 zip 二进制内容
    #[serde(skip)]
    pub content_zip: Vec<u8>,
    /// zip 内容的 sha256（hex）
    pub content_sha256: String,
    /// zip 字节大小
    pub size_bytes: i64,
    /// 安装者用户 ID（0 表示系统）
    pub installed_by: i64,
    /// 状态
    pub status: i16,
}

/// Skill 安装包元数据（不含 zip 字节）
///
/// 用于列表查询、状态展示等不需要包体的场景，避免读取大体积 BLOB。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackageMeta {
    /// 技能唯一编码
    pub skill_code: String,
    /// 技能展示名
    pub name: String,
    /// 技能描述
    pub description: String,
    /// 版本号
    pub version: String,
    /// 来源类型
    pub source_type: i16,
    /// 原始安装来源标识
    pub source_url: String,
    /// zip 内容的 sha256（hex）
    pub content_sha256: String,
    /// zip 字节大小
    pub size_bytes: i64,
    /// 状态
    pub status: i16,
}

impl From<&SkillPackage> for SkillPackageMeta {
    fn from(pkg: &SkillPackage) -> Self {
        Self {
            skill_code: pkg.skill_code.clone(),
            name: pkg.name.clone(),
            description: pkg.description.clone(),
            version: pkg.version.clone(),
            source_type: pkg.source_type,
            source_url: pkg.source_url.clone(),
            content_sha256: pkg.content_sha256.clone(),
            size_bytes: pkg.size_bytes,
            status: pkg.status,
        }
    }
}

/// 已注册的 Skill 包记录（数据库行的子集，含主键）
///
/// 用于线上 [`crate::skill::installer::SkillPackageStore::find_package`]
/// 返回，调用方据此判断是否已注册并复用主键。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackageRecord {
    /// 主键（`t_agent_skill_package.id`）
    pub id: i64,
    /// 所属用户 ID（0 = 系统级）
    pub user_id: i64,
    /// 元数据
    pub meta: SkillPackageMeta,
}

/// Agent 与 Skill 的绑定关系记录
///
/// 用于线上 [`crate::skill::installer::SkillPackageStore::find_rel`] /
/// `list_rel` 返回，作为"是否已绑定"的依据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRelRecord {
    /// 主键（`t_agent_skill_rel.id`）
    pub id: i64,
    /// 所属用户 ID
    pub user_id: i64,
    /// 智能体 ID
    pub agent_id: i64,
    /// 关联的安装包 ID
    pub skill_package_id: i64,
    /// 该绑定对应的 skill_code（冗余字段，便于上层判断）
    pub skill_code: String,
    /// 是否启用
    pub enabled: bool,
}

/// 注册与关联状态查询结果
///
/// 由 [`crate::skill::installer::SkillPackageStore::check`] 返回，
/// 合并"包是否已注册"与"当前 Agent 是否已关联"两个维度的信息。
#[derive(Debug, Clone)]
pub struct SkillCheckResult {
    /// 包是否已注册（`t_agent_skill_package` 中存在）
    pub package: Option<SkillPackageRecord>,
    /// 当前 Agent 是否已关联（`t_agent_skill_rel` 中存在）
    pub rel: Option<SkillRelRecord>,
}

impl SkillCheckResult {
    /// 包是否已注册
    pub fn is_registered(&self) -> bool {
        self.package.is_some()
    }

    /// 当前 Agent 是否已关联
    pub fn is_linked(&self) -> bool {
        self.rel.is_some()
    }
}

/// 安装/更新操作的结果语义
///
/// 由 [`crate::skill::installer::DbInstaller::install`] /
/// [`crate::skill::installer::DbInstaller::update`] 返回，
/// 便于调用方区分"新装"、"已存在跳过"、"覆盖更新"三种情况。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum InstallOutcome {
    /// 首次安装：包注册 + 关系绑定均为新建
    Installed(SkillPackageMeta),
    /// 已存在绑定：跳过任何写入
    Skipped(SkillPackageMeta),
    /// 覆盖更新：包内容被替换为新的 zip
    Updated(SkillPackageMeta),
}

impl InstallOutcome {
    /// 提取内部的元数据（无论何种结果）
    pub fn meta(&self) -> &SkillPackageMeta {
        match self {
            Self::Installed(m) | Self::Skipped(m) | Self::Updated(m) => m,
        }
    }

    /// 操作是否实际写入了数据库
    pub fn is_mutated(&self) -> bool {
        !matches!(self, Self::Skipped(_))
    }
}
