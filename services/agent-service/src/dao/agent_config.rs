//! 智能体配置数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, Set,
};

use entity::agent::{AgentConfig, agent_config};
use database::Pagination;
use inject::PrimaryDb;

use crate::dto::agent_config::GetAgentConfigsReq;

/// 数据访问
#[injectable]
pub struct AgentConfigDao {
    pub db: PrimaryDb,
}

impl AgentConfigDao {
    /// 获取数据列表（按用户过滤）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetAgentConfigsReq,
    ) -> Result<(Vec<agent_config::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let states = AgentConfig::find()
            .filter(agent_config::Column::UserId.eq(user_id))
            .filter(agent_config::Column::Deleted.eq(false))
            .apply_if(req.start_time, |query, v| {
                query.filter(agent_config::Column::CreatedAt.gte(v))
            })
            .apply_if(req.end_time, |query, v| {
                query.filter(agent_config::Column::CreatedAt.lt(v))
            })
            .apply_if(req.name, |query, v| {
                query.filter(agent_config::Column::Name.like(format!("{v}%")))
            });

        let total = states.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = states
            .order_by_desc(agent_config::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取详情信息
    ///
    /// # Parameters
    ///
    /// * `id` - 智能体 ID
    pub async fn info(&self, id: i64) -> Result<Option<agent_config::Model>, DbErr> {
        AgentConfig::find_by_id(id)
            .filter(agent_config::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 通过名称获取详情信息（用户维度唯一性）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `name` - 智能体名称
    pub async fn info_by_name(
        &self,
        user_id: i64,
        name: String,
    ) -> Result<Option<agent_config::Model>, DbErr> {
        AgentConfig::find()
            .filter(agent_config::Column::UserId.eq(user_id))
            .filter(agent_config::Column::Name.eq(name))
            .filter(agent_config::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 添加详情信息
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待插入的 ActiveModel
    pub async fn create(
        &self,
        active_model: agent_config::ActiveModel,
    ) -> Result<agent_config::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: agent_config::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = AgentConfig::update_many()
            .set(active_model)
            .filter(agent_config::Column::Id.eq(id))
            .filter(agent_config::Column::Deleted.eq(false))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }

    /// 软删除智能体配置
    ///
    /// # Parameters
    ///
    /// * `id` - 智能体 ID
    pub async fn delete(&self, id: i64) -> Result<u64, DbErr> {
        let active_model = agent_config::ActiveModel {
            id: Set(id),
            deleted: Set(true),
            ..Default::default()
        };
        let result = AgentConfig::update_many()
            .set(active_model)
            .filter(agent_config::Column::Id.eq(id))
            .filter(agent_config::Column::Deleted.eq(false))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}
