//! 基于数据库的技能加载器
//!
//! 从 `t_agent_skill_rel` 与 `t_agent_skill_package` 装配指定智能体启用的技能列表。
//! 与 `agent_base::skill::loader::LocalSkillLoader` 形成对称实现：
//! 前者属于装配层（依赖 SeaORM），后者属于核心运行时（零外部存储依赖）。
//!
//! 提供的主要内容：
//! - [`DbSkillLoader`]：数据库技能加载器

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use agent_base::types::skill::SkillConfig;
use entity::agent::{skill_package, skill_rel};

use crate::error::PlatformResult;

/// 数据库技能加载器
///
/// 按 `agent_id` 查询启用的技能绑定，通过 `skill_package_id` 关联
/// `t_agent_skill_package` 获取完整的技能元数据。
/// 加载过程不做任何缓存，调用方应在自己的装配流程中按需调用。
///
/// ## 与 `LocalSkillLoader` 的关系
///
/// `LocalSkillLoader` 从本地文件系统加载技能，包含完整的 `content`（SKILL.md 正文）；
/// `DbSkillLoader` 从数据库加载技能元数据，`content` 为空——完整内容由
/// `skill_load` 工具在运行时从本地缓存目录按需加载。
///
/// ## 加载流程
///
/// 1. 查询 `t_agent_skill_rel`，获取 `agent_id` 对应的启用绑定
/// 2. 通过 `skill_package_id` 关联查询 `t_agent_skill_package`
/// 3. 组装 `SkillConfig`（`content` 为空，运行时按需加载）
pub struct DbSkillLoader {
    db: DatabaseConnection,
}

impl DbSkillLoader {
    /// 创建数据库技能加载器
    ///
    /// # Parameters
    ///
    /// * `db` - SeaORM 数据库连接
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// 加载指定智能体启用的技能列表
    ///
    /// 查询逻辑：
    /// 1. 从 `t_agent_skill_rel` 取出启用绑定
    /// 2. 通过 `skill_package_id` 关联查询 `t_agent_skill_package`
    /// 3. 组装 `SkillConfig`，`content` 为空（运行时由 `skill_load` 工具按需加载）
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    ///
    /// # Returns
    ///
    /// 启用的技能列表；若无任何启用技能则返回空 `Vec`
    pub async fn load(&self, agent_id: i64) -> PlatformResult<Vec<SkillConfig>> {
        let rels = skill_rel::Entity::find()
            .filter(skill_rel::Column::AgentId.eq(agent_id))
            .filter(skill_rel::Column::Enabled.eq(true))
            .all(&self.db)
            .await?;

        if rels.is_empty() {
            return Ok(Vec::new());
        }

        // 通过 skill_package_id 批量查询包元数据
        let package_ids: Vec<i64> = rels.iter().map(|r| r.skill_package_id).collect();

        let packages = skill_package::Entity::find()
            .filter(skill_package::Column::Deleted.eq(false))
            .filter(skill_package::Column::Id.is_in(package_ids))
            .all(&self.db)
            .await?;

        let pkg_map: std::collections::HashMap<i64, skill_package::Model> =
            packages.into_iter().map(|p| (p.id, p)).collect();

        let skills = rels
            .into_iter()
            .filter_map(|rel| {
                let pkg = pkg_map.get(&rel.skill_package_id)?;
                Some(SkillConfig {
                    skill_code: pkg.skill_code.clone(),
                    name: pkg.name.clone(),
                    description: pkg.description.clone(),
                    source_hint: Some(format!("db:skill_package:{}", pkg.id)),
                })
            })
            .collect();

        Ok(skills)
    }
}
