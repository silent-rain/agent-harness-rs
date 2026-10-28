//! A2A 服务注册表数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, Set,
};

use database::Pagination;
use entity::agent::{A2aServiceRegistry, a2a_service_registry};
use inject::PrimaryDb;

use crate::dto::a2a_service_registry::GetA2aServiceRegistriesReq;

/// 数据访问
#[injectable]
pub struct A2aServiceRegistryDao {
    pub db: PrimaryDb,
}

impl A2aServiceRegistryDao {
    /// 获取数据列表（按用户过滤）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetA2aServiceRegistriesReq,
    ) -> Result<(Vec<a2a_service_registry::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let states = A2aServiceRegistry::find()
            .filter(a2a_service_registry::Column::UserId.eq(user_id))
            .filter(a2a_service_registry::Column::Deleted.eq(false))
            .apply_if(req.start_time, |query, v| {
                query.filter(a2a_service_registry::Column::CreatedAt.gte(v))
            })
            .apply_if(req.end_time, |query, v| {
                query.filter(a2a_service_registry::Column::CreatedAt.lt(v))
            })
            .apply_if(req.name, |query, v| {
                query.filter(a2a_service_registry::Column::Name.like(format!("{v}%")))
            })
            .apply_if(req.enabled, |query, v| {
                query.filter(a2a_service_registry::Column::Enabled.eq(v))
            });

        let total = states.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = states
            .order_by_desc(a2a_service_registry::Column::CreatedAt)
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
    /// * `id` - 注册表 ID
    pub async fn info(&self, id: i64) -> Result<Option<a2a_service_registry::Model>, DbErr> {
        A2aServiceRegistry::find_by_id(id)
            .filter(a2a_service_registry::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 通过名称获取详情信息（用户维度唯一性）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `name` - 服务名称
    pub async fn info_by_name(
        &self,
        user_id: i64,
        name: String,
    ) -> Result<Option<a2a_service_registry::Model>, DbErr> {
        A2aServiceRegistry::find()
            .filter(a2a_service_registry::Column::UserId.eq(user_id))
            .filter(a2a_service_registry::Column::Name.eq(name))
            .filter(a2a_service_registry::Column::Deleted.eq(false))
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
        active_model: a2a_service_registry::ActiveModel,
    ) -> Result<a2a_service_registry::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(
        &self,
        active_model: a2a_service_registry::ActiveModel,
    ) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = A2aServiceRegistry::update_many()
            .set(active_model)
            .filter(a2a_service_registry::Column::Id.eq(id))
            .filter(a2a_service_registry::Column::Deleted.eq(false))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }

    /// 软删除 A2A 服务注册表
    ///
    /// # Parameters
    ///
    /// * `id` - 注册表 ID
    pub async fn delete(&self, id: i64) -> Result<u64, DbErr> {
        let active_model = a2a_service_registry::ActiveModel {
            id: Set(id),
            deleted: Set(true),
            ..Default::default()
        };
        let result = A2aServiceRegistry::update_many()
            .set(active_model)
            .filter(a2a_service_registry::Column::Id.eq(id))
            .filter(a2a_service_registry::Column::Deleted.eq(false))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}
