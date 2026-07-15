//! 工具注册表数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait,
};

use entity::agent::{ToolRegistry, tool_registry};
use database::Pagination;
use inject::PrimaryDb;

use crate::dto::tool_registry::GetToolRegistriesReq;

/// 数据访问
#[injectable]
pub struct ToolRegistryDao {
    pub db: PrimaryDb,
}

impl ToolRegistryDao {
    /// 获取所有数据
    pub async fn all(&self) -> Result<(Vec<tool_registry::Model>, u64), DbErr> {
        let results = ToolRegistry::find()
            .order_by_asc(tool_registry::Column::Id)
            .all(self.db.db())
            .await?;
        let total = results.len() as u64;
        Ok((results, total))
    }

    /// 获取数据列表
    ///
    /// # Parameters
    ///
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        req: GetToolRegistriesReq,
    ) -> Result<(Vec<tool_registry::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let states = ToolRegistry::find()
            .apply_if(req.start_time, |query, v| {
                query.filter(tool_registry::Column::CreatedAt.gte(v))
            })
            .apply_if(req.end_time, |query, v| {
                query.filter(tool_registry::Column::CreatedAt.lt(v))
            })
            .apply_if(req.name, |query, v| {
                query.filter(tool_registry::Column::Name.like(format!("{v}%")))
            })
            .apply_if(req.enabled, |query, v| {
                query.filter(tool_registry::Column::Enabled.eq(v))
            });

        let total = states.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = states
            .order_by_desc(tool_registry::Column::Id)
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
    /// * `id` - 工具 ID
    pub async fn info(&self, id: i64) -> Result<Option<tool_registry::Model>, DbErr> {
        ToolRegistry::find_by_id(id).one(self.db.db()).await
    }

    /// 通过工具编码获取详情信息
    ///
    /// # Parameters
    ///
    /// * `tool_code` - 工具唯一编码
    pub async fn info_by_tool_code(
        &self,
        tool_code: String,
    ) -> Result<Option<tool_registry::Model>, DbErr> {
        ToolRegistry::find()
            .filter(tool_registry::Column::ToolCode.eq(tool_code))
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
        active_model: tool_registry::ActiveModel,
    ) -> Result<tool_registry::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: tool_registry::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = ToolRegistry::update_many()
            .set(active_model)
            .filter(tool_registry::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }

    /// 按主键删除信息（物理删除）
    ///
    /// # Parameters
    ///
    /// * `id` - 工具 ID
    pub async fn delete(&self, id: i64) -> Result<u64, DbErr> {
        let result = ToolRegistry::delete_by_id(id).exec(self.db.db()).await?;
        Ok(result.rows_affected)
    }
}
