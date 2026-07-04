//! OpenAI Embedding 后端 — 对接 Embeddings API.

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::embedding::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
    usage: EmbedUsageResp,
    model: String,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f64>,
}

#[derive(Deserialize)]
struct EmbedUsageResp {
    total_tokens: u64,
}

pub struct OpenAiEmbedding {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    dimensions: usize,
    total_tokens: AtomicU64,
}

impl OpenAiEmbedding {
    /// 创建 OpenAI Embedding 实例.
    ///
    /// 默认模型: "text-embedding-3-small" (1536 维).
    pub fn new(
        api_key: impl Into<String>,
        model: Option<String>,
        dimensions: Option<usize>,
        base_url: Option<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".into()),
            model: model.unwrap_or_else(|| "text-embedding-3-small".into()),
            dimensions: dimensions.unwrap_or(1536),
            total_tokens: AtomicU64::new(0),
        }
    }

    fn embed_url(&self) -> String {
        format!("{}/embeddings", self.base_url)
    }
}

#[async_trait]
impl Embedding for OpenAiEmbedding {
    async fn embed(&self, _ctx: LsContext, request: EmbeddingRequest) -> LsResult<EmbeddingResponse> {
        let req_body = EmbedRequest {
            model: request.model.unwrap_or_else(|| self.model.clone()),
            input: request.input,
        };

        let resp = self.client
            .post(self.embed_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req_body)
            .send()
            .await
            .map_err(|e| LsError::Embedding(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(LsError::Embedding(format!("API error {status}: {body}")));
        }

        let embed_resp: EmbedResponse = resp
            .json()
            .await
            .map_err(|e| LsError::Embedding(format!("parse failed: {e}")))?;

        self.total_tokens.fetch_add(embed_resp.usage.total_tokens, Ordering::AcqRel);

        let vectors: Vec<EmbeddingVector> = embed_resp.data.into_iter().map(|d| {
            EmbeddingVector {
                dimensions: d.embedding.len(),
                values: d.embedding.into_iter().map(|v| v as f32).collect(),
            }
        }).collect();

        Ok(EmbeddingResponse {
            vectors,
            model: embed_resp.model,
            usage: EmbeddingUsage {
                total_tokens: embed_resp.usage.total_tokens,
            },
        })
    }

    fn validate_dimensions(&self, vector: &EmbeddingVector) -> LsResult<()> {
        if vector.dimensions != self.dimensions {
            return Err(LsError::Embedding(format!(
                "expected {} dimensions, got {}",
                self.dimensions, vector.dimensions
            )));
        }
        if vector.values.len() != vector.dimensions {
            return Err(LsError::Embedding(format!(
                "values length {} != dimensions {}",
                vector.values.len(), vector.dimensions
            )));
        }
        Ok(())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_dimensions() {
        let emb = OpenAiEmbedding::new("sk-test", None, Some(3), None);
        let v = EmbeddingVector { dimensions: 3, values: vec![0.1, 0.2, 0.3] };
        assert!(emb.validate_dimensions(&v).is_ok());
    }

    #[tokio::test]
    async fn test_validate_dimensions_mismatch() {
        let emb = OpenAiEmbedding::new("sk-test", None, Some(3), None);
        let v = EmbeddingVector { dimensions: 5, values: vec![0.1; 5] };
        assert!(emb.validate_dimensions(&v).is_err());
    }
}
