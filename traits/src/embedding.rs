use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};

/// Embedding 向量.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingVector {
    pub dimensions: usize,
    pub values: Vec<f32>,
}

/// Embedding 请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub input: Vec<String>,
    pub model: Option<String>,
}

/// Embedding 响应.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub vectors: Vec<EmbeddingVector>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

/// Embedding 用量.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    pub total_tokens: u64,
}

/// Embedding — 向量生成、维度校验、批量处理.
#[async_trait]
pub trait Embedding: Send + Sync + 'static {
    /// 生成向量.
    async fn embed(&self, ctx: LsContext, request: EmbeddingRequest)
        -> LsResult<EmbeddingResponse>;

    /// 校验向量维度.
    fn validate_dimensions(&self, vector: &EmbeddingVector) -> LsResult<()>;

    /// 查询模型维度.
    fn dimensions(&self) -> usize;
}
