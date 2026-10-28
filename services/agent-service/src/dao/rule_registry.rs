//! 规则注册表数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, Set,
};

use database::Pagination;
use entity::agent::{RuleRegistry, rule_registry};
use inject::PrimaryDb;

use crate::dto::rule_registry::GetRuleRegistriesReq;

/// 数据访问
#[injectable]
pub struct RuleRegistryDao {
    pub db: PrimaryDb,
}

impl RuleRegistryDao {
    /// 获取数据列表（按用户过滤）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetRuleRegistriesReq,
    ) -> Result<(Vec<rule_registry::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let states = RuleRegistry::find()
            .filter(rule_registry::Column::UserId.eq(user_id))
            .filter(rule_registry::Column::Deleted.eq(false))
            .apply_if(req.start_time, |query, v| {
                query.filter(rule_registry::Column::CreatedAt.gte(v))
            })
            .apply_if(req.end_time, |query, v| {
                query.filter(rule_registry::Column::CreatedAt.lt(v))
            })
            .apply_if(req.name, |query, v| {
                query.filter(rule_registry::Column::Name.like(format!("{v}%")))
            })
            .apply_if(req.enabled, |query, v| {
                query.filter(rule_registry::Column::Enabled.eq(v))
            });

        let total = states.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = states
            .order_by_desc(rule_registry::Column::CreatedAt)
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
    /// * `id` - 规则 ID
    pub async fn info(&self, id: i64) -> Result<Option<rule_registry::Model>, DbErr> {
        RuleRegistry::find_by_id(id)
            .filter(rule_registry::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 通过名称获取详情信息（用户维度唯一性）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `name` - 规则名称
    pub async fn info_by_name(
        &self,
        user_id: i64,
        name: String,
    ) -> Result<Option<rule_registry::Model>, DbErr> {
        RuleRegistry::find()
            .filter(rule_registry::Column::UserId.eq(user_id))
            .filter(rule_registry::Column::Name.eq(name))
            .filter(rule_registry::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 通过 ID 列表批量查询规则信息
    ///
    /// # Parameters
    ///
    /// * `ids` - 规则 ID 列表
    pub async fn list_by_ids(&self, ids: &[i64]) -> Result<Vec<rule_registry::Model>, DbErr> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        RuleRegistry::find()
            .filter(rule_registry::Column::Id.is_in(ids.to_vec()))
            .filter(rule_registry::Column::Deleted.eq(false))
            .all(self.db.db())
            .await
    }

    /// 添加详情信息
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待插入的 ActiveModel
    pub async fn create(
        &self,
        active_model: rule_registry::ActiveModel,
    ) -> Result<rule_registry::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: rule_registry::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = RuleRegistry::update_many()
            .set(active_model)
            .filter(rule_registry::Column::Id.eq(id))
            .filter(rule_registry::Column::Deleted.eq(false))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }

    /// 软删除规则
    ///
    /// # Parameters
    ///
    /// * `id` - 规则 ID
    pub async fn delete(&self, id: i64) -> Result<u64, DbErr> {
        let active_model = rule_registry::ActiveModel {
            id: Set(id),
            deleted: Set(true),
            ..Default::default()
        };
        let result = RuleRegistry::update_many()
            .set(active_model)
            .filter(rule_registry::Column::Id.eq(id))
            .filter(rule_registry::Column::Deleted.eq(false))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}
