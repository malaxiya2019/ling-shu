use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

use crate::database::{PaginatedResult, Pagination, QueryFilter};

/// Repository<T> — 业务仓储层通用模板、数据访问抽象.
///
/// 为实体类型 T 提供类型安全的 CRUD 操作。
/// T 必须满足序列化/反序列化以及 'static 约束。
#[async_trait]
pub trait Repository<T>: Send + Sync + 'static
where
    T: Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    /// 集合/表名.
    fn collection_name(&self) -> &str;

    /// 插入实体.
    async fn insert(&self, ctx: LsContext, entity: T) -> LsResult<T>;

    /// 按 ID 查询.
    async fn get_by_id(&self, ctx: LsContext, id: &str) -> LsResult<Option<T>>;

    /// 条件查询.
    async fn query(
        &self,
        ctx: LsContext,
        filters: Vec<QueryFilter>,
        pagination: Pagination,
    ) -> LsResult<PaginatedResult>;

    /// 更新实体.
    async fn update(&self, ctx: LsContext, id: &str, entity: T) -> LsResult<Option<T>>;

    /// 删除实体.
    async fn delete(&self, ctx: LsContext, id: &str) -> LsResult<bool>;
}

/// 基于 Database trait 的仓储默认实现骨架.
pub struct DatabaseRepository<T> {
    collection: String,
    _marker: PhantomData<T>,
}

impl<T> DatabaseRepository<T> {
    pub fn new(collection: impl Into<String>) -> Self {
        Self {
            collection: collection.into(),
            _marker: PhantomData,
        }
    }
}

#[async_trait]
impl<T> Repository<T> for DatabaseRepository<T>
where
    T: Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    fn collection_name(&self) -> &str {
        &self.collection
    }

    async fn insert(&self, ctx: LsContext, entity: T) -> LsResult<T> {
        let value = serde_json::to_value(&entity)?;
        let _ = (ctx, value);
        Err(lingshu_core::LsError::NotImplemented(
            "DatabaseRepository::insert — requires a Database backend".into(),
        ))
    }

    async fn get_by_id(&self, _ctx: LsContext, _id: &str) -> LsResult<Option<T>> {
        Err(lingshu_core::LsError::NotImplemented(
            "DatabaseRepository::get_by_id — requires a Database backend".into(),
        ))
    }

    async fn query(
        &self,
        _ctx: LsContext,
        _filters: Vec<QueryFilter>,
        _pagination: Pagination,
    ) -> LsResult<PaginatedResult> {
        Err(lingshu_core::LsError::NotImplemented(
            "DatabaseRepository::query — requires a Database backend".into(),
        ))
    }

    async fn update(&self, _ctx: LsContext, _id: &str, _entity: T) -> LsResult<Option<T>> {
        Err(lingshu_core::LsError::NotImplemented(
            "DatabaseRepository::update — requires a Database backend".into(),
        ))
    }

    async fn delete(&self, _ctx: LsContext, _id: &str) -> LsResult<bool> {
        Err(lingshu_core::LsError::NotImplemented(
            "DatabaseRepository::delete — requires a Database backend".into(),
        ))
    }
}
