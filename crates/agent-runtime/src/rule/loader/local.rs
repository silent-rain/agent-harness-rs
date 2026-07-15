//! 本地规则加载器
//!
//! 从本地文件系统扫描规则文件，将每个文件转换为 [`RuleConfig`]。
//!
//! 提供的主要内容：
//! - [`LocalRuleLoader`] - 本地文件系统规则加载器

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tokio::fs;
use tracing::{info, warn};

use crate::{
    Error,
    constant::{DEFAULT_RULES_DIR, DEFAULT_WORKSPACE_DIR, RULE_ENTRY_EXTENSIONS},
    types::rule::{RuleConfig, RuleSource},
};

/// 本地规则加载器
///
/// 扫描指定目录下匹配扩展名的文件（非递归），将每个文件加载为一条规则。
/// 文件名（不含扩展名）作为规则的 `name`，文件内容作为规则正文。
///
/// 设计上保持"被动"：仅在 `load()` 调用时执行 I/O，不持有任何运行期可变状态。
/// 加载结果由调用方通过
/// [`AgentBuilder::add_rules`](crate::runtime::AgentBuilder::add_rules) 注入到 bundle 中统一合并。
///
/// ## 使用示例
///
/// ```rust,no_run
/// use agent_runtime::rule::LocalRuleLoader;
///
/// # async fn example() -> anyhow::Result<()> {
/// let loader = LocalRuleLoader::new(".codebuddy/rules");
/// let rules = loader.load().await?;
///
/// // 通过 AgentBuilder::add_rules 注入到 bundle
/// // builder.add_rules(rules);
/// # Ok(())
/// # }
/// ```
pub struct LocalRuleLoader {
    /// Rules 根目录路径
    rules_root: PathBuf,
    /// 支持的文件扩展名集合（不含前缀点，如 `"md"`、`"txt"`）
    extensions: HashSet<String>,
}

impl Default for LocalRuleLoader {
    fn default() -> Self {
        let extensions = RULE_ENTRY_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            rules_root: Path::new(DEFAULT_WORKSPACE_DIR).join(DEFAULT_RULES_DIR),
            extensions,
        }
    }
}

impl LocalRuleLoader {
    /// 创建本地规则加载器
    ///
    /// # Parameters
    ///
    /// * `rules_root` - rules 根目录路径（如 `.codebuddy/rules`）
    pub fn new(rules_root: impl Into<PathBuf>) -> Self {
        Self {
            rules_root: rules_root.into(),
            ..Default::default()
        }
    }

    /// 设置支持的文件解析格式列表
    ///
    /// 替换默认的扩展名集合（`.md`、`.txt`）。
    /// 扩展名不含前缀点，例如 `"md"`、`"txt"`、`"yaml"`。
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
    /// 将 `rules_root` 重新设置为 `workspace_dir/rules`。
    ///
    /// # Parameters
    ///
    /// * `workspace_dir` - 工作空间根目录（如 `.codebuddy`）
    pub fn with_workspace_dir(mut self, workspace_dir: impl Into<PathBuf>) -> Self {
        self.rules_root = workspace_dir.into().join(DEFAULT_RULES_DIR);
        self
    }

    /// 获取 rules 根目录
    pub fn rules_root(&self) -> &Path {
        &self.rules_root
    }

    /// 获取当前支持的文件扩展名集合
    pub fn extensions(&self) -> &HashSet<String> {
        &self.extensions
    }

    /// 扫描本地 rules 根目录并加载所有规则
    ///
    /// 遍历 `rules_root` 下匹配扩展名的文件（非递归），将每个文件转换为 [`RuleConfig`]。
    /// 单个文件读取失败不会中断整体加载，仅记录 `warn` 级别日志后跳过。
    ///
    /// 当 `rules_root` 不存在时返回空列表（视作"无本地规则"，不视为错误）。
    ///
    /// # Returns
    ///
    /// 加载成功的规则列表，按文件名字典序排列以保证确定性
    ///
    /// # Errors
    ///
    /// 当 `rules_root` 存在但读取目录失败时返回 IO 错误
    pub async fn load(&self) -> Result<Vec<RuleConfig>, Error> {
        if !self.rules_root.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&self.rules_root).await?;
        let mut rule_files: Vec<PathBuf> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && self.is_rule_file(&path) {
                rule_files.push(path);
            }
        }

        // Sort for deterministic order
        rule_files.sort();

        let mut rules = Vec::with_capacity(rule_files.len());
        for file in rule_files {
            match Self::load_single_rule(&file).await {
                Ok(config) => {
                    info!(
                        name = %config.name,
                        path = %file.display(),
                        "Loaded rule from local file"
                    );
                    rules.push(config);
                }
                Err(e) => {
                    warn!(
                        path = %file.display(),
                        error = %e,
                        "Failed to load rule file, skipped"
                    );
                }
            }
        }

        info!(
            rules_root = %self.rules_root.display(),
            total_loaded = rules.len(),
            "Local rule directory load completed"
        );

        Ok(rules)
    }

    /// 从多个目录批量加载规则，合并结果并按文件路径去重
    ///
    /// 按目录顺序依次加载，先出现的目录优先级更高（相同文件路径只保留第一次加载的结果）。
    /// 单个目录加载失败不会中断整体流程，仅记录 `warn` 日志后跳过。
    ///
    /// # Parameters
    ///
    /// * `dirs` - 规则目录列表，按优先级从高到低排列
    ///
    /// # Returns
    ///
    /// 合并去重后的规则列表
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agent_runtime::rule::LocalRuleLoader;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let rules = LocalRuleLoader::from_dirs([
    ///     ".codebuddy/rules",
    ///     "/global/shared/rules",
    /// ]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_dirs(
        dirs: impl IntoIterator<Item = impl Into<PathBuf>>,
    ) -> Result<Vec<RuleConfig>, Error> {
        let mut seen_paths: HashSet<String> = HashSet::new();
        let mut all_rules: Vec<RuleConfig> = Vec::new();

        for dir in dirs {
            let loader = LocalRuleLoader::new(dir);
            match loader.load().await {
                Ok(rules) => {
                    for rule in rules {
                        // Deduplicate by name — first directory wins
                        if seen_paths.insert(rule.name.clone()) {
                            all_rules.push(rule);
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        dir = %loader.rules_root.display(),
                        error = %e,
                        "Failed to load rule directory, skipped"
                    );
                }
            }
        }

        Ok(all_rules)
    }

    /// 加载单个规则文件
    ///
    /// 读取指定路径的文件，将文件名（不含扩展名）作为规则名称，
    /// 文件内容作为规则正文，构造 [`RuleConfig`]。
    ///
    /// # Parameters
    ///
    /// * `path` - 规则文件路径
    ///
    /// # Errors
    ///
    /// 文件读取失败时返回 IO 错误
    pub async fn load_single_rule(path: &Path) -> Result<RuleConfig, Error> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let content = fs::read_to_string(path).await?;

        Ok(RuleConfig {
            name,
            description: String::new(),
            content,
            source: RuleSource::Local,
            source_hint: Some(path.to_string_lossy().into_owned()),
        })
    }

    /// 判断文件是否为支持的规则文件格式
    fn is_rule_file(&self, path: &Path) -> bool {
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

    fn create_rule_file(root: &Path, name: &str, content: &str) {
        std_fs::write(root.join(name), content).unwrap();
    }

    #[tokio::test]
    async fn test_load_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let loader = LocalRuleLoader::new(tmp.path());
        let rules = loader.load().await.unwrap();
        assert!(rules.is_empty());
    }

    #[tokio::test]
    async fn test_load_nonexistent_directory_returns_empty() {
        let loader = LocalRuleLoader::new("/nonexistent/path/for/rule/loader/test");
        let rules = loader.load().await.unwrap();
        assert!(rules.is_empty());
    }

    #[tokio::test]
    async fn test_load_md_and_txt_files() {
        let tmp = TempDir::new().unwrap();
        create_rule_file(tmp.path(), "code-style.md", "Use 4 spaces for indentation.");
        create_rule_file(tmp.path(), "naming.txt", "Use snake_case for variables.");

        let loader = LocalRuleLoader::new(tmp.path());
        let rules = loader.load().await.unwrap();

        assert_eq!(rules.len(), 2);
        let names: Vec<&str> = rules.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"code-style"));
        assert!(names.contains(&"naming"));
        assert!(rules.iter().all(|r| r.source == RuleSource::Local));
    }

    #[tokio::test]
    async fn test_load_skips_non_rule_files() {
        let tmp = TempDir::new().unwrap();
        create_rule_file(tmp.path(), "rule.md", "A rule.");
        create_rule_file(tmp.path(), "ignore.json", "{}");
        create_rule_file(tmp.path(), "ignore.rs", "fn main() {}");

        let loader = LocalRuleLoader::new(tmp.path());
        let rules = loader.load().await.unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "rule");
    }

    #[tokio::test]
    async fn test_with_extensions_replaces_defaults() {
        let tmp = TempDir::new().unwrap();
        create_rule_file(tmp.path(), "rule.md", "Markdown rule.");
        create_rule_file(tmp.path(), "rule.yaml", "yaml: true");

        let loader = LocalRuleLoader::new(tmp.path()).with_extensions(vec!["yaml".to_string()]);
        let rules = loader.load().await.unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "rule");
        assert_eq!(rules[0].content, "yaml: true");
    }

    #[test]
    fn test_extensions_accessor() {
        let loader = LocalRuleLoader::new(".codebuddy/rules")
            .with_extensions(vec!["md".to_string(), "rst".to_string()]);
        let exts = loader.extensions();
        assert!(exts.contains("md"));
        assert!(exts.contains("rst"));
        assert!(!exts.contains("txt"));
    }

    #[test]
    fn test_with_workspace_dir() {
        let loader = LocalRuleLoader::default().with_workspace_dir("/tmp/my-workspace");
        assert_eq!(loader.rules_root(), Path::new("/tmp/my-workspace/rules"));
    }

    #[tokio::test]
    async fn test_load_from_dirs_merges_multiple_directories() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        create_rule_file(tmp1.path(), "rule-a.md", "Rule A from dir1.");
        create_rule_file(tmp2.path(), "rule-b.md", "Rule B from dir2.");

        let rules = LocalRuleLoader::from_dirs([tmp1.path(), tmp2.path()])
            .await
            .unwrap();

        assert_eq!(rules.len(), 2);
        let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"rule-a"));
        assert!(names.contains(&"rule-b"));
    }

    #[tokio::test]
    async fn test_load_from_dirs_deduplicates_by_path() {
        let tmp = TempDir::new().unwrap();
        create_rule_file(tmp.path(), "shared.md", "Shared rule.");

        // Same directory listed twice — should only load once
        let rules = LocalRuleLoader::from_dirs([tmp.path(), tmp.path()])
            .await
            .unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "shared");
    }

    #[tokio::test]
    async fn test_load_from_dirs_first_dir_wins_on_same_filename() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        // Same name in both dirs — first directory wins
        create_rule_file(tmp1.path(), "rule.md", "Content from dir1.");
        create_rule_file(tmp2.path(), "rule.md", "Content from dir2.");

        let rules = LocalRuleLoader::from_dirs([tmp1.path(), tmp2.path()])
            .await
            .unwrap();

        // Same name → deduplicated; only dir1's version is kept
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].content, "Content from dir1.");
    }

    #[tokio::test]
    async fn test_load_from_dirs_skips_nonexistent_directory() {
        let tmp = TempDir::new().unwrap();
        create_rule_file(tmp.path(), "rule.md", "A rule.");

        let rules = LocalRuleLoader::from_dirs([
            tmp.path().to_path_buf(),
            PathBuf::from("/nonexistent/path/for/test"),
        ])
        .await
        .unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "rule");
    }
}
