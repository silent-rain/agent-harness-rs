//! 智能体技能绑定关系数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, Set,
};

use entity::agent::{SkillRel, skill_rel};
use database::Pagination;
use inject::PrimaryDb;

use crate::dto::skill_rel::GetSkillRelsReq;

/// 数据访问
#[injectable]
pub struct SkillRelDao {
    pub db: PrimaryDb,
}

impl SkillRelDao {
    /// 获取指定智能体的技能绑定列表
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求（agent_id 必填）
    pub async fn list(
        &self,
        user_id: i64,
        req: GetSkillRelsReq,
    ) -> Result<(Vec<skill_rel::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = SkillRel::find()
            .filter(skill_rel::Column::UserId.eq(user_id))
            .filter(skill_rel::Column::AgentId.eq(req.agent_id))
            .apply_if(req.enabled, |query, v| {
                query.filter(skill_rel::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .order_by_desc(skill_rel::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取技能绑定详情
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn info(&self, id: i64) -> Result<Option<skill_rel::Model>, DbErr> {
        SkillRel::find_by_id(id).one(self.db.db()).await
    }

    /// 通过智能体 ID 和包 ID 查询绑定关系（用于唯一性检查）
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `skill_package_id` - 技能安装包 ID
    pub async fn info_by_agent_and_package(
        &self,
        agent_id: i64,
        skill_package_id: i64,
    ) -> Result<Option<skill_rel::Model>, DbErr> {
        SkillRel::find()
            .filter(skill_rel::Column::AgentId.eq(agent_id))
            .filter(skill_rel::Column::SkillPackageId.eq(skill_package_id))
            .one(self.db.db())
            .await
    }

    /// 添加技能绑定
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待插入的 ActiveModel
    pub async fn create(
        &self,
        active_model: skill_rel::ActiveModel,
    ) -> Result<skill_rel::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新技能绑定
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: skill_rel::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = SkillRel::update_many()
            .set(active_model)
            .filter(skill_rel::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }

    /// 删除技能绑定（物理删除）
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn delete(&self, id: i64) -> Result<u64, DbErr> {
        let active_model = skill_rel::ActiveModel {
            id: Set(id),
            ..Default::default()
        };
        let result = SkillRel::delete(active_model).exec(self.db.db()).await?;

        Ok(result.rows_affected)
    }
}
