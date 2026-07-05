//! 内存向量存储 — 基于余弦相似度的纯内存实现.

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::vector_store::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 集合内部表示.
#[allow(dead_code)]
struct CollectionData {
    collection_id: LsId,
    name: String,
    dimensions: usize,
    records: Vec<InternalRecord>,
}

struct InternalRecord {
    id: LsId,
    vector: Vec<f32>,
    metadata: Value,
}

/// 内存向量存储.
pub struct InMemoryVectorStore {
    collections: Arc<RwLock<HashMap<LsId, CollectionData>>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            collections: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryVectorStore {
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        (dot / (norm_a * norm_b)) as f64
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn create_collection(
        &self,
        _ctx: LsContext,
        name: &str,
        dimensions: usize,
    ) -> LsResult<LsId> {
        let id = LsId::new();
        let mut collections = self.collections.write().await;
        collections.insert(
            id,
            CollectionData {
                collection_id: id,
                name: name.to_string(),
                dimensions,
                records: Vec::new(),
            },
        );
        Ok(id)
    }

    async fn delete_collection(&self, _ctx: LsContext, collection_id: LsId) -> LsResult<()> {
        self.collections.write().await.remove(&collection_id);
        Ok(())
    }

    async fn upsert(
        &self,
        _ctx: LsContext,
        collection_id: LsId,
        records: Vec<VectorRecord>,
    ) -> LsResult<()> {
        let mut collections = self.collections.write().await;
        let col = collections.get_mut(&collection_id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("collection {collection_id}"))
        })?;

        for rec in records {
            if rec.vector.len() != col.dimensions {
                return Err(lingshu_core::LsError::InvalidArgument(format!(
                    "vector dimension {} != {}",
                    rec.vector.len(),
                    col.dimensions
                )));
            }
            if let Some(existing) = col.records.iter_mut().find(|r| r.id == rec.id) {
                existing.vector = rec.vector;
                existing.metadata = rec.metadata;
            } else {
                col.records.push(InternalRecord {
                    id: rec.id,
                    vector: rec.vector,
                    metadata: rec.metadata,
                });
            }
        }
        Ok(())
    }

    async fn search(
        &self,
        _ctx: LsContext,
        collection_id: LsId,
        query: Vec<f32>,
        top_k: u64,
    ) -> LsResult<VectorSearchResult> {
        let collections = self.collections.read().await;
        let col = collections.get(&collection_id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("collection {collection_id}"))
        })?;

        if query.len() != col.dimensions {
            return Err(lingshu_core::LsError::InvalidArgument(format!(
                "query dim {} != {}",
                query.len(),
                col.dimensions
            )));
        }

        let mut scored: Vec<(f64, &InternalRecord)> = col
            .records
            .iter()
            .map(|r| (Self::cosine_similarity(&query, &r.vector), r))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let total = scored.len() as u64;
        let records: Vec<VectorRecord> = scored
            .into_iter()
            .take(top_k as usize)
            .map(|(score, r)| VectorRecord {
                id: r.id,
                vector: r.vector.clone(),
                metadata: r.metadata.clone(),
                score: Some(score),
            })
            .collect();

        Ok(VectorSearchResult { records, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_delete_collection() {
        let store = InMemoryVectorStore::new();
        let ctx = LsContext::with_session(LsId::new());
        let id = store
            .create_collection(ctx.clone(), "test", 3)
            .await
            .unwrap();
        assert!(store
            .search(ctx.clone(), id, vec![1.0; 3], 10)
            .await
            .is_ok());
        store.delete_collection(ctx.clone(), id).await.unwrap();
        assert!(store
            .search(ctx.clone(), id, vec![1.0; 3], 10)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_upsert_and_search() {
        let store = InMemoryVectorStore::new();
        let ctx = LsContext::with_session(LsId::new());
        let col_id = store
            .create_collection(ctx.clone(), "vectors", 2)
            .await
            .unwrap();
        let records = vec![
            VectorRecord {
                id: LsId::new(),
                vector: vec![1.0, 0.0],
                metadata: Value::String("A".into()),
                score: None,
            },
            VectorRecord {
                id: LsId::new(),
                vector: vec![0.0, 1.0],
                metadata: Value::String("B".into()),
                score: None,
            },
            VectorRecord {
                id: LsId::new(),
                vector: vec![1.0, 1.0],
                metadata: Value::String("C".into()),
                score: None,
            },
        ];
        store.upsert(ctx.clone(), col_id, records).await.unwrap();
        let result = store
            .search(ctx.clone(), col_id, vec![1.0, 0.0], 2)
            .await
            .unwrap();
        assert_eq!(result.records.len(), 2);
        assert_eq!(result.records[0].metadata, Value::String("A".into()));
        assert!(result.records[0].score.unwrap() > 0.99);
    }

    #[tokio::test]
    async fn test_dimension_mismatch() {
        let store = InMemoryVectorStore::new();
        let ctx = LsContext::with_session(LsId::new());
        let col_id = store
            .create_collection(ctx.clone(), "test", 3)
            .await
            .unwrap();
        let bad = vec![VectorRecord {
            id: LsId::new(),
            vector: vec![1.0, 0.0],
            metadata: Value::Null,
            score: None,
        }];
        assert!(store.upsert(ctx.clone(), col_id, bad).await.is_err());
    }

    #[test]
    fn test_cosine_similarity() {
        assert!(
            (InMemoryVectorStore::cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6
        );
        assert!(
            (InMemoryVectorStore::cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]) - 0.0).abs() < 1e-6
        );
    }
}
