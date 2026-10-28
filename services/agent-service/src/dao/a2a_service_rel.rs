//! 智能体 A2A 服务绑定关系数据访问层

use nject::injectable;
use sea_orm::{
    ColumnTrait, Condition, DatabaseTransaction, DbErr, EntityTrait, JoinType, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, QueryTrait, TransactionTrait,
};

use database::Pagination;
use entity::agent::{A2aServiceRel, a2a_service_registry, a2a_service_rel};
use inject::PrimaryDb;

use crate::dto::a2a_service_rel::GetA2aServiceRelsReq;

/// 数据访问
#[injectable]
pub struct A2aServiceRelDao {
    pub db: PrimaryDb,
}

impl A2aServiceRelDao {
    /// 获取指定智能体的 A2A 服务绑定列表（关联注册表详情）
    ///
    /// 通过 INNER JOIN `t_agent_a2a_service_registry` 一次查询获取绑定关系及服务名称、配置等信息。
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求（agent_id 必填）
    pub async fn list(
        &self,
        req: &GetA2aServiceRelsReq,
    ) -> Result<(Vec<a2a_service_rel::A2aServiceRelWithRegistry>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = A2aServiceRel::find()
            .filter(a2a_service_rel::Column::AgentId.eq(req.agent_id))
            .apply_if(req.enabled, |query, v| {
                query.filter(a2a_service_rel::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .select_only()
            // a2a_service_rel fields
            .column(a2a_service_rel::Column::Id)
            .column(a2a_service_rel::Column::AgentId)
            .column(a2a_service_rel::Column::RegistryId)
            .column(a2a_service_rel::Column::Enabled)
            .column(a2a_service_rel::Column::CreatedAt)
            .column(a2a_service_rel::Column::UpdatedAt)
            // a2a_service_registry fields (aliased)
            .column_as(a2a_service_registry::Column::Name, "registry_name")
            .column_as(
                a2a_service_registry::Column::Description,
                "registry_description",
            )
            .column_as(a2a_service_registry::Column::BaseUrl, "registry_base_url")
            .column_as(
                a2a_service_registry::Column::TimeoutSecs,
                "registry_timeout_secs",
            )
            .column_as(
                a2a_service_registry::Column::ConfigJson,
                "registry_config_json",
            )
            .column_as(a2a_service_registry::Column::Enabled, "registry_enabled")
            // JOIN a2a_service_registry
            .join(
                JoinType::InnerJoin,
                a2a_service_rel::Entity::belongs_to(a2a_service_registry::Entity)
                    .from(a2a_service_rel::Column::RegistryId)
                    .to(a2a_service_registry::Column::Id)
                    .into(),
            )
            .filter(Condition::all().add(a2a_service_registry::Column::Deleted.eq(false)))
            .order_by_desc(a2a_service_rel::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .into_model::<a2a_service_rel::A2aServiceRelWithRegistry>()
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取指定智能体已绑定的所有 A2A 服务注册表 ID 列表
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    pub async fn list_registry_ids_by_agent(&self, agent_id: i64) -> Result<Vec<i64>, DbErr> {
        let results = A2aServiceRel::find()
            .filter(a2a_service_rel::Column::AgentId.eq(agent_id))
            .all(self.db.db())
            .await?;

        Ok(results.into_iter().map(|r| r.registry_id).collect())
    }

    /// 获取详情信息
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn info(&self, id: i64) -> Result<Option<a2a_service_rel::Model>, DbErr> {
        A2aServiceRel::find_by_id(id).one(self.db.db()).await
    }

    /// 更新绑定关系
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: a2a_service_rel::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = A2aServiceRel::update_many()
            .set(active_model)
            .filter(a2a_service_rel::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}

/// 批量同步绑定关系（事务处理）
impl A2aServiceRelDao {
    /// 批量同步智能体 A2A 服务绑定（在事务中执行新增和删除）
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `to_add` - 待新增的 ActiveModel 列表
    /// * `to_remove` - 待删除的注册表 ID 列表
    pub async fn batch_save(
        &self,
        agent_id: i64,
        to_add: Vec<a2a_service_rel::ActiveModel>,
        to_remove: &[i64],
    ) -> Result<(), DbErr> {
        let txn = self.db.db().begin().await?;

        self.txn_batch_create(&txn, to_add).await?;
        self.txn_delete_by_agent_and_registries(&txn, agent_id, to_remove)
            .await?;

        txn.commit().await?;
        Ok(())
    }

    /// 在事务中批量创建绑定关系
    ///
    /// # Parameters
    ///
    /// * `txn` - 数据库事务
    /// * `active_models` - 待插入的 ActiveModel 列表
    async fn txn_batch_create(
        &self,
        txn: &DatabaseTransaction,
        active_models: Vec<a2a_service_rel::ActiveModel>,
    ) -> Result<(), DbErr> {
        if active_models.is_empty() {
            return Ok(());
        }
        A2aServiceRel::insert_many(active_models).exec(txn).await?;
        Ok(())
    }

    /// 在事务中删除指定智能体下指定注册表的绑定关系
    ///
    /// # Parameters
    ///
    /// * `txn` - 数据库事务
    /// * `agent_id` - 智能体 ID
    /// * `registry_ids` - 待删除的注册表 ID 列表
    async fn txn_delete_by_agent_and_registries(
        &self,
        txn: &DatabaseTransaction,
        agent_id: i64,
        registry_ids: &[i64],
    ) -> Result<u64, DbErr> {
        if registry_ids.is_empty() {
            return Ok(0);
        }
        let result = A2aServiceRel::delete_many()
            .filter(a2a_service_rel::Column::AgentId.eq(agent_id))
            .filter(a2a_service_rel::Column::RegistryId.is_in(registry_ids.to_vec()))
            .exec(txn)
            .await?;

        Ok(result.rows_affected)
    }
}
