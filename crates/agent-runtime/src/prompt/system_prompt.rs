//! 系统提示词构建
//!
//! 将基础 system prompt 与 Rules / Skills / Commands 组合为完整的提示词。
//!
//! 设计原则：
//! - 通过 [`SystemPrompt`] 以链式调用方式按需注入各 section
//! - 每个 section 自管其 title、intro 和表体格式，调用方无需感知细节
//! - 任一 section 为空时跳过，避免在最终提示词中出现无意义的空标题
//!
//! 提供的主要内容：
//! - [`SystemPrompt`] - 通过 builder 模式组合 base + Rules + Skills + Commands

use crate::types::{command::CommandConfig, rule::RuleConfig, skill::SkillConfig};

/// 系统提示词。
///
/// 以 builder 风格收集 base / rules / skills / commands，最终通过 [`build`](Self::build)
/// 渲染为完整的 system prompt 字符串。各 section 的渲染顺序固定为：
/// `base → Rules → Skills → Commands`，任一部分为空则跳过；全部为空时返回空字符串。
///
/// # Examples
///
/// ```no_run
/// use agent_base::prompt::SystemPrompt;
///
/// let prompt = SystemPrompt::new()
///     .with_base("You are a helpful assistant.")
///     .with_rules(vec![])
///     .with_skills(vec![])
///     .with_commands(vec![])
///     .build();
/// ```
#[derive(Debug, Default, Clone)]
pub struct SystemPrompt {
    base: String,
    rules: Vec<RuleConfig>,
    skills: Vec<SkillConfig>,
    commands: Vec<CommandConfig>,
}

impl SystemPrompt {
    /// 创建一个空的系统提示词。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置基础 system prompt
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    /// 设置规则列表。
    pub fn with_rules(mut self, rules: Vec<RuleConfig>) -> Self {
        self.rules = rules;
        self
    }

    /// 设置技能列表（仅摘要，完整内容通过 `skill_load` 工具按需加载）。
    pub fn with_skills(mut self, skills: Vec<SkillConfig>) -> Self {
        self.skills = skills;
        self
    }

    /// 设置命令列表（仅名称，完整内容通过 `use_command` 工具按需加载）。
    pub fn with_commands(mut self, commands: Vec<CommandConfig>) -> Self {
        self.commands = commands;
        self
    }

    /// 构建完整的 system prompt。
    ///
    /// 按 `base → Rules → Skills → Commands` 顺序拼接；任一部分为空则跳过；
    /// 全部为空时返回空字符串。
    pub fn build(&self) -> String {
        [
            self.render_base(),
            self.render_rules(),
            self.render_skills(),
            self.render_commands(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n")
    }

    // ─── 各 section 渲染（每个函数自管 title、intro 和表体格式）──────────

    /// 渲染基础 system prompt，空白则返回 `None`。
    fn render_base(&self) -> Option<String> {
        let trimmed = self.base.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    /// 渲染 Rules 区块（`## Rules` + 每条规则 `### {name}` + 正文），列表为空时返回 `None`。
    fn render_rules(&self) -> Option<String> {
        if self.rules.is_empty() {
            return None;
        }

        let mut section = String::from("## Rules\n\n以下规则约束你的行为，请严格遵守：\n");
        for item in &self.rules {
            section.push_str(&format!("\n### {}\n\n{}\n", item.name, item.content.trim()));
        }
        Some(section)
    }

    /// 渲染 Skills 摘要区块（Markdown 表格：技能编码 / 名称 / 描述），列表为空时返回 `None`。
    ///
    /// 仅注入摘要信息，完整指令由 `skill_load` 工具按需加载。
    fn render_skills(&self) -> Option<String> {
        if self.skills.is_empty() {
            return None;
        }

        let mut section = String::from(
            "## Skills\n\n\
             以下是你具备的专项能力。当用户需求匹配某个技能时，\n\
             请先调用 `skill_load` 工具加载该技能的完整指令后再执行。\n\
             \n| 技能编码 | 名称 | 描述 |\n|---|---|---|\n",
        );
        for item in &self.skills {
            let name = if item.name.is_empty() {
                &item.skill_code
            } else {
                &item.name
            };
            let desc = if item.description.is_empty() {
                "（无描述）"
            } else {
                item.description.trim()
            };
            section.push_str(&format!("| {} | {} | {} |\n", item.skill_code, name, desc));
        }
        Some(section)
    }

    /// 渲染 Commands 摘要区块（Markdown 表格：命令名称），列表为空时返回 `None`。
    ///
    /// 仅注入名称列表，完整提示词由 `command_load` 工具按需加载。
    fn render_commands(&self) -> Option<String> {
        if self.commands.is_empty() {
            return None;
        }

        let mut section = String::from(
            "## Commands\n\n\
             以下是你可用的命令列表。当用户使用 `/<command>` 斜杠指令时，\n\
             请先调用 `command_load` 工具加载该命令的完整提示词后再执行。\n\
             \n| 命令名称 |\n|---|\n",
        );
        for item in &self.commands {
            section.push_str(&format!("| {} |\n", item.name));
        }
        Some(section)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{command::CommandSource, rule::RuleSource};

    fn rule(name: &str, content: &str) -> RuleConfig {
        RuleConfig {
            name: name.to_string(),
            description: String::new(),
            content: content.to_string(),
            source: RuleSource::Local,
            source_hint: None,
        }
    }

    fn skill(code: &str, name: &str, desc: &str) -> SkillConfig {
        SkillConfig {
            skill_code: code.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            source_hint: None,
        }
    }

    fn command(name: &str) -> CommandConfig {
        CommandConfig {
            name: name.to_string(),
            description: String::new(),
            content: String::new(),
            source: CommandSource::Local,
            source_hint: None,
        }
    }

    #[test]
    fn empty_inputs_yield_empty_string() {
        let out = SystemPrompt::new().build();
        assert!(out.is_empty());
    }

    #[test]
    fn base_only_trimmed() {
        let out = SystemPrompt::new().with_base("  hello  ").build();
        assert_eq!(out, "hello");
    }

    #[test]
    fn rules_section_rendered() {
        let out = SystemPrompt::new()
            .with_base("base")
            .with_rules(vec![rule("NoSecrets", "  do not leak secrets  ")])
            .build();
        assert!(out.contains("base"));
        assert!(out.contains("## Rules"));
        assert!(out.contains("### NoSecrets"));
        assert!(out.contains("do not leak secrets"));
    }

    #[test]
    fn skills_use_skill_code_when_name_empty() {
        let out = SystemPrompt::new()
            .with_skills(vec![skill("sk_code", "", "")])
            .build();
        assert!(out.contains("## Skills"));
        assert!(out.contains("sk_code"));
        assert!(out.contains("（无描述）"));
    }

    #[test]
    fn commands_rendered_as_table() {
        let out = SystemPrompt::new()
            .with_commands(vec![command("deploy"), command("rollback")])
            .build();
        assert!(out.contains("## Commands"));
        assert!(out.contains("| deploy |"));
        assert!(out.contains("| rollback |"));
    }

    #[test]
    fn sections_joined_in_order() {
        let out = SystemPrompt::new()
            .with_base("BASE")
            .with_rules(vec![rule("R", "rc")])
            .with_skills(vec![skill("S", "sname", "sdesc")])
            .build();
        let pos_base = out.find("BASE").unwrap();
        let pos_rules = out.find("## Rules").unwrap();
        let pos_skills = out.find("## Skills").unwrap();
        assert!(pos_base < pos_rules);
        assert!(pos_rules < pos_skills);
    }
}
