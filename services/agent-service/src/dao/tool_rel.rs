//! 智能体工具绑定关系数据访问层

use nject::injectable;
use sea_orm::{
    ColumnTrait, DatabaseTransaction, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, TransactionTrait,
};

use entity::agent::{ToolRel, tool_rel};
use database::Pagination;
use inject::PrimaryDb;

use crate::dto::tool_rel::GetToolRelsReq;

/// 数据访问
#[injectable]
pub struct ToolRelDao {
    pub db: PrimaryDb,
}

impl ToolRelDao {
    /// 获取指定智能体的工具绑定列表
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求（agent_id 必填）
    pub async fn list(&self, req: &GetToolRelsReq) -> Result<(Vec<tool_rel::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = ToolRel::find()
            .filter(tool_rel::Column::AgentId.eq(req.agent_id))
            .apply_if(req.enabled, |query, v| {
                query.filter(tool_rel::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .order_by_asc(tool_rel::Column::Priority)
            .order_by_desc(tool_rel::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取指定智能体已绑定的所有工具 ID 列表
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    pub async fn list_tool_ids_by_agent(&self, agent_id: i64) -> Result<Vec<i64>, DbErr> {
        let results = ToolRel::find()
            .filter(tool_rel::Column::AgentId.eq(agent_id))
            .all(self.db.db())
            .await?;

        Ok(results.into_iter().map(|r| r.tool_id).collect())
    }

    /// 获取详情信息
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn info(&self, id: i64) -> Result<Option<tool_rel::Model>, DbErr> {
        ToolRel::find_by_id(id).one(self.db.db()).await
    }

    /// 通过智能体 ID 和工具 ID 查询绑定关系
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `tool_id` - 工具 ID
    pub async fn info_by_agent_and_tool(
        &self,
        agent_id: i64,
        tool_id: i64,
    ) -> Result<Option<tool_rel::Model>, DbErr> {
        ToolRel::find()
            .filter(tool_rel::Column::AgentId.eq(agent_id))
            .filter(tool_rel::Column::ToolId.eq(tool_id))
            .one(self.db.db())
            .await
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: tool_rel::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = ToolRel::update_many()
            .set(active_model)
            .filter(tool_rel::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}

/// 批量同步绑定关系（事务处理）
impl ToolRelDao {
    /// 批量同步智能体工具绑定（在事务中执行新增和删除）
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `to_add` - 待新增的 ActiveModel 列表
    /// * `to_remove` - 待删除的工具 ID 列表
    pub async fn batch_save(
        &self,
        agent_id: i64,
        to_add: Vec<tool_rel::ActiveModel>,
        to_remove: &[i64],
    ) -> Result<(), DbErr> {
        let txn = self.db.db().begin().await?;

        // Batch insert new bindings
        self.txn_batch_create(&txn, to_add).await?;

        // Batch delete removed bindings
        self.txn_delete_by_agent_and_tools(&txn, agent_id, to_remove)
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
        active_models: Vec<tool_rel::ActiveModel>,
    ) -> Result<(), DbErr> {
        if active_models.is_empty() {
            return Ok(());
        }
        ToolRel::insert_many(active_models).exec(txn).await?;
        Ok(())
    }

    /// 在事务中删除指定智能体下指定工具的绑定关系
    ///
    /// # Parameters
    ///
    /// * `txn` - 数据库事务
    /// * `agent_id` - 智能体 ID
    /// * `tool_ids` - 待删除的工具 ID 列表
    async fn txn_delete_by_agent_and_tools(
        &self,
        txn: &DatabaseTransaction,
        agent_id: i64,
        tool_ids: &[i64],
    ) -> Result<u64, DbErr> {
        if tool_ids.is_empty() {
            return Ok(0);
        }
        let result = ToolRel::delete_many()
            .filter(tool_rel::Column::AgentId.eq(agent_id))
            .filter(tool_rel::Column::ToolId.is_in(tool_ids.to_vec()))
            .exec(txn)
            .await?;

        Ok(result.rows_affected)
    }
}
