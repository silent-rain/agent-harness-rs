//! 智能体命令绑定关系数据访问层

use nject::injectable;
use sea_orm::{
    ColumnTrait, Condition, DatabaseTransaction, DbErr, EntityTrait, JoinType, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, QueryTrait, TransactionTrait,
};

use database::Pagination;
use entity::agent::{CommandRel, command_registry, command_rel};
use inject::PrimaryDb;

use crate::dto::command_rel::GetCommandRelsReq;

/// 数据访问
#[injectable]
pub struct CommandRelDao {
    pub db: PrimaryDb,
}

impl CommandRelDao {
    /// 获取指定智能体的命令绑定列表（关联命令详情）
    ///
    /// 通过 INNER JOIN `t_agent_command_registry` 一次查询获取绑定关系及命令名称、描述、启用状态。
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求（agent_id 必填）
    pub async fn list(
        &self,
        req: &GetCommandRelsReq,
    ) -> Result<(Vec<command_rel::CommandRelWithCommand>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = CommandRel::find()
            .filter(command_rel::Column::AgentId.eq(req.agent_id))
            .apply_if(req.enabled, |query, v| {
                query.filter(command_rel::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .select_only()
            // command_rel fields
            .column(command_rel::Column::Id)
            .column(command_rel::Column::AgentId)
            .column(command_rel::Column::CommandId)
            .column(command_rel::Column::Priority)
            .column(command_rel::Column::Enabled)
            .column(command_rel::Column::CreatedAt)
            .column(command_rel::Column::UpdatedAt)
            // command_registry fields (aliased)
            .column_as(command_registry::Column::Name, "command_name")
            .column_as(command_registry::Column::Description, "command_description")
            .column_as(command_registry::Column::Enabled, "command_enabled")
            // JOIN command_registry
            .join(
                JoinType::InnerJoin,
                command_rel::Entity::belongs_to(command_registry::Entity)
                    .from(command_rel::Column::CommandId)
                    .to(command_registry::Column::Id)
                    .into(),
            )
            .filter(Condition::all().add(command_registry::Column::Deleted.eq(false)))
            .order_by_asc(command_rel::Column::Priority)
            .order_by_desc(command_rel::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .into_model::<command_rel::CommandRelWithCommand>()
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取指定智能体已绑定的所有命令 ID 列表
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    pub async fn list_command_ids_by_agent(&self, agent_id: i64) -> Result<Vec<i64>, DbErr> {
        let results = CommandRel::find()
            .filter(command_rel::Column::AgentId.eq(agent_id))
            .all(self.db.db())
            .await?;

        Ok(results.into_iter().map(|r| r.command_id).collect())
    }

    /// 获取详情信息
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn info(&self, id: i64) -> Result<Option<command_rel::Model>, DbErr> {
        CommandRel::find_by_id(id).one(self.db.db()).await
    }

    /// 通过智能体 ID 和命令 ID 查询绑定关系
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `command_id` - 命令 ID
    pub async fn info_by_agent_and_command(
        &self,
        agent_id: i64,
        command_id: i64,
    ) -> Result<Option<command_rel::Model>, DbErr> {
        CommandRel::find()
            .filter(command_rel::Column::AgentId.eq(agent_id))
            .filter(command_rel::Column::CommandId.eq(command_id))
            .one(self.db.db())
            .await
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: command_rel::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = CommandRel::update_many()
            .set(active_model)
            .filter(command_rel::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}

/// 批量同步绑定关系（事务处理）
impl CommandRelDao {
    /// 批量同步智能体命令绑定（在事务中执行新增和删除）
    ///
    /// # Parameters
    ///
    /// * `agent_id` - 智能体 ID
    /// * `to_add` - 待新增的 ActiveModel 列表
    /// * `to_remove` - 待删除的命令 ID 列表
    pub async fn batch_save(
        &self,
        agent_id: i64,
        to_add: Vec<command_rel::ActiveModel>,
        to_remove: &[i64],
    ) -> Result<(), DbErr> {
        let txn = self.db.db().begin().await?;

        // Batch insert new bindings
        self.txn_batch_create(&txn, to_add).await?;

        // Batch delete removed bindings
        self.txn_delete_by_agent_and_commands(&txn, agent_id, to_remove)
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
        active_models: Vec<command_rel::ActiveModel>,
    ) -> Result<(), DbErr> {
        if active_models.is_empty() {
            return Ok(());
        }
        CommandRel::insert_many(active_models).exec(txn).await?;
        Ok(())
    }

    /// 在事务中删除指定智能体下指定命令的绑定关系
    ///
    /// # Parameters
    ///
    /// * `txn` - 数据库事务
    /// * `agent_id` - 智能体 ID
    /// * `command_ids` - 待删除的命令 ID 列表
    async fn txn_delete_by_agent_and_commands(
        &self,
        txn: &DatabaseTransaction,
        agent_id: i64,
        command_ids: &[i64],
    ) -> Result<u64, DbErr> {
        if command_ids.is_empty() {
            return Ok(0);
        }
        let result = CommandRel::delete_many()
            .filter(command_rel::Column::AgentId.eq(agent_id))
            .filter(command_rel::Column::CommandId.is_in(command_ids.to_vec()))
            .exec(txn)
            .await?;

        Ok(result.rows_affected)
    }
}
