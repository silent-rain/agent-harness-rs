//! 系统提示词构建模块
//!
//! 提供将基础 system prompt 与 Rules / Skills / Commands 组合为完整提示词的能力。
//!
//! 提供的主要内容：
//! - [`SystemPrompt`] - 通过 builder 模式组合 base + Rules + Skills + Commands

mod system_prompt;

pub use system_prompt::SystemPrompt;
