//! 智能体 MCP 服务绑定关系数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DbErr, EntityTrait, JoinType, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, QueryTrait, Set,
};

use entity::agent::{McpServiceRel, mcp_service_registry, mcp_service_rel};
use database::Pagination;
use inject::PrimaryDb;

use crate::dto::mcp_service_rel::GetMcpServiceRelsReq;

/// 数据访问
#[injectable]
pub struct McpServiceRelDao {
    pub db: PrimaryDb,
}

impl McpServiceRelDao {
    /// 获取指定智能体的 MCP 服务绑定列表（关联注册表详情）
    ///
    /// 通过 INNER JOIN `t_agent_mcp_service_registry` 一次查询获取绑定关系及服务名称、配置等信息。
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求（agent_id 必填）
    pub async fn list(
        &self,
        req: &GetMcpServiceRelsReq,
    ) -> Result<(Vec<mcp_service_rel::McpServiceRelWithRegistry>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let base_query = McpServiceRel::find()
            .filter(mcp_service_rel::Column::AgentId.eq(req.agent_id))
            .apply_if(req.enabled, |query, v| {
                query.filter(mcp_service_rel::Column::Enabled.eq(v))
            });

        let total = base_query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = base_query
            .select_only()
            // mcp_service_rel fields
            .column(mcp_service_rel::Column::Id)
            .column(mcp_service_rel::Column::AgentId)
            .column(mcp_service_rel::Column::RegistryId)
            .column(mcp_service_rel::Column::EnabledTools)
            .column(mcp_service_rel::Column::Enabled)
            .column(mcp_service_rel::Column::CreatedAt)
            .column(mcp_service_rel::Column::UpdatedAt)
            // mcp_service_registry fields (aliased)
            .column_as(mcp_service_registry::Column::Name, "registry_name")
            .column_as(
                mcp_service_registry::Column::Description,
                "registry_description",
            )
            .column_as(
                mcp_service_registry::Column::TransportType,
                "registry_transport_type",
            )
            .column_as(
                mcp_service_registry::Column::McpServiceUrl,
                "registry_mcp_service_url",
            )
            .column_as(
                mcp_service_registry::Column::ConfigJson,
                "registry_config_json",
            )
            .column_as(
                mcp_service_registry::Column::EnabledTools,
                "registry_enabled_tools",
            )
            .column_as(mcp_service_registry::Column::Enabled, "registry_enabled")
            // JOIN mcp_service_registry
            .join(
                JoinType::InnerJoin,
                mcp_service_rel::Entity::belongs_to(mcp_service_registry::Entity)
                    .from(mcp_service_rel::Column::RegistryId)
                    .to(mcp_service_registry::Column::Id)
                    .into(),
            )
            .filter(Condition::all().add(mcp_service_registry::Column::Deleted.eq(false)))
            .order_by_desc(mcp_service_rel::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size())
            .into_model::<mcp_service_rel::McpServiceRelWithRegistry>()
            .all(self.db.db())
            .await?;

        Ok((results, total))
    }

    /// 获取详情信息
    ///
    /// # Parameters
    ///
    /// * `id` - 绑定关系 ID
    pub async fn info(&self, id: i64) -> Result<Option<mcp_service_rel::Model>, DbErr> {
        McpServiceRel::find_by_id(id).one(self.db.db()).await
    }

    /// 新增绑定关系
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待插入的 ActiveModel
    pub async fn add(
        &self,
        active_model: mcp_service_rel::ActiveModel,
    ) -> Result<mcp_service_rel::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新绑定关系
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: mcp_service_rel::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = McpServiceRel::update_many()
            .set(active_model)
            .filter(mcp_service_rel::Column::Id.eq(id))
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
        let active_model = mcp_service_rel::ActiveModel {
            id: Set(id),
            ..Default::default()
        };
        let result = McpServiceRel::delete(active_model)
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}
