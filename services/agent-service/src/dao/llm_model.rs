//! LLM 模型配置数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, Set,
};

use entity::agent::{LlmModel, llm_model};
use database::Pagination;
use inject::PrimaryDb;

use crate::dto::llm_model::GetLlmModelsReq;

/// 数据访问
#[injectable]
pub struct LlmModelDao {
    pub db: PrimaryDb,
}

impl LlmModelDao {
    /// 获取当前用户的 LLM 模型列表
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetLlmModelsReq,
    ) -> Result<(Vec<llm_model::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = LlmModel::find()
            .filter(llm_model::Column::UserId.eq(user_id))
            .filter(llm_model::Column::Deleted.eq(false))
            .apply_if(req.provider, |query, v| {
                query.filter(llm_model::Column::Provider.like(format!("{v}%")))
            })
            .apply_if(req.model_code, |query, v| {
                query.filter(llm_model::Column::ModelCode.like(format!("{v}%")))
            })
            .apply_if(req.enabled, |query, v| {
                query.filter(llm_model::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .order_by_desc(llm_model::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取 LLM 模型详情（仅返回未删除的记录）
    ///
    /// # Parameters
    ///
    /// * `id` - 模型 ID
    pub async fn info(&self, id: i64) -> Result<Option<llm_model::Model>, DbErr> {
        LlmModel::find_by_id(id)
            .filter(llm_model::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 通过用户 ID、供应商和模型编码查询（用于唯一性检查，仅查未删除记录）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 用户 ID
    /// * `provider` - 供应商
    /// * `model_code` - 模型编码
    pub async fn info_by_user_provider_code(
        &self,
        user_id: i64,
        provider: &str,
        model_code: &str,
    ) -> Result<Option<llm_model::Model>, DbErr> {
        LlmModel::find()
            .filter(llm_model::Column::UserId.eq(user_id))
            .filter(llm_model::Column::Provider.eq(provider))
            .filter(llm_model::Column::ModelCode.eq(model_code))
            .filter(llm_model::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 创建 LLM 模型
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待插入的 ActiveModel
    pub async fn create(
        &self,
        active_model: llm_model::ActiveModel,
    ) -> Result<llm_model::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新 LLM 模型
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: llm_model::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = LlmModel::update_many()
            .set(active_model)
            .filter(llm_model::Column::Id.eq(id))
            .filter(llm_model::Column::Deleted.eq(false))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }

    /// 删除 LLM 模型（软删除，标记 deleted = true）
    ///
    /// # Parameters
    ///
    /// * `id` - 模型 ID
    pub async fn delete(&self, id: i64) -> Result<u64, DbErr> {
        let active_model = llm_model::ActiveModel {
            id: Set(id),
            deleted: Set(true),
            ..Default::default()
        };
        let result = LlmModel::update_many()
            .set(active_model)
            .filter(llm_model::Column::Id.eq(id))
            .filter(llm_model::Column::Deleted.eq(false))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}
