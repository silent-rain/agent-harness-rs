//! Skills 内省工具
//!
//! 提供以下工具实现技能的生命周期管理、三阶段加载与脚本执行：
//! - [`SkillManagementTool`] - 统一管理工具（`skill_manage`：install / update / uninstall / list / query）
//! - [`SkillLoadTool`] - 加载技能完整指令（`skill_load`）
//! - [`SkillListDocsTool`] - 列出技能可用文档（`skill_list_docs`）
//! - [`SkillSelectDocsTool`] - 读取指定文档内容（`skill_select_docs`）
//! - [`SkillExecTool`] - 在技能目录下执行脚本并收集输出文件（`skill_exec`）

mod skill_exec;
mod skill_list_docs;
mod skill_load;
mod skill_manage;
mod skill_select_docs;

pub use skill_exec::SkillExecTool;
pub use skill_list_docs::SkillListDocsTool;
pub use skill_load::SkillLoadTool;
pub use skill_manage::{SkillBackend, SkillManageArgs, SkillManageOutput, SkillManagementTool};
pub use skill_select_docs::SkillSelectDocsTool;
