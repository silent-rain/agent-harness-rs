//! 智能体规则绑定关系数据访问层

use nject::injectable;
use sea_orm::{
    ColumnTrait, Condition, DatabaseTransaction, DbErr, EntityTrait, JoinType, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, QueryTrait, TransactionTrait,
};

use database::Pagination;
use entity::agent::{RuleRel, rule_registry, rule_rel};
use inject::PrimaryDb;

use crate::dto::rule_rel::GetRuleRelsReq;

/// 数据访问
#[injectable]
pub struct RuleRelDao {
    pub db: PrimaryDb,
}

impl RuleRelDao {
    /// 获取指定智能体的规则绑定列表（关联规则详情）
    ///
    /// 通过 INNER JOIN `t_agent_rule_registry` 一次查询获取绑定关系及规则名称、描述、启用状态。
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求（agent_id 必填）
    pub async fn list(
        &self,
        req: &GetRuleRelsReq,
    ) -> Result<(Vec<rule_rel::RuleRelWithRule>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = RuleRel::find()
            .filter(rule_rel::Column::AgentId.eq(req.agent_id))
            .apply_if(req.enabled, |query, v| {
                query.filter(rule_rel::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .select_only()
            // rule_rel fields
            .column(rule_rel::Column::Id)
            .column(rule_rel::Column::AgentId)
            .column(rule_rel::Column::RuleId)
            .column(rule_rel::Column::Priority)
            .column(rule_rel::Column::Enabled)
            .column(rule_rel::Column::CreatedAt)
            .column(rule_rel::Column::UpdatedAt)
            // rule_registry fields (aliased)
            .column_as(rule_registry::Column::Name, "rule_name")
            .column_as(rule_registry::Column::Description, "rule_description")
            .column_as(rule_registry::Column::Enabled, "rule_enabled")
            // JOIN rule_registry
            .join(
                JoinType::InnerJoin,
                rule_rel::Entity::belongs_to(rule_registry::Entity)
                    .from(rule_rel::Column::RuleId)
                    .to(rule_registry::Column::Id)
                    .into(),
            )
            .filter(Condition::all().add(rule_registry::Column::Deleted.eq(false)))
            .order_by_asc(rule_rel::Column::Priority)
            .order_by_desc(rule_rel::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .into_model::<rule_rel::RuleRelWithRule>()
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取指定智能体已绑定的所有规则 ID 列表
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    pub async fn list_rule_ids_by_agent(&self, agent_id: i64) -> Result<Vec<i64>, DbErr> {
        let results = RuleRel::find()
            .filter(rule_rel::Column::AgentId.eq(agent_id))
            .all(self.db.db())
            .await?;

        Ok(results.into_iter().map(|r| r.rule_id).collect())
    }

    /// 获取详情信息
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn info(&self, id: i64) -> Result<Option<rule_rel::Model>, DbErr> {
        RuleRel::find_by_id(id).one(self.db.db()).await
    }

    /// 通过智能体 ID 和规则 ID 查询绑定关系
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `rule_id` - 规则 ID
    pub async fn info_by_agent_and_rule(
        &self,
        agent_id: i64,
        rule_id: i64,
    ) -> Result<Option<rule_rel::Model>, DbErr> {
        RuleRel::find()
            .filter(rule_rel::Column::AgentId.eq(agent_id))
            .filter(rule_rel::Column::RuleId.eq(rule_id))
            .one(self.db.db())
            .await
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: rule_rel::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = RuleRel::update_many()
            .set(active_model)
            .filter(rule_rel::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}

/// 批量同步绑定关系（事务处理）
impl RuleRelDao {
    /// 批量同步智能体规则绑定（在事务中执行新增和删除）
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `to_add` - 待新增的 ActiveModel 列表
    /// * `to_remove` - 待删除的规则 ID 列表
    pub async fn batch_save(
        &self,
        agent_id: i64,
        to_add: Vec<rule_rel::ActiveModel>,
        to_remove: &[i64],
    ) -> Result<(), DbErr> {
        let txn = self.db.db().begin().await?;

        // Batch insert new bindings
        self.txn_batch_create(&txn, to_add).await?;

        // Batch delete removed bindings
        self.txn_delete_by_agent_and_rules(&txn, agent_id, to_remove)
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
        active_models: Vec<rule_rel::ActiveModel>,
    ) -> Result<(), DbErr> {
        if active_models.is_empty() {
            return Ok(());
        }
        RuleRel::insert_many(active_models).exec(txn).await?;
        Ok(())
    }

    /// 在事务中删除指定智能体下指定规则的绑定关系
    ///
    /// # Parameters
    ///
    /// * `txn` - 数据库事务
    /// * `agent_id` - 智能体 ID
    /// * `rule_ids` - 待删除的规则 ID 列表
    async fn txn_delete_by_agent_and_rules(
        &self,
        txn: &DatabaseTransaction,
        agent_id: i64,
        rule_ids: &[i64],
    ) -> Result<u64, DbErr> {
        if rule_ids.is_empty() {
            return Ok(0);
        }
        let result = RuleRel::delete_many()
            .filter(rule_rel::Column::AgentId.eq(agent_id))
            .filter(rule_rel::Column::RuleId.is_in(rule_ids.to_vec()))
            .exec(txn)
            .await?;

        Ok(result.rows_affected)
    }
}
