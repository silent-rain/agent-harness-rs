//! 常量

/// 默认工作空间根目录名
pub const DEFAULT_WORKSPACE_DIR: &str = ".codebuddy";

/// 默认 Skills 子目录名（相对于 `DEFAULT_WORKSPACE_DIR`）
pub const DEFAULT_SKILLS_DIR: &str = "skills";
/// Skill 入口文件名
pub const SKILL_ENTRY_FILE: &str = "SKILL.md";

/// Skill 安装包默认大小上限（10 MiB）
///
/// 用于内置安装通道的安全护栏，超过此值的包将被拒绝。
pub const DEFAULT_SKILL_PACKAGE_MAX_BYTES: u64 = 10 * 1024 * 1024;

/// Skill 安装包惰性解压缓存子目录名
///
/// 实际路径：`<workspace_dir>/<DEFAULT_SKILLS_DIR>/<DEFAULT_SKILL_PACKAGE_CACHE_DIR>/<skill_code>@<sha256_short>`
pub const DEFAULT_SKILL_PACKAGE_CACHE_DIR: &str = ".cache";

/// 默认 Rules 子目录名（相对于 `DEFAULT_WORKSPACE_DIR`）
pub const DEFAULT_RULES_DIR: &str = "rules";
/// Rule 入口文件扩展名（支持 `.md` 和 `.txt`）
pub const RULE_ENTRY_EXTENSIONS: &[&str] = &["md", "txt"];

/// 默认 MCP 子目录名（相对于 `DEFAULT_WORKSPACE_DIR`）
pub const DEFAULT_MCP_CONFIG_DIR: &str = "mcp";
/// 默认 MCP 配置文件名（相对于 `DEFAULT_MCP_CONFIG_DIR`）
pub const DEFAULT_MCP_CONFIG_FILE: &str = "mcp.json";

/// 默认 A2A 子目录名（相对于 `DEFAULT_WORKSPACE_DIR`）
pub const DEFAULT_A2A_CONFIG_DIR: &str = "a2a";
/// 默认 A2A 配置文件名（相对于 `DEFAULT_A2A_CONFIG_DIR`）
pub const DEFAULT_A2A_CONFIG_FILE: &str = "a2a.json";

/// 默认 Commands 子目录名（相对于 `DEFAULT_WORKSPACE_DIR`）
pub const DEFAULT_COMMANDS_DIR: &str = "commands";
/// Command 入口文件扩展名（支持 `.md`）
pub const COMMAND_ENTRY_EXTENSIONS: &[&str] = &["md"];

/// 默认 Sub-Agents 子目录名（相对于 `DEFAULT_WORKSPACE_DIR`）
pub const DEFAULT_SUB_AGENTS_DIR: &str = "agents";
/// Sub-Agent 入口文件扩展名（支持 `.md`）
pub const SUB_AGENT_ENTRY_EXTENSIONS: &[&str] = &["md"];

/// 默认最大并发工具数
pub const DEFAULT_MAX_PARALLEL_TOOLS: usize = 8;
/// 默认最大推理轮次
pub const DEFAULT_MAX_TURNS: u32 = 10;
/// 默认取消状态轮询间隔（秒）
///
/// 流式推理中定时查询数据库检测取消信号的间隔。
pub const DEFAULT_CANCEL_POLL_INTERVAL_SECS: u64 = 5;

/// 默认事件流通道容量
pub const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 64;
