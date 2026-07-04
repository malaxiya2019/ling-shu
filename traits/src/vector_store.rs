use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 向量集合配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorCollection {
    pub collection_id: LsId,
    pub name: String,
    pub dimensions: usize,
    pub metadata: Value,
}

/// 向量记录.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: LsId,
    pub vector: Vec<f32>,
    pub metadata: Value,
    pub score: Option<f64>,
}

/// 相似度检索结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub records: Vec<VectorRecord>,
    pub total: u64,
}

/// VectorStore — 集合管理、向量写入、相似度检索.
#[async_trait]
pub trait VectorStore: Send + Sync + 'static {
    /// 创建集合.
    async fn create_collection(&self, ctx: LsContext, name: &str, dimensions: usize) -> LsResult<LsId>;

    /// 删除集合.
    async fn delete_collection(&self, ctx: LsContext, collection_id: LsId) -> LsResult<()>;

    /// 写入向量.
    async fn upsert(&self, ctx: LsContext, collection_id: LsId, records: Vec<VectorRecord>) -> LsResult<()>;

    /// 相似度检索.
    async fn search(&self, ctx: LsContext, collection_id: LsId, query: Vec<f32>, top_k: u64) -> LsResult<VectorSearchResult>;
}
