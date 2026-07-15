//! 本地命令加载器
//!
//! 从本地文件系统扫描命令文件，将每个文件转换为 [`CommandConfig`]。
//!
//! 提供的主要内容：
//! - [`LocalCommandLoader`] - 本地文件系统命令加载器

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tokio::fs;
use tracing::{info, warn};

use crate::{
    Error,
    constant::{COMMAND_ENTRY_EXTENSIONS, DEFAULT_COMMANDS_DIR, DEFAULT_WORKSPACE_DIR},
    types::command::{CommandConfig, CommandSource},
};

/// 本地命令加载器
///
/// 扫描指定目录下匹配扩展名的文件（非递归），将每个文件加载为一条命令。
/// 文件名（不含扩展名）作为命令的 `name`，文件完整内容作为命令正文。
///
/// 设计上保持"被动"：仅在 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
/// 加载结果由调用方通过
/// [`AgentBuilder::add_commands`](crate::runtime::AgentBuilder::add_commands) 注入到 bundle 中统一合并。
///
/// ## 使用示例
///
/// ```rust,no_run
/// use agent_runtime::command::LocalCommandLoader;
///
/// # async fn example() -> anyhow::Result<()> {
/// let loader = LocalCommandLoader::new(".codebuddy/commands");
/// let commands = loader.load().await?;
///
/// // 通过 AgentBuilder::add_commands 注入到 bundle
/// // builder.add_commands(commands);
/// # Ok(())
/// # }
/// ```
pub struct LocalCommandLoader {
    /// Commands 根目录路径
    commands_root: PathBuf,
    /// 支持的文件扩展名集合（不含前缀点，如 `"md"`）
    extensions: HashSet<String>,
}

impl Default for LocalCommandLoader {
    fn default() -> Self {
        let extensions = COMMAND_ENTRY_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            commands_root: Path::new(DEFAULT_WORKSPACE_DIR).join(DEFAULT_COMMANDS_DIR),
            extensions,
        }
    }
}

impl LocalCommandLoader {
    /// 创建本地命令加载器
    ///
    /// # Parameters
    ///
    /// * `commands_root` - commands 根目录路径（如 `.codebuddy/commands`）
    pub fn new(commands_root: impl Into<PathBuf>) -> Self {
        Self {
            commands_root: commands_root.into(),
            ..Default::default()
        }
    }

    /// 设置支持的文件解析格式列表
    ///
    /// 替换默认的扩展名集合（`.md`）。
    /// 扩展名不含前缀点，例如 `"md"`、`"txt"`。
    ///
    /// # Parameters
    ///
    /// * `extensions` - 文件扩展名列表
    ///
    /// # Returns
    ///
    /// 返回 `self` 以支持链式调用
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = extensions.into_iter().collect();
        self
    }

    /// 切换工作空间根目录
    ///
    /// 将 `commands_root` 重新设置为 `workspace_dir/commands`。
    ///
    /// # Parameters
    ///
    /// * `workspace_dir` - 工作空间根目录（如 `.codebuddy`）
    ///
    /// # Returns
    ///
    /// 返回 `self` 以支持链式调用
    pub fn with_workspace_dir(mut self, workspace_dir: impl Into<PathBuf>) -> Self {
        self.commands_root = workspace_dir.into().join(DEFAULT_COMMANDS_DIR);
        self
    }

    /// 获取 commands 根目录
    pub fn commands_root(&self) -> &Path {
        &self.commands_root
    }

    /// 获取当前支持的文件扩展名集合
    pub fn extensions(&self) -> &HashSet<String> {
        &self.extensions
    }

    /// 扫描本地 commands 根目录并加载所有命令
    ///
    /// 遍历 `commands_root` 下匹配扩展名的文件（非递归），将每个文件转换为 [`CommandConfig`]。
    /// 单个文件读取失败不会中断整体加载，仅记录 `warn` 级别日志后跳过。
    ///
    /// 当 `commands_root` 不存在时返回空列表（视作"无本地命令"，不视为错误）。
    ///
    /// # Returns
    ///
    /// 加载成功的命令列表，按文件名字典序排列以保证确定性
    ///
    /// # Errors
    ///
    /// 当 `commands_root` 存在但读取目录失败时返回 IO 错误
    pub async fn load(&self) -> Result<Vec<CommandConfig>, Error> {
        if !self.commands_root.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&self.commands_root).await?;
        let mut command_files: Vec<PathBuf> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && self.is_command_file(&path) {
                command_files.push(path);
            }
        }

        // Sort for deterministic order
        command_files.sort();

        let mut commands = Vec::with_capacity(command_files.len());
        for file in command_files {
            match Self::load_single_command(&file).await {
                Ok(config) => {
                    info!(
                        name = %config.name,
                        path = %file.display(),
                        "Loaded command from local file"
                    );
                    commands.push(config);
                }
                Err(e) => {
                    warn!(
                        path = %file.display(),
                        error = %e,
                        "Failed to load command file, skipped"
                    );
                }
            }
        }

        info!(
            commands_root = %self.commands_root.display(),
            total_loaded = commands.len(),
            "Local command directory load completed"
        );

        Ok(commands)
    }

    /// 从多个目录批量加载命令，合并结果并按名称去重
    ///
    /// 按目录顺序依次加载，先出现的目录优先级更高（相同名称只保留第一次加载的结果）。
    /// 单个目录加载失败不会中断整体流程，仅记录 `warn` 日志后跳过。
    ///
    /// # Parameters
    ///
    /// * `dirs` - 命令目录列表，按优先级从高到低排列
    ///
    /// # Returns
    ///
    /// 合并去重后的命令列表
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agent_runtime::command::LocalCommandLoader;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let commands = LocalCommandLoader::from_dirs([
    ///     ".codebuddy/commands",
    ///     "/global/shared/commands",
    /// ]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_dirs(
        dirs: impl IntoIterator<Item = impl Into<PathBuf>>,
    ) -> Result<Vec<CommandConfig>, Error> {
        let mut seen_names: HashSet<String> = HashSet::new();
        let mut all_commands: Vec<CommandConfig> = Vec::new();

        for dir in dirs {
            let loader = LocalCommandLoader::new(dir);
            match loader.load().await {
                Ok(commands) => {
                    for command in commands {
                        // Deduplicate by name — first directory wins
                        if seen_names.insert(command.name.clone()) {
                            all_commands.push(command);
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        dir = %loader.commands_root.display(),
                        error = %e,
                        "Failed to load command directory, skipped"
                    );
                }
            }
        }

        Ok(all_commands)
    }

    /// 加载单个命令文件
    ///
    /// 文件名（不含扩展名）即为 `name`，文件完整内容即为 `content`。
    ///
    /// # Parameters
    ///
    /// * `path` - 命令文件路径
    ///
    /// # Errors
    ///
    /// 文件读取失败时返回 IO 错误
    pub async fn load_single_command(path: &Path) -> Result<CommandConfig, Error> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let content = fs::read_to_string(path).await?;

        Ok(CommandConfig {
            name,
            description: String::new(),
            content,
            source: CommandSource::Local,
            source_hint: Some(path.to_string_lossy().into_owned()),
        })
    }

    /// 判断文件是否为支持的命令文件格式
    fn is_command_file(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| self.extensions.contains(ext))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::TempDir;

    fn create_command_file(root: &Path, name: &str, content: &str) {
        std_fs::write(root.join(name), content).unwrap();
    }

    #[tokio::test]
    async fn test_load_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let loader = LocalCommandLoader::new(tmp.path());
        let commands = loader.load().await.unwrap();
        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_load_nonexistent_directory_returns_empty() {
        let loader = LocalCommandLoader::new("/nonexistent/path/for/command/loader/test");
        let commands = loader.load().await.unwrap();
        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_load_md_files() {
        let tmp = TempDir::new().unwrap();
        create_command_file(tmp.path(), "gen-doc.md", "Please generate docs.");
        create_command_file(tmp.path(), "gen-test.md", "Please generate tests.");

        let loader = LocalCommandLoader::new(tmp.path());
        let commands = loader.load().await.unwrap();

        assert_eq!(commands.len(), 2);
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"gen-doc"));
        assert!(names.contains(&"gen-test"));
        assert!(commands.iter().all(|c| c.source == CommandSource::Local));
    }

    #[tokio::test]
    async fn test_load_skips_non_command_files() {
        let tmp = TempDir::new().unwrap();
        create_command_file(tmp.path(), "gen-doc.md", "A command.");
        create_command_file(tmp.path(), "ignore.json", "{}");
        create_command_file(tmp.path(), "ignore.rs", "fn main() {}");

        let loader = LocalCommandLoader::new(tmp.path());
        let commands = loader.load().await.unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "gen-doc");
    }

    #[tokio::test]
    async fn test_with_extensions_replaces_defaults() {
        let tmp = TempDir::new().unwrap();
        create_command_file(tmp.path(), "cmd.md", "Markdown command.");
        create_command_file(tmp.path(), "cmd.txt", "Text command.");

        let loader = LocalCommandLoader::new(tmp.path()).with_extensions(vec!["txt".to_string()]);
        let commands = loader.load().await.unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "cmd");
        assert_eq!(commands[0].content, "Text command.");
    }

    #[test]
    fn test_extensions_accessor() {
        let loader = LocalCommandLoader::new(".codebuddy/commands")
            .with_extensions(vec!["md".to_string(), "txt".to_string()]);
        let exts = loader.extensions();
        assert!(exts.contains("md"));
        assert!(exts.contains("txt"));
        assert!(!exts.contains("rs"));
    }

    #[test]
    fn test_with_workspace_dir() {
        let loader = LocalCommandLoader::default().with_workspace_dir("/tmp/my-workspace");
        assert_eq!(
            loader.commands_root(),
            Path::new("/tmp/my-workspace/commands")
        );
    }

    #[tokio::test]
    async fn test_load_from_dirs_merges_multiple_directories() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        create_command_file(tmp1.path(), "cmd-a.md", "Command A from dir1.");
        create_command_file(tmp2.path(), "cmd-b.md", "Command B from dir2.");

        let commands = LocalCommandLoader::from_dirs([tmp1.path(), tmp2.path()])
            .await
            .unwrap();

        assert_eq!(commands.len(), 2);
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"cmd-a"));
        assert!(names.contains(&"cmd-b"));
    }

    #[tokio::test]
    async fn test_load_from_dirs_deduplicates_by_name() {
        let tmp = TempDir::new().unwrap();
        create_command_file(tmp.path(), "shared.md", "Shared command.");

        // Same directory listed twice — should only load once
        let commands = LocalCommandLoader::from_dirs([tmp.path(), tmp.path()])
            .await
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "shared");
    }

    #[tokio::test]
    async fn test_load_from_dirs_first_dir_wins_on_same_name() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        // Same name in both dirs — first directory wins
        create_command_file(tmp1.path(), "cmd.md", "Content from dir1.");
        create_command_file(tmp2.path(), "cmd.md", "Content from dir2.");

        let commands = LocalCommandLoader::from_dirs([tmp1.path(), tmp2.path()])
            .await
            .unwrap();

        // Same name → deduplicated; only dir1's version is kept
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].content, "Content from dir1.");
    }

    #[tokio::test]
    async fn test_load_from_dirs_skips_nonexistent_directory() {
        let tmp = TempDir::new().unwrap();
        create_command_file(tmp.path(), "cmd.md", "A command.");

        let commands = LocalCommandLoader::from_dirs([
            tmp.path().to_path_buf(),
            PathBuf::from("/nonexistent/path/for/test"),
        ])
        .await
        .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "cmd");
    }

    #[tokio::test]
    async fn test_load_sorted_deterministic_order() {
        let tmp = TempDir::new().unwrap();
        create_command_file(tmp.path(), "z-last.md", "Z command.");
        create_command_file(tmp.path(), "a-first.md", "A command.");
        create_command_file(tmp.path(), "m-middle.md", "M command.");

        let loader = LocalCommandLoader::new(tmp.path());
        let commands = loader.load().await.unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].name, "a-first");
        assert_eq!(commands[1].name, "m-middle");
        assert_eq!(commands[2].name, "z-last");
    }
}
