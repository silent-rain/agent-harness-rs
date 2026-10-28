//! 智能体技能安装包数据访问层

use nject::injectable;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait,
};

use database::Pagination;
use entity::agent::{SkillPackage as SkillPackageEntity, skill_package};
use inject::PrimaryDb;

use crate::dto::skill_package::GetSkillPackagesReq;

/// 技能安装包数据访问对象
///
/// 提供面向 HTTP 接口的 CRUD 操作，按用户维度隔离数据。
#[injectable]
pub struct SkillPackageDao {
    pub db: PrimaryDb,
}

impl SkillPackageDao {
    /// 获取数据列表（按用户过滤）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `req` - 查询请求
    pub async fn list(
        &self,
        user_id: i64,
        req: GetSkillPackagesReq,
    ) -> Result<(Vec<skill_package::Model>, u64), DbErr> {
        let page = Pagination::new(req.page, req.page_size);

        let query = SkillPackageEntity::find()
            .filter(skill_package::Column::Deleted.eq(false))
            .filter(skill_package::Column::UserId.eq(user_id))
            .apply_if(req.skill_code, |q, v| {
                q.filter(skill_package::Column::SkillCode.like(format!("{v}%")))
            })
            .apply_if(req.name, |q, v| {
                q.filter(skill_package::Column::Name.like(format!("{v}%")))
            })
            .apply_if(req.status, |q, v| {
                q.filter(skill_package::Column::Status.eq(i16::from(v)))
            });

        let total = query.clone().count(self.db.db()).await?;
        if total == 0 {
            return Ok((vec![], total));
        }

        let results = query
            .order_by_desc(skill_package::Column::CreatedAt)
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
    /// * `id` - 安装包 ID
    pub async fn info(&self, id: i64) -> Result<Option<skill_package::Model>, DbErr> {
        SkillPackageEntity::find_by_id(id)
            .filter(skill_package::Column::Deleted.eq(false))
            .one(self.db.db())
            .await
    }

    /// 通过 `skill_code` 获取详情（用户维度唯一性检查）
    ///
    /// # Parameters
    ///
    /// * `user_id` - 当前用户 ID
    /// * `skill_code` - 技能编码
    pub async fn info_by_code(
        &self,
        user_id: i64,
        skill_code: &str,
    ) -> Result<Option<skill_package::Model>, DbErr> {
        SkillPackageEntity::find()
            .filter(skill_package::Column::Deleted.eq(false))
            .filter(skill_package::Column::UserId.eq(user_id))
            .filter(skill_package::Column::SkillCode.eq(skill_code))
            .one(self.db.db())
            .await
    }

    /// 添加数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待插入的 ActiveModel
    pub async fn create(
        &self,
        active_model: skill_package::ActiveModel,
    ) -> Result<skill_package::Model, DbErr> {
        active_model.insert(self.db.db()).await
    }

    /// 更新数据
    ///
    /// # Parameters
    ///
    /// * `active_model` - 待更新的 ActiveModel
    pub async fn update(&self, active_model: skill_package::ActiveModel) -> Result<u64, DbErr> {
        let id: i64 = *(active_model.id.clone().as_ref());
        let result = SkillPackageEntity::update_many()
            .filter(skill_package::Column::Deleted.eq(false))
            .set(active_model)
            .filter(skill_package::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }

    /// 物理删除安装包
    ///
    /// # Parameters
    ///
    /// * `id` - 安装包 ID
    pub async fn delete(&self, id: i64) -> Result<u64, DbErr> {
        let result = SkillPackageEntity::delete_many()
            .filter(skill_package::Column::Deleted.eq(false))
            .filter(skill_package::Column::Id.eq(id))
            .exec(self.db.db())
            .await?;

        Ok(result.rows_affected)
    }
}
