//! 内置工具集合
use rig_core::tool::ToolDyn;

use super::{DuckDuckGoTool, ListDirTool, ReadFileTool, RunCommandTool, SearchFilesTool};

/// 返回所有内置工具实例列表
///
/// # Returns
///
/// 已装箱的工具实例列表，可直接传入
/// [`crate::tool::ToolPool::register_many`] 或 `AgentBuilder::with_tools()`。
///
/// # Examples
///
/// ```rust,no_run
/// use agent_runtime::tool::{ToolPool, simple_tools::all_builtin_tools};
/// use agent_runtime::types::tool::ToolSource;
///
/// let mut pool = ToolPool::new();
/// pool.register_many(all_builtin_tools(), ToolSource::Builtin);
/// let toolset = pool.into_contextual_toolset();
/// ```
pub fn all_builtin_tools() -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(ListDirTool),
        Box::new(ReadFileTool),
        Box::new(SearchFilesTool),
        Box::new(RunCommandTool),
        Box::new(DuckDuckGoTool),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `all_builtin_tools()` should include the DuckDuckGo search tool so
    /// that any agent declaring `duckduckgo_search` in its `tool_configs`
    /// whitelist resolves to a working instance via the registered
    /// [`crate::tool::ToolPool`].
    #[test]
    fn all_builtin_tools_contains_duckduckgo() {
        let names: Vec<String> = all_builtin_tools().iter().map(|t| t.name()).collect();
        assert!(
            names.iter().any(|n| n == "duckduckgo_search"),
            "duckduckgo_search must be registered, got: {names:?}"
        );
    }
}
