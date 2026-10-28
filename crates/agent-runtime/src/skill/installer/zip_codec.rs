//! Skill 包 zip 编解码工具
//!
//! 提供 zip 打包/解包与 sha256 计算、zip-slip 防御等基础设施。
//! 所有 IO 密集操作通过 [`tokio::task::spawn_blocking`] 卸载到阻塞线程池，
//! 避免阻塞异步运行时。

use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::task;
use tracing::{debug, warn};

use crate::{Error, constant::SKILL_ENTRY_FILE};

/// 单个 zip 内文件大小上限（200 MiB），防御 zip-bomb
const MAX_SINGLE_FILE_BYTES: u64 = 200 * 1024 * 1024;
/// zip 内最大文件数量，防御 zip-bomb
const MAX_FILES_IN_ZIP: usize = 5_000;

/// 计算字节切片的 sha256 hex 值
///
/// # Parameters
///
/// * `bytes` - 待计算的字节切片
///
/// # Returns
///
/// 64 字符的小写 hex 字符串
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// 取 sha256 的前缀，用于本地缓存目录命名
pub fn sha256_short(sha256_full: &str) -> &str {
    let len = sha256_full.len().min(12);
    &sha256_full[..len]
}

/// 将本地目录打包为 zip 字节流
///
/// 仅打包 `dir` 下的内容（不包含 `dir` 本身），递归遍历所有文件。
/// 适用场景：从本地目录或 git clone 后的临时目录构造安装包。
///
/// # Parameters
///
/// * `dir` - 要打包的源目录
///
/// # Errors
///
/// 当目录不存在、无法读取或 zip 写入失败时返回错误
/// 将指定目录递归打包为 zip 字节。
///
/// # Parameters
///
/// * `dir` - 要打包的目录
/// * `top_name` - 可选顶层目录名；为 `Some(name)` 时，所有条目路径前追加 `<name>/`
///
/// # Returns
///
/// 序列化后的 zip 字节流
pub async fn pack_dir_to_zip(dir: &Path, top_name: Option<&str>) -> Result<Vec<u8>, Error> {
    if !dir.exists() {
        return Err(Error::Internal(format!(
            "skill source directory does not exist: {}",
            dir.display()
        )));
    }
    if !dir.is_dir() {
        return Err(Error::Internal(format!(
            "skill source path is not a directory: {}",
            dir.display()
        )));
    }

    let dir = dir.to_path_buf();
    let top = top_name.map(|s| s.to_string());
    task::spawn_blocking(move || pack_dir_blocking(&dir, top.as_deref()))
        .await
        .map_err(|e| Error::Internal(format!("pack join error: {e}")))?
}

fn pack_dir_blocking(dir: &Path, top_name: Option<&str>) -> Result<Vec<u8>, Error> {
    let buf: Vec<u8> = Vec::new();
    let cursor = Cursor::new(buf);
    let mut writer = zip::ZipWriter::new(cursor);
    let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let prefix = top_name
        .filter(|s| !s.is_empty())
        .map(|s| format!("{s}/"))
        .unwrap_or_default();

    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current).map_err(|e| {
            Error::Internal(format!("read_dir failed: {} ({e})", current.display()))
        })?;
        for entry in entries {
            let entry =
                entry.map_err(|e| Error::Internal(format!("read_dir entry failed: {e}")))?;
            let path = entry.path();
            let rel = path.strip_prefix(dir).map_err(|e| {
                Error::Internal(format!("strip_prefix failed: {} ({e})", path.display()))
            })?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let zip_path = format!("{prefix}{rel_str}");

            if path.is_dir() {
                if !rel_str.is_empty() {
                    writer
                        .add_directory(format!("{zip_path}/"), options)
                        .map_err(|e| Error::Internal(format!("zip add_directory failed: {e}")))?;
                }
                stack.push(path);
            } else if path.is_file() {
                writer
                    .start_file(zip_path.clone(), options)
                    .map_err(|e| Error::Internal(format!("zip start_file failed: {e}")))?;
                let mut f = std::fs::File::open(&path).map_err(|e| {
                    Error::Internal(format!("open file failed: {} ({e})", path.display()))
                })?;
                let mut bytes = Vec::new();
                f.read_to_end(&mut bytes).map_err(|e| {
                    Error::Internal(format!("read file failed: {} ({e})", path.display()))
                })?;
                writer
                    .write_all(&bytes)
                    .map_err(|e| Error::Internal(format!("zip write_all failed: {e}")))?;
            }
        }
    }

    let cursor = writer
        .finish()
        .map_err(|e| Error::Internal(format!("zip finish failed: {e}")))?;
    Ok(cursor.into_inner())
}

/// 校验 zip 字节流是否构成一个合法的 Skill 包
///
/// 验证规则：
/// - 至少包含一个 `SKILL.md`（在根或第一层子目录中）
/// - 单文件大小不超过 [`MAX_SINGLE_FILE_BYTES`]
/// - 文件总数不超过 [`MAX_FILES_IN_ZIP`]
/// - 不含 zip-slip 路径（绝对路径或 `..` 跳出）
///
/// # Returns
///
/// 校验通过时返回 zip 内 `SKILL.md` 的相对路径（用于推断顶层目录）
pub fn validate_skill_zip(bytes: &[u8]) -> Result<String, Error> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| Error::Internal(format!("invalid zip archive: {e}")))?;

    if archive.len() > MAX_FILES_IN_ZIP {
        return Err(Error::Internal(format!(
            "zip contains too many files: {} > {}",
            archive.len(),
            MAX_FILES_IN_ZIP
        )));
    }

    let mut skill_md_path: Option<String> = None;
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| Error::Internal(format!("zip read entry {i} failed: {e}")))?;
        let name = file
            .name()
            .map_err(|e| Error::Internal(format!("zip entry {i} name read failed: {e}")))?
            .to_string();

        // zip-slip defense
        let path = Path::new(&name);
        if path.is_absolute() {
            return Err(Error::Internal(format!(
                "zip contains absolute path: {name}"
            )));
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(Error::Internal(format!(
                "zip contains parent dir component: {name}"
            )));
        }

        if file.size() > MAX_SINGLE_FILE_BYTES {
            return Err(Error::Internal(format!(
                "zip entry too large: {name} ({} bytes)",
                file.size()
            )));
        }

        // Find SKILL.md (root or one-level deep)
        if skill_md_path.is_none() {
            let normalized = name.replace('\\', "/");
            let depth = normalized.matches('/').count();
            if normalized.ends_with(SKILL_ENTRY_FILE) && depth <= 1 {
                skill_md_path = Some(normalized);
            }
        }
    }

    skill_md_path.ok_or_else(|| {
        Error::Internal(format!(
            "zip does not contain {SKILL_ENTRY_FILE} at root or one-level deep"
        ))
    })
}

/// 从 zip 字节流中读取 `SKILL.md` 的内容文本
///
/// # Parameters
///
/// * `bytes` - zip 字节流
/// * `skill_md_path` - 由 [`validate_skill_zip`] 返回的 SKILL.md 路径
pub fn read_skill_md_from_zip(bytes: &[u8], skill_md_path: &str) -> Result<String, Error> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| Error::Internal(format!("invalid zip archive: {e}")))?;
    let mut file = archive
        .by_name(skill_md_path)
        .map_err(|e| Error::Internal(format!("zip entry {skill_md_path} not found: {e}")))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| Error::Internal(format!("read SKILL.md failed: {e}")))?;
    Ok(content)
}

/// 将 zip 字节流惰性解压到目标目录
///
/// 若 `dest` 已存在且非空，则直接返回（认为已解压完成）。
/// 解压时严防 zip-slip：所有路径必须落在 `dest` 内。
///
/// # Parameters
///
/// * `bytes` - zip 字节流
/// * `dest` - 解压目标目录
/// * `strip_top_level` - 若为 `true`，则去掉 zip 顶层目录（适用于 git clone 打包的 `repo-name/...`）
pub async fn unpack_zip_to_dir(
    bytes: Vec<u8>,
    dest: PathBuf,
    strip_top_level: bool,
) -> Result<(), Error> {
    if dir_is_non_empty(&dest).await {
        debug!(dest = %dest.display(), "skill cache dir already populated, skipping unpack");
        return Ok(());
    }

    fs::create_dir_all(&dest)
        .await
        .map_err(|e| Error::Internal(format!("create_dir_all failed: {} ({e})", dest.display())))?;

    task::spawn_blocking(move || unpack_zip_blocking(&bytes, &dest, strip_top_level))
        .await
        .map_err(|e| Error::Internal(format!("unpack join error: {e}")))?
}

fn unpack_zip_blocking(bytes: &[u8], dest: &Path, strip_top_level: bool) -> Result<(), Error> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| Error::Internal(format!("invalid zip archive: {e}")))?;

    // Detect single top-level directory if requested
    let top_level: Option<String> = if strip_top_level {
        detect_top_level(&mut archive)
    } else {
        None
    };

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| Error::Internal(format!("zip read entry {i} failed: {e}")))?;
        let raw_name = file
            .name()
            .map_err(|e| Error::Internal(format!("zip entry {i} name read failed: {e}")))?
            .replace('\\', "/");

        // Skip top-level dir prefix if applicable
        let stripped = match &top_level {
            Some(prefix) => match raw_name.strip_prefix(prefix.as_str()) {
                Some(rest) => rest.to_string(),
                None => raw_name.clone(),
            },
            None => raw_name.clone(),
        };
        if stripped.is_empty() || stripped == "/" {
            continue;
        }

        let rel = Path::new(&stripped);
        if rel.is_absolute() || rel.components().any(|c| matches!(c, Component::ParentDir)) {
            warn!(name = %raw_name, "skipping unsafe zip entry");
            continue;
        }

        let out_path = dest.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| {
                Error::Internal(format!(
                    "create_dir_all failed: {} ({e})",
                    out_path.display()
                ))
            })?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    Error::Internal(format!(
                        "create parent dir failed: {} ({e})",
                        parent.display()
                    ))
                })?;
            }
            let mut out = std::fs::File::create(&out_path).map_err(|e| {
                Error::Internal(format!("create file failed: {} ({e})", out_path.display()))
            })?;
            std::io::copy(&mut file, &mut out).map_err(|e| {
                Error::Internal(format!("copy file failed: {} ({e})", out_path.display()))
            })?;
        }
    }
    Ok(())
}

/// 检测 zip 中所有条目是否共享同一个顶层目录前缀（用于 strip_top_level）
fn detect_top_level<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Option<String> {
    let mut prefix: Option<String> = None;
    for i in 0..archive.len() {
        let Ok(file) = archive.by_index(i) else {
            return None;
        };
        let Ok(name) = file.name() else {
            return None;
        };
        let name = name.replace('\\', "/");
        if name.is_empty() {
            continue;
        }
        let top = match name.split_once('/') {
            Some((head, _)) => format!("{head}/"),
            None => return None, // file at root → no shared prefix
        };
        match &prefix {
            Some(p) if *p != top => return None,
            None => prefix = Some(top),
            _ => {}
        }
    }
    prefix
}

/// 判断目录是否存在且非空
async fn dir_is_non_empty(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }
    let Ok(mut entries) = fs::read_dir(dir).await else {
        return false;
    };
    entries.next_entry().await.ok().flatten().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_pack_validate_unpack_round_trip() {
        let src = TempDir::new().unwrap();
        std::fs::write(src.path().join("SKILL.md"), "---\nname: round\n---\n\nbody").unwrap();
        std::fs::create_dir_all(src.path().join("references")).unwrap();
        std::fs::write(src.path().join("references/g.md"), "guide").unwrap();

        let zip_bytes = pack_dir_to_zip(src.path(), None).await.unwrap();
        let entry = validate_skill_zip(&zip_bytes).unwrap();
        assert!(entry.ends_with("SKILL.md"));

        let dest = TempDir::new().unwrap();
        unpack_zip_to_dir(zip_bytes, dest.path().to_path_buf(), false)
            .await
            .unwrap();
        assert!(dest.path().join("SKILL.md").exists());
        assert!(dest.path().join("references/g.md").exists());
    }

    #[test]
    fn test_sha256_short_handles_empty() {
        assert_eq!(sha256_short(""), "");
        assert_eq!(sha256_short("abcdef"), "abcdef");
        assert_eq!(sha256_short("0123456789abcdef"), "0123456789ab");
    }

    #[test]
    fn test_validate_rejects_zip_without_skill_md() {
        let buf: Vec<u8> = Vec::new();
        let mut writer = zip::ZipWriter::new(Cursor::new(buf));
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        writer.start_file("README.md", opts).unwrap();
        writer.write_all(b"hello").unwrap();
        let bytes = writer.finish().unwrap().into_inner();

        let err = validate_skill_zip(&bytes).unwrap_err();
        assert!(matches!(err, Error::Internal(_)));
    }
}
