//! 智能体运行时模型
//!
//! 定义智能体运行时所需的核心数据结构
//!
//! 提供的主要内容：
//! - [`AgentConfig`] - 智能体运行时配置（身份 + 模型绑定）
//! - [`RunConfig`] - ReAct 循环运行时参数（轮次 / 并发 / 重试 / 轮询间隔）
//! - [`AgentBundle`] - 智能体装配包（核心 + 全部能力 + 子节点）

use std::{collections::HashSet, time::Duration};

use rig_core::completion::Usage;
use serde::{Deserialize, Serialize};

use crate::{
    MessageContext, ModelTurn,
    constant::{DEFAULT_CANCEL_POLL_INTERVAL_SECS, DEFAULT_MAX_PARALLEL_TOOLS, DEFAULT_MAX_TURNS},
    types::{
        a2a::A2aServiceConfig, command::CommandConfig, mcp::McpServerConfig, rule::RuleConfig,
        skill::SkillConfig, tool::ToolConfig, tool_call_retry::ToolCallRetryConfig,
    },
};
/// 每下钻一层子智能体时，委派深度递减的步长。
///
/// 用于将 `max_delegation_depth`（父预算）转换为子智能体的「剩余可委派次数」，
/// 避免代码中出现裸数字 `1`。
const DELEGATION_DEPTH_STEP: i32 = 1;

/// ReAct 循环运行时参数
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// 最大推理轮次（>= 1）
    pub max_turns: u32,
    /// 工具调用的最大并发数（>= 1，1 即串行）
    pub max_parallel_tools: usize,
    /// 工具调用重试策略
    pub retry: ToolCallRetryConfig,
    /// 是否对工具调用 `arguments` 启用 JSON 容错修复
    pub arguments_json_repair: bool,
    /// 取消状态轮询间隔
    ///
    /// 流式推理中定时查询数据库检测取消信号的间隔。
    pub cancel_poll_interval: Duration,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            max_turns: DEFAULT_MAX_TURNS,
            max_parallel_tools: DEFAULT_MAX_PARALLEL_TOOLS,
            retry: ToolCallRetryConfig::default(),
            arguments_json_repair: false,
            cancel_poll_interval: Duration::from_secs(DEFAULT_CANCEL_POLL_INTERVAL_SECS),
        }
    }
}

/// 智能体运行时配置
///
/// 描述智能体的身份与模型绑定。能力配置（工具、规则、技能、MCP、命令、子智能体）
/// 通过 `AgentBuilder` 的便捷 API（`add_rules`、`add_mcps` 等）注入到
/// [`AgentBundle`] 中，不在此结构体中承载。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// 智能体基础信息
    #[serde(default)]
    pub agent_definition: AgentDefinition,
    /// 模型绑定配置
    #[serde(default)]
    pub model_config: ModelConfig,
}

/// 智能体基础定义
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// 智能体ID
    pub id: i64,
    /// 所属用户ID
    pub user_id: String,
    /// 智能体名称
    pub name: String,
    /// 智能体描述
    pub description: String,
    /// 系统 Prompt
    pub system_prompt: String,
    /// 欢迎语
    pub welcome_message: Option<String>,
    /// 最大子智能体递归委派深度（默认 3）
    ///
    /// 与「智能体身份配置」属于同一类静态属性，与 `(agent_id, user_id)`
    /// 一同构成 Agent 的不可变快照。
    #[serde(default)]
    pub max_delegation_depth: i32,
}

/// 模型绑定配置
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// 模型供应商
    pub provider: String,
    /// 模型编码
    pub model: String,
    /// API 基础地址
    pub base_url: String,
    /// API Key 引用
    pub api_key: String,
    /// 温度参数
    pub temperature: Option<f64>,
    /// 最大输出 token 数
    pub max_tokens: Option<u64>,
}

/// 智能体装配包
///
/// `AgentBuilder` 的标准输入聚合体：在 [`AgentConfig`]（身份 + 模型绑定）的
/// 基础上，进一步聚合工具、规则、技能、MCP、命令以及子智能体装配包。
/// 由各类 Provider（数据库 / HTTP / 本地目录扫描等）一次性装配产出，
/// 可直接喂给 `AgentBuilder` 完成运行时构造。
///
/// 设计要点：
/// - [`AgentConfig`] 描述「节点核心」（不可变身份快照）
/// - [`AgentBundle`] 描述「装配蓝图」（核心 + 全部能力 + 子节点）
/// - 通过 [`AgentBundle::to_agent_config`] 抽取出运行时所需的核心快照，
///   用于注入子智能体身份等场景
#[derive(Debug, Default, Clone)]
pub struct AgentBundle {
    /// 智能体基础信息
    pub agent_definition: AgentDefinition,
    /// 模型绑定配置
    pub model_config: ModelConfig,
    /// 工具配置列表
    pub tool_configs: Vec<ToolConfig>,
    /// 规则配置列表
    pub rule_configs: Vec<RuleConfig>,
    /// 技能配置列表
    pub skill_configs: Vec<SkillConfig>,
    /// MCP 服务配置列表
    pub mcp_configs: Vec<McpServerConfig>,
    /// 命令配置列表
    pub command_configs: Vec<CommandConfig>,
    /// 子智能体装配包列表（递归承载多级委派树）
    pub sub_agent_bundles: Vec<AgentBundle>,
    /// A2A 远程 Agent 服务配置列表
    pub a2a_configs: Vec<A2aServiceConfig>,
}

impl AgentBundle {
    /// 抽取「运行时核心快照」
    ///
    /// 仅复制身份与模型绑定，丢弃所有能力字段。常用于将装配包降级为
    /// 子智能体身份注入到 `DelegateSubAgentTool`、或暴露给只关心
    /// 运行时基础信息的下游组件。
    ///
    /// # Returns
    ///
    /// 仅含 `agent_definition` 与 `model_config` 的 [`AgentConfig`]
    pub fn to_agent_config(&self) -> AgentConfig {
        AgentConfig {
            agent_definition: self.agent_definition.clone(),
            model_config: self.model_config.clone(),
        }
    }

    /// 获取所有工具名称
    pub fn tool_names(&self) -> Vec<String> {
        self.tool_configs.iter().map(|t| t.name.clone()).collect()
    }

    /// 合并另一个 bundle 到自身，并立即归一化
    /// 主智能体的配置以第一个为主
    ///
    /// 合并语义：
    /// - **身份字段**（`agent_definition` / `model_config`）：不覆盖
    /// - **能力字段**（rules / skills / mcp / commands / tools / sub_agents）：
    ///   先追加再交由 [`Self::normalize`] 统一去重与深度截断
    ///
    /// # Parameters
    ///
    /// * `other` - 待并入的 bundle 片段
    pub fn merge(&mut self, other: AgentBundle) {
        self.tool_configs.extend(other.tool_configs);
        self.rule_configs.extend(other.rule_configs);
        self.skill_configs.extend(other.skill_configs);
        self.mcp_configs.extend(other.mcp_configs);
        self.command_configs.extend(other.command_configs);
        self.sub_agent_bundles.extend(other.sub_agent_bundles);
        self.a2a_configs.extend(other.a2a_configs);
        self.normalize();
    }

    /// 对所有能力字段进行规范化
    ///
    /// 规范化操作：
    /// 1. 按业务键对 rules / skills / commands / sub_agents 做 **first-wins** 去重
    ///    （先注入者优先，让本地 / 主 source 的配置稳定胜出）
    /// 2. 根据 `agent_definition.max_delegation_depth` 递归截断子智能体树深度
    ///
    /// 该方法是幂等的：连续多次调用与调用一次结果一致。
    /// `merge` 在合并末尾会自动调用本方法，外部通常无需直接调用。
    pub fn normalize(&mut self) {
        self.rule_configs =
            Self::dedup_first_wins(std::mem::take(&mut self.rule_configs), |r| r.name.clone());
        self.skill_configs = Self::dedup_first_wins(std::mem::take(&mut self.skill_configs), |s| {
            s.skill_code.clone()
        });
        self.command_configs =
            Self::dedup_first_wins(std::mem::take(&mut self.command_configs), |c| {
                c.name.clone()
            });
        self.sub_agent_bundles =
            Self::dedup_first_wins(std::mem::take(&mut self.sub_agent_bundles), |b| {
                b.agent_definition.name.clone()
            });
        self.apply_sub_agent_depth_limit(self.agent_definition.max_delegation_depth);
    }

    /// 递归地为 `sub_agent_bundles` 应用委派深度限制
    ///
    /// - 以 `parent_max_depth - DELEGATION_DEPTH_STEP` 作为顶层子智能体的剩余预算
    ///   （不会低于 0），并递归向下传递，确保整棵子智能体树的深度单调递减
    /// - 当剩余预算为 0 时，进一步的 `sub_agent_bundles` 会被截断，
    ///   与 `DelegateSubAgentTool` / `FlatSubAgentTool` 的下游语义保持一致
    ///
    /// # Parameters
    ///
    /// * `parent_max_depth` - 父级允许的最大委派深度
    fn apply_sub_agent_depth_limit(&mut self, parent_max_depth: i32) {
        let child_depth = (parent_max_depth - DELEGATION_DEPTH_STEP).max(0);
        let bundles = std::mem::take(&mut self.sub_agent_bundles);
        self.sub_agent_bundles = bundles
            .into_iter()
            .map(|b| Self::apply_depth_limit_recursive(b, child_depth))
            .collect();
    }

    /// 按 key 去重，保留**首次**出现的元素与位置
    fn dedup_first_wins<T, K, F>(items: Vec<T>, key_of: F) -> Vec<T>
    where
        K: std::hash::Hash + Eq,
        F: Fn(&T) -> K,
    {
        let mut seen: HashSet<K> = HashSet::new();
        let mut result: Vec<T> = Vec::with_capacity(items.len());
        for item in items {
            if seen.insert(key_of(&item)) {
                result.push(item);
            }
        }
        result
    }

    /// 递归地对单个 bundle 应用深度限制
    fn apply_depth_limit_recursive(mut bundle: AgentBundle, depth: i32) -> AgentBundle {
        if depth <= 0 {
            bundle.sub_agent_bundles.clear();
            return bundle;
        }

        let next_depth = (depth - DELEGATION_DEPTH_STEP).max(0);
        bundle.sub_agent_bundles = bundle
            .sub_agent_bundles
            .into_iter()
            .map(|child| Self::apply_depth_limit_recursive(child, next_depth))
            .collect();
        bundle
    }
}

/// ReAct 循环返回的状态结构体
///
/// 封装 `drive_react_loop` 的核心产出，便于调用方（如 `run_stream_inner`）
/// 在循环外统一处理最终答案的总结与持久化。也便于未来替换核心循环实现。
pub struct LoopResult {
    /// 模型产出的最后一轮 [`ModelTurn`]（纯文本或含工具调用）。
    ///
    /// 仅在正常终止（`FinalizeAction`）时有值；`MaxTurns` 时为 `None`。
    pub last_turn: Option<ModelTurn>,
    /// 循环终止原因
    pub finish_reason: crate::session::FinishReason,
    /// 当前已执行的步数
    pub step: u32,
    /// 截至目前的累计 token 用量
    pub accumulated_usage: Option<Usage>,
    /// 消息上下文（已包含全部推理历史与工具结果）
    pub messages: MessageContext,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{command::CommandSource, rule::RuleSource};

    fn rule(name: &str, content: &str, source: RuleSource) -> RuleConfig {
        RuleConfig {
            name: name.to_string(),
            description: String::new(),
            content: content.to_string(),
            source,
            source_hint: None,
        }
    }

    fn skill(code: &str) -> SkillConfig {
        SkillConfig {
            skill_code: code.to_string(),
            name: code.to_string(),
            description: format!("desc of {code}"),
            source_hint: None,
        }
    }

    fn cmd(name: &str, content: &str, source: CommandSource) -> CommandConfig {
        CommandConfig {
            name: name.to_string(),
            description: String::new(),
            content: content.to_string(),
            source,
            source_hint: None,
        }
    }

    fn make_bundle(name: &str) -> AgentBundle {
        AgentBundle {
            agent_definition: AgentDefinition {
                name: name.to_string(),
                description: format!("desc of {name}"),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn normalize_rules_first_wins_preserves_first_position() {
        let mut bundle = AgentBundle {
            rule_configs: vec![
                rule("a", "local-a", RuleSource::Local),
                rule("b", "local-b", RuleSource::Local),
                rule("a", "remote-a", RuleSource::Injected),
                rule("c", "remote-c", RuleSource::Injected),
            ],
            ..Default::default()
        };
        bundle.normalize();

        let names: Vec<&str> = bundle
            .rule_configs
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, vec!["a", "b", "c"]);
        assert_eq!(bundle.rule_configs[0].content, "local-a");
        assert_eq!(bundle.rule_configs[0].source, RuleSource::Local);
        assert_eq!(bundle.rule_configs[1].content, "local-b");
        assert_eq!(bundle.rule_configs[2].content, "remote-c");
    }

    #[test]
    fn normalize_skills_first_wins() {
        let mut bundle = AgentBundle {
            skill_configs: vec![
                skill("local-only"),
                skill("shared"),
                skill("shared"),
                skill("db-only"),
            ],
            ..Default::default()
        };
        bundle.normalize();

        let codes: Vec<&str> = bundle
            .skill_configs
            .iter()
            .map(|s| s.skill_code.as_str())
            .collect();
        assert_eq!(codes, vec!["local-only", "shared", "db-only"]);
    }

    #[test]
    fn normalize_commands_first_wins() {
        let mut bundle = AgentBundle {
            command_configs: vec![
                cmd("gen-doc", "Local version.", CommandSource::Local),
                cmd("gen-test", "lt", CommandSource::Local),
                cmd("gen-doc", "Remote version.", CommandSource::Injected),
            ],
            ..Default::default()
        };
        bundle.normalize();

        assert_eq!(bundle.command_configs.len(), 2);
        assert_eq!(bundle.command_configs[0].name, "gen-doc");
        assert_eq!(bundle.command_configs[0].content, "Local version.");
        assert_eq!(bundle.command_configs[0].source, CommandSource::Local);
        assert_eq!(bundle.command_configs[1].name, "gen-test");
    }

    #[test]
    fn normalize_sub_agents_first_wins() {
        let mut bundle = AgentBundle {
            sub_agent_bundles: vec![
                make_bundle("local-only"),
                make_bundle("shared"),
                make_bundle("shared"),
                make_bundle("db-only"),
            ],
            ..Default::default()
        };
        bundle.normalize();

        let names: Vec<&str> = bundle
            .sub_agent_bundles
            .iter()
            .map(|b| b.agent_definition.name.as_str())
            .collect();
        assert_eq!(names, vec!["local-only", "shared", "db-only"]);
    }

    #[test]
    fn merge_invokes_normalize_for_dedup_and_depth_limit() {
        let mut base = AgentBundle {
            agent_definition: AgentDefinition {
                max_delegation_depth: 1,
                ..Default::default()
            },
            rule_configs: vec![rule("a", "base-a", RuleSource::Local)],
            sub_agent_bundles: vec![make_nested_bundle()],
            ..Default::default()
        };

        let fragment = AgentBundle {
            rule_configs: vec![rule("a", "frag-a", RuleSource::Injected)],
            ..Default::default()
        };

        base.merge(fragment);

        // first-wins: 保留 base 的 "base-a"
        assert_eq!(base.rule_configs.len(), 1);
        assert_eq!(base.rule_configs[0].content, "base-a");

        // depth_limit 已应用：max_delegation_depth=1 → 顶层剩余预算 0 → 子树被截断
        let parent = &base.sub_agent_bundles[0];
        assert!(parent.sub_agent_bundles.is_empty());
    }

    /// 构造嵌套装配包：parent → child → grandchild
    fn make_nested_bundle() -> AgentBundle {
        let grandchild = make_bundle("grandchild");
        let mut child = make_bundle("child");
        child.sub_agent_bundles = vec![grandchild];
        let mut parent = make_bundle("parent");
        parent.sub_agent_bundles = vec![child];
        parent
    }

    #[test]
    fn depth_limit_recursively_decremented() {
        let mut bundle = AgentBundle {
            agent_definition: AgentDefinition {
                max_delegation_depth: 3,
                ..Default::default()
            },
            sub_agent_bundles: vec![make_nested_bundle()],
            ..Default::default()
        };
        bundle.normalize();

        assert_eq!(bundle.sub_agent_bundles.len(), 1);
        let parent = &bundle.sub_agent_bundles[0];
        assert_eq!(parent.sub_agent_bundles.len(), 1);
        let child = &parent.sub_agent_bundles[0];
        assert_eq!(child.sub_agent_bundles.len(), 1);
        let grandchild = &child.sub_agent_bundles[0];
        assert!(grandchild.sub_agent_bundles.is_empty());
    }

    #[test]
    fn depth_limit_truncates_subtree_when_exhausted() {
        let mut bundle = AgentBundle {
            agent_definition: AgentDefinition {
                max_delegation_depth: 1,
                ..Default::default()
            },
            sub_agent_bundles: vec![make_nested_bundle()],
            ..Default::default()
        };
        // 父预算 1 → 顶层剩余预算 0 → 子树被截断
        bundle.normalize();

        let parent = &bundle.sub_agent_bundles[0];
        assert!(parent.sub_agent_bundles.is_empty());
    }

    #[test]
    fn depth_limit_zero_budget_truncates_top_level() {
        let mut bundle = AgentBundle {
            agent_definition: AgentDefinition {
                max_delegation_depth: 0,
                ..Default::default()
            },
            sub_agent_bundles: vec![make_nested_bundle()],
            ..Default::default()
        };
        bundle.normalize();

        let parent = &bundle.sub_agent_bundles[0];
        assert!(parent.sub_agent_bundles.is_empty());
    }
}
