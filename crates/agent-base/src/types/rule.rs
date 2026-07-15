//! Rule types
use serde::{Deserialize, Serialize};

/// 规则来源
///
/// 描述规则内容的加载来源，用于溯源和调试。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    /// 从本地文件系统加载
    Local,
    /// 在运行时动态注入（如数据库、HTTP 等外部来源）
    Injected,
}

/// 规则配置
///
/// 承载从本地文件或远程数据库加载的规则内容。
/// 运行时将所有规则内容按顺序拼接注入到 system prompt 中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    /// 规则名称（本地文件名不含扩展名，或数据库 name 字段）
    pub name: String,
    /// 规则描述（本地文件加载时可为空）
    pub description: String,
    /// 规则纯文本正文内容
    pub content: String,
    /// 规则来源
    pub source: RuleSource,
    /// 来源提示（文件路径或数据库 ID）
    pub source_hint: Option<String>,
}
