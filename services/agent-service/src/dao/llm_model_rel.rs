//! 智能体 LLM 模型绑定关系数据访问层

use nject::injectable;
use sea_orm::{
    ColumnTrait, DatabaseTransaction, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, TransactionTrait,
};

use entity::agent::{LlmModelRel, llm_model_rel};
use database::Pagination;
use inject::PrimaryDb;

use crate::dto::llm_model_rel::GetLlmModelRelsReq;

/// 数据访问
#[injectable]
pub struct LlmModelRelDao {
    pub db: PrimaryDb,
}

impl LlmModelRelDao {
    /// 获取指定智能体的 LLM 模型绑定列表
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求（agent_id 必填）
    pub async fn list(
        &self,
        req: &GetLlmModelRelsReq,
    ) -> Result<(Vec<llm_model_rel::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = LlmModelRel::find()
            .filter(llm_model_rel::Column::AgentId.eq(req.agent_id))
            .apply_if(req.enabled, |query, v| {
                query.filter(llm_model_rel::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .order_by_desc(llm_model_rel::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取指定智能体已绑定的所有 LLM 模型 ID 列表
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    pub async fn list_model_ids_by_agent(&self, agent_id: i64) -> Result<Vec<i64>, DbErr> {
        let results = LlmModelRel::find()
            .filter(llm_model_rel::Column::AgentId.eq(agent_id))
            .all(self.db.db())
            .await?;

        Ok(results.into_iter().map(|r| r.model_id).collect())
    }

    /// 获取详情信息
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn info(&self, id: i64) -> Result<Option<llm_model_rel::Model>, DbErr> {
        LlmModelRel::find_by_id(id).one(self.db.db()).await
    }

    /// 更新绑定数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: llm_model_rel::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = LlmModelRel::update_many()
            .set(active_model)
            .filter(llm_model_rel::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}

/// 批量同步绑定关系（事务处理）
impl LlmModelRelDao {
    /// 批量同步智能体 LLM 模型绑定（在事务中执行新增和删除）
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `to_add` - 待新增的 ActiveModel 列表
    /// * `to_remove` - 待删除的模型 ID 列表
    pub async fn batch_save(
        &self,
        agent_id: i64,
        to_add: Vec<llm_model_rel::ActiveModel>,
        to_remove: &[i64],
    ) -> Result<(), DbErr> {
        let txn = self.db.db().begin().await?;

        // Batch insert new bindings
        self.txn_batch_create(&txn, to_add).await?;

        // Batch delete removed bindings
        self.txn_delete_by_agent_and_models(&txn, agent_id, to_remove)
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
        active_models: Vec<llm_model_rel::ActiveModel>,
    ) -> Result<(), DbErr> {
        if active_models.is_empty() {
            return Ok(());
        }
        LlmModelRel::insert_many(active_models).exec(txn).await?;
        Ok(())
    }

    /// 在事务中删除指定智能体下指定模型的绑定关系
    ///
    /// # Parameters
    ///
    /// * `txn` - 数据库事务
    /// * `agent_id` - 智能体 ID
    /// * `model_ids` - 待删除的模型 ID 列表
    async fn txn_delete_by_agent_and_models(
        &self,
        txn: &DatabaseTransaction,
        agent_id: i64,
        model_ids: &[i64],
    ) -> Result<u64, DbErr> {
        if model_ids.is_empty() {
            return Ok(0);
        }
        let result = LlmModelRel::delete_many()
            .filter(llm_model_rel::Column::AgentId.eq(agent_id))
            .filter(llm_model_rel::Column::ModelId.is_in(model_ids.to_vec()))
            .exec(txn)
            .await?;

        Ok(result.rows_affected)
    }
}
