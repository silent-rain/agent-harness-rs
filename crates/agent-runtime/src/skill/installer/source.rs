//! Skill 安装来源
//!
//! 定义内置安装通道支持的来源协议（[`SkillSource`]），
//! 并提供从字符串识别协议的 [`SkillSource::parse`] 入口。

use std::path::PathBuf;

use crate::Error;

/// Skill 安装来源
///
/// 当前内置安装模式仅支持以下"零网络/低风险"的来源类型，
/// CLI/远端协议（git、http zip、npm 等）后续在外部 fetcher 中按需扩展。
#[derive(Debug, Clone)]
pub enum SkillSource {
    /// 从本地目录打包安装
    ///
    /// 适用于开发者将本地 skill 目录注册到 DB 中作为基线，
    /// 也是单元测试与受信任启动安装路径的首选。
    LocalDir(PathBuf),

    /// 直接传入 zip 字节流
    ///
    /// 由调用方（CLI / 上层 Service / 远端 fetcher）完成下载后调用，
    /// 安装器仅负责校验、计算 sha256 并写入 store。
    InlineZip {
        /// 原始来源标识（用于审计与展示）
        origin: String,
        /// zip 字节流
        bytes: Vec<u8>,
    },
}

impl SkillSource {
    /// 从字符串识别安装来源协议
    ///
    /// 当前支持：
    /// - `file://<path>` 或不含 scheme 的本地路径 → [`SkillSource::LocalDir`]
    ///
    /// 其他协议（`http(s)://`、`git+...`、`npm:...`）在内部安装模式中
    /// 暂不支持，由调用方先行下载为字节流后通过 [`SkillSource::InlineZip`] 传入。
    pub fn parse(source: &str) -> Result<Self, Error> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(Error::Internal("skill source is empty".to_string()));
        }
        if let Some(rest) = trimmed.strip_prefix("file://") {
            return Ok(Self::LocalDir(PathBuf::from(rest)));
        }
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("git+")
            || trimmed.starts_with("npm:")
        {
            return Err(Error::Internal(format!(
                "remote skill source is not supported by internal installer: {trimmed}"
            )));
        }
        // Fallback: treat as local path
        Ok(Self::LocalDir(PathBuf::from(trimmed)))
    }

    /// 获取来源的展示字符串（用于 `source_url` 字段写入数据库）
    pub fn display_origin(&self) -> String {
        match self {
            Self::LocalDir(path) => format!("file://{}", path.display()),
            Self::InlineZip { origin, .. } => origin.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_scheme() {
        let src = SkillSource::parse("file:///tmp/foo").unwrap();
        assert!(matches!(src, SkillSource::LocalDir(_)));
        assert_eq!(src.display_origin(), "file:///tmp/foo");
    }

    #[test]
    fn test_parse_bare_path() {
        let src = SkillSource::parse("./local-skill").unwrap();
        assert!(matches!(src, SkillSource::LocalDir(_)));
    }

    #[test]
    fn test_parse_rejects_remote() {
        assert!(SkillSource::parse("https://example.com/x.zip").is_err());
        assert!(SkillSource::parse("npm:foo").is_err());
        assert!(SkillSource::parse("").is_err());
    }
}
