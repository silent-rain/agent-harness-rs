//! 智能体子智能体绑定关系数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DbErr, EntityTrait, JoinType, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, QueryTrait,
};

use entity::agent::{SubAgentRel, agent_config, sub_agent_rel};
use database::Pagination;
use inject::PrimaryDb;

use crate::dto::sub_agent_rel::GetSubAgentRelsReq;

/// 数据访问
#[injectable]
pub struct SubAgentRelDao {
    pub db: PrimaryDb,
}

impl SubAgentRelDao {
    /// 获取指定智能体的子智能体绑定列表（关联子智能体详情）
    ///
    /// 通过 INNER JOIN `t_agent_config` 一次查询获取绑定关系及子智能体名称、描述、状态。
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求（agent_id 必填）
    pub async fn list(
        &self,
        req: &GetSubAgentRelsReq,
    ) -> Result<(Vec<sub_agent_rel::SubAgentRelWithAgent>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = SubAgentRel::find()
            .filter(sub_agent_rel::Column::AgentId.eq(req.agent_id))
            .apply_if(req.enabled, |query, v| {
                query.filter(sub_agent_rel::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .select_only()
            // sub_agent_rel fields
            .column(sub_agent_rel::Column::Id)
            .column(sub_agent_rel::Column::AgentId)
            .column(sub_agent_rel::Column::SubAgentId)
            .column(sub_agent_rel::Column::Alias)
            .column(sub_agent_rel::Column::ShareContext)
            .column(sub_agent_rel::Column::Enabled)
            .column(sub_agent_rel::Column::CreatedAt)
            .column(sub_agent_rel::Column::UpdatedAt)
            // agent_config fields (aliased)
            .column_as(agent_config::Column::Name, "agent_name")
            .column_as(agent_config::Column::Description, "agent_description")
            .column_as(agent_config::Column::Status, "agent_status")
            // JOIN agent_config
            .join(
                JoinType::InnerJoin,
                sub_agent_rel::Entity::belongs_to(agent_config::Entity)
                    .from(sub_agent_rel::Column::SubAgentId)
                    .to(agent_config::Column::Id)
                    .into(),
            )
            .filter(Condition::all().add(agent_config::Column::Deleted.eq(false)))
            .order_by_desc(sub_agent_rel::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .into_model::<sub_agent_rel::SubAgentRelWithAgent>()
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取详情信息
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn info(&self, id: i64) -> Result<Option<sub_agent_rel::Model>, DbErr> {
        SubAgentRel::find_by_id(id).one(self.db.db()).await
    }

    /// 新增绑定关系
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待插入的 ActiveModel
    pub async fn add(
        &self,
        active_model: sub_agent_rel::ActiveModel,
    ) -> Result<sub_agent_rel::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新绑定关系
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: sub_agent_rel::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = SubAgentRel::update_many()
            .set(active_model)
            .filter(sub_agent_rel::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }

    /// 删除绑定关系
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn delete(&self, id: i64) -> Result<u64, DbErr> {
        let result = SubAgentRel::delete_by_id(id).exec(self.db.db()).await?;

        Ok(result.rows_affected)
    }
}
