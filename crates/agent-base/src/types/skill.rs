//! Skill types
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// 技能配置
///
/// 从本地 skill 目录包的 `SKILL.md` 解析而来。
/// `skill_code` 对应目录名，`content` 为 `SKILL.md` 正文。
/// 运行时将技能内容注入到 system prompt 中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConfig {
    /// 技能唯一编码（= 本地目录名）
    pub skill_code: String,
    /// 技能展示名（来自 SKILL.md frontmatter `name`）
    pub name: String,
    /// 技能描述（来自 SKILL.md frontmatter `description`）
    pub description: String,
    /// 来源提示（文件路径或数据库标识）
    pub source_hint: Option<String>,
}

/// 技能白名单策略
///
/// 控制扫描结果中哪些技能进入最终配置列表，类似 CORS 的 Origin 策略：
/// - `AllowAll`：全部放行（类似 `Access-Control-Allow-Origin: *`）
/// - `Specific(set)`：仅放行列表中的技能（类似显式 Origin 列表）
#[derive(Debug, Clone, Default)]
pub enum SkillWhitelistPolicy {
    /// No filtering — all scanned skills are included.
    /// This is the default policy, suitable for development/testing.
    #[default]
    AllowAll,
    /// Only skills whose `skill_code` is in the set are included.
    /// Empty set means NO skills will be included.
    Specific(HashSet<String>),
}
