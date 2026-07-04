use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 分页参数.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u64,
    pub page_size: u64,
}

/// 分页结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult {
    pub items: Vec<Value>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
    pub total_pages: u64,
}

/// 查询过滤器.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilter {
    pub field: String,
    pub operator: String,
    pub value: Value,
}

/// Database — 结构化数据 CRUD、事务、分页查询.
#[async_trait]
pub trait Database: Send + Sync + 'static {
    /// 插入记录.
    async fn insert(&self, ctx: LsContext, collection: &str, data: Value) -> LsResult<Value>;

    /// 按 ID 查询.
    async fn get_by_id(&self, ctx: LsContext, collection: &str, id: &str) -> LsResult<Option<Value>>;

    /// 条件查询.
    async fn query(
        &self,
        ctx: LsContext,
        collection: &str,
        filters: Vec<QueryFilter>,
        pagination: Pagination,
    ) -> LsResult<PaginatedResult>;

    /// 更新记录.
    async fn update(&self, ctx: LsContext, collection: &str, id: &str, data: Value) -> LsResult<Option<Value>>;

    /// 删除记录.
    async fn delete(&self, ctx: LsContext, collection: &str, id: &str) -> LsResult<bool>;

    /// 开启事务.
    async fn begin_transaction(&self, ctx: LsContext) -> LsResult<String>;

    /// 提交事务.
    async fn commit_transaction(&self, ctx: LsContext, txn_id: &str) -> LsResult<()>;

    /// 回滚事务.
    async fn rollback_transaction(&self, ctx: LsContext, txn_id: &str) -> LsResult<()>;
}
